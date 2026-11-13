//! `redefinition-direction` — a redefinition keeps its target's direction.
//!
//! `validateRedefinitionDirectionConformance`: where the redefined feature has a direction, the
//! redefinition must have the same one. A direction says which way a feature faces at a boundary —
//! `in` is supplied to the owner, `out` is produced by it — and a redefinition that flips it
//! describes a boundary that carries the same thing both ways.
//!
//! ```text
//! class C { in z; }
//! class D specializes C {
//!     out :>> z;      // rejected: `z` comes in, this one goes out
//! }
//! ```
//!
//! # Conjugation flips the expectation
//!
//! `class C1 conjugates C` is `C` with every direction turned around, so a redefinition inherited
//! through `C1` must match the *conjugated* direction. The pilot implementation's own test models
//! spell this case out (`Redefinition_DirectionConformance_invalid.kerml`), and they are what this
//! rule is checked against.
//!
//! `inout` accepts either written direction — it is the union — and a target with no direction
//! says nothing to conform to.
//!
//! # What is covered, and what is not
//!
//! The pilot implementation's `Redefinition_DirectionConformance_invalid.kerml` states five
//! violations. Four of them are covered here: the two `class`-to-`class` redefinitions where the
//! target is a direct inherited member, the nested one (`A::b { in x1 }` where `b : B` redefines
//! `B`'s `out x1`), and the conjugated one (`class D1 specializes C1` where `C1 conjugates C`).
//!
//! The fifth is `B specializes A { out x1; }` at line 35, and it is left alone for a reason that
//! took reading the reference implementation to establish. `B` inherits no `x1`, so no
//! name-collision redefinition exists; the reference reaches this line through a different
//! mechanism entirely — it pairs a behaviour's **parameters by position** with the parameters of
//! each general type. `B`'s parameters are `[x1, y1]` and `A`'s are `[x, y]`, so `B::x1` implicitly
//! redefines `A::x` (`in`, incompatible with `out`) while `B::y1` redefines `A::y` (`inout`,
//! compatible — which is why the pilot asserts nothing for line 36).
//!
//! # Where that pairing actually lives
//!
//! An earlier version of this comment cited `FeatureAdapter.getParameterRelevantFeatures`. That
//! method is real but is about the *result* parameter — its `@satisfies` tag is
//! `checkFeatureResultRedefinition` (`FeatureAdapter.java:709`) — so citing it was wrong. The index
//! arithmetic is in `addRedefinitions` (`:634-651`), whose own doc comment states the rule
//! (`:629-632`: "paired with relevant Features in the same position in Generalizations of the owning
//! Type") and whose body does `int i = getRelevantFeatures(type).indexOf(target)` then
//! `features.get(i)` (`:638-643`). For a parameter, `getRelevantParameters` supplies the list as
//! `TypeUtil.getAllParametersOf(type)` (`:728-733`) and carries
//! `@satisfies checkFeatureParameterRedefinition` (`:726`).
//!
//! That positional pairing is **not in the specification**, and the check is stronger than the name
//! search the first version relied on. Name matching cannot settle it: every `checkFeature*` tag in
//! the reference maps to no `validate*`/`derive*` name, including ones whose stated counterpart
//! plainly exists, so an absent name proves nothing. What does settle it is the OCL itself —
//! `ownedRedefinition = ownedSubsetting->selectByKind(Redefinition)`
//! (`deriveFeatureOwnedRedefinition`), which means a redefinition exists only where one was written
//! or where an *implicit general type* puts one there; no stated derivation builds one from an index;
//! and the only two stated rules with `ParameterRedefinition` in the name are
//! `validateInvocationExpressionParameterRedefinition` and its duplicate-parameter sibling, both
//! `metaclass: InvocationExpression`.
//!
//! So imitating it would put a rule in this kernel that no normative sentence backs, and it would do
//! so for every behaviour in every model — a far larger surface than the one assertion it buys.
//!
//! The half the rule can name — the nested `in x1` that redefines `B`'s `out x1` — is already
//! reported at line 30.
//!
//! # How it reads
//!
//! The target is looked up in what the owner *inherits* (`Index::inherited_member_of`): `:>> x`
//! names the `x` the owner inherits, and resolving from the owner or from the redefinition finds
//! the wrong thing in both cases.
//!
//! Whether the expectation is flipped is decided by walking from the owner to the target and
//! counting conjugation edges: an odd number turns the direction around, two cancel. The walk
//! follows specializations and conjugations both, because a conjugation is not a specialization.
//!
//! The report code this group reports under is `SEM081`.

use sysml_model::{Direction, Element};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM081";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether two directions are compatible under the rule.
///
/// `inout` is the union of the other two, so it conforms to either. A missing direction says
/// nothing and conforms to everything.
fn conforms(redefined: Option<Direction>, redefining: Option<Direction>, flipped: bool) -> bool {
    let Some(expected) = redefined else {
        return true;
    };
    let Some(actual) = redefining else {
        return true;
    };
    let expected = if flipped {
        match expected {
            Direction::In => Direction::Out,
            Direction::Out => Direction::In,
            Direction::InOut => Direction::InOut,
        }
    } else {
        expected
    };
    matches!(expected, Direction::InOut) || matches!(actual, Direction::InOut) || expected == actual
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        // An element with neither a `:>>` clause nor an inherited name to repeat has nothing to
        // check. The name is a candidate only when the owner inherits something by it, which is
        // cheaper to ask here than to look up twice.
        if el.redefines.is_empty()
            && (el.name.is_empty() || !el.kind.is_feature())
            && el.direction.is_none()
        {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        // A declaration that repeats an inherited member's name *is* a redefinition — the metamodel
        // derives `ownedRedefinition` from the name collision, not from the `:>>` keyword — so the
        // pilot model marks `out x1;` inside `behavior B specializes A` as a direction violation
        // even though `x1` writes no clause at all. Both spellings are collected here; an explicit
        // `:>>` wins because it names its target.
        let mut clauses: Vec<String> = el.redefines.clone();
        if !el.name.is_empty() && clauses.is_empty() {
            if index.inherited_member_of(owner, &el.name).is_some() {
                clauses.push(el.name.clone());
            }
        }
        // With neither a clause nor an inherited name, a parameter still redefines the parameter at
        // its position in each general type: `behavior B specializes A { out x1; }` redefines
        // `A::x`, and the pilot reports the direction there
        // (`Redefinition_DirectionConformance_invalid.kerml.xt:35`).
        let mut targets: Vec<(String, usize)> = Vec::new();
        for text in &clauses {
            let head = text.split(',').next().unwrap_or(text).trim();
            if let Some(target) = index.inherited_member_of(owner, head) {
                targets.push((head.to_string(), target));
            }
        }
        if clauses.is_empty() {
            for target in index.positional_parameter_targets(el.id) {
                targets.push((index.name[target].clone(), target));
            }
        }
        for (head, target) in targets {
            let head = head.as_str();
            // A conjugated owner turns every inherited direction around.
            let flipped = is_conjugated_inheritance(index, owner, target);
            let redefined = index.direction_of(target);
            if conforms(redefined, el.direction, flipped) {
                continue;
            }
            let target_owner = index
                .parent
                .get(target)
                .copied()
                .flatten()
                .map(|p| index.name[p].clone())
                .unwrap_or_default();
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` is `{:?}` but redefines `{head}` from `{target_owner}`, which is `{:?}`{} \
                     (validateRedefinitionDirectionConformance)",
                    qualified(el, index),
                    el.direction,
                    redefined,
                    if flipped { " through a conjugation" } else { "" }
                ),
            ));
        }
    });
    out
}

/// Whether the redefinition reaches the target through a conjugated type.
///
/// `class C1 conjugates C` means `C1`'s inherited directions are `C`'s turned around. The turn can
/// happen anywhere on the path: the pilot model's case has the *owner* specialize a conjugated type
/// (`class D1 specializes C1` where `C1 conjugates C`), so asking only whether the owner conjugates
/// the target's owner answers "no" for the very model the rule is checked against.
///
/// The walk therefore counts conjugations along the path and reports an odd number. Two
/// conjugations cancel, which is why the parity is tracked rather than a flag set once.
fn is_conjugated_inheritance(index: &Index, owner: usize, target: usize) -> bool {
    let Some(target_owner) = index.parent.get(target).copied().flatten() else {
        return false;
    };
    let mut frontier = vec![(owner, false)];
    let mut seen: std::collections::HashSet<(usize, bool)> = frontier.iter().copied().collect();
    while let Some((node, flipped)) = frontier.pop() {
        if node == target_owner && flipped {
            return true;
        }
        let conjugates = index.conjugate_edges(node);
        // The walk takes both kinds of edge: a conjugation is not a specialization, so
        // `supertypes_of` does not carry it, and a path that only followed specializations would
        // stop at the conjugated type instead of arriving at what it conjugates.
        let mut edges = index.supertypes_of_public(node);
        for conjugated in &conjugates {
            if !edges.contains(conjugated) {
                edges.push(*conjugated);
            }
        }
        for sup in edges {
            let next = flipped != conjugates.contains(&sup);
            if seen.insert((sup, next)) {
                frontier.push((sup, next));
            }
        }
    }
    false
}
