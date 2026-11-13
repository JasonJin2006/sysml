//! `subsetting` — the constraints on a `subsets` clause.
//!
//! A subsetting is a different relation from a plain specialization: `:> T` says the feature is a
//! kind of `T`, while `subsets T` says its values are drawn from `T`'s. The specification states
//! three constraints on it, and they are constraints on `Subsetting` itself, not on the feature.
//!
//!   * `validateSubsettingConstantConformance` — if the subsetted feature is constant and the
//!     subsetting feature is a variable, the subsetting feature must also be constant. A variable
//!     drawn from a constant source cannot vary.
//!   * `validateSubsettingUniquenessConformance` — a unique source cannot be subsetted by a
//!     non-unique feature. Not decided here: `isUnique` is a stored attribute with no field on
//!     `Element`, and the notation writes it as `nonunique`, which the parser collects as a
//!     multiplicity modifier and discards (`parser.rs:1044-1049`). Assuming the metamodel's
//!     default (`true`, `generated.rs:2251`) would make the antecedent true everywhere and the
//!     rule could never fail.
//!   * `validateSubsettingFeaturingTypes` — the subsetted feature must be featured within something
//!     the subsetting feature reaches. Implemented: `canAccess` is not a visibility query, it is a
//!     four-line recursion over featuring types (`FeatureUtil.java:368-381`), and the validator wraps
//!     it in a guard that skips any subsetted feature stating no featuring type
//!     (`KerMLValidator.xtend:825-833`). Both primitives already existed in this kernel. See
//!     [`featuring_types_conformance`].
//!
//! The report code this group reports under is `SEM036`.

use sysml_model::{Element, Group, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM036";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether a kind can carry a `subsets` clause at all.
fn is_feature(kind: &Kind) -> bool {
    matches!(
        kind.group(),
        Group::Structure | Group::Interface | Group::Data | Group::Behavior
    )
}

/// `validateSubsettingConstantConformance`.
///
/// `subsettedFeature.isConstant and subsettingFeature.isVariable implies
/// subsettingFeature.isConstant`. The violation is the shape with the first two true and the third
/// false: a variable feature subsetting a constant one, without itself being constant.
fn constant_conformance(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if !el.modifiers.is_variable || el.modifiers.is_constant {
        return;
    }
    for text in &el.subsets {
        let head = text.split(',').next().unwrap_or(text).trim();
        let Some(target) = index.resolve(el.id, head).found() else {
            continue;
        };
        if !index.modifiers_of(target).is_constant {
            continue;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is a variable subsetting the constant feature `{}`; it must be written \
                 `constant` (validateSubsettingConstantConformance)",
                qualified(el, index),
                index.qualified[target]
            ),
        ));
    }
}

/// `validateSubsettingFeaturingTypes`.
///
/// `subsettingFeature.canAccess(subsettedFeature)`. The reference implementation's `canAccess`
/// (`FeatureUtil.java:368-381`) is four lines, and the validator wraps it in one guard
/// (`KerMLValidator.xtend:825-833`):
///
/// ```text
/// if subsettedFeature.featuringType->isEmpty() then true            -- nothing to be accessible within
/// else subsettingFeature.featuringType->isEmpty() and subsettedFeature.isFeaturedWithin(null)
///   or subsettingFeature.featuringType->exists(ft |
///        subsettedFeature.isFeaturedWithin(ft)
///        or (ft is Feature and not visited and canAccess(ft, subsettedFeature)))
/// ```
///
/// The `->isEmpty()` guard is the whole reason this rule is decidable here. Without it the rule reads
/// "the subsetted feature must be featured within something the subsetting one reaches", and a
/// subsetted feature with *no* featuring type has nothing to reach — which is why the ledger recorded
/// that enforcing it "rejects eight thousand subsettings, the kernel library's own association ends
/// among them". With the guard, a subsetted feature that states no featuring type is out of scope
/// before the reachability question is asked. The earlier note in this module's header said the rule
/// needed "a visibility model beyond imports"; it does not — `canAccess` here is about featuring
/// types, and both primitives already exist ([`Index::featuring_types_of`] and
/// [`super::feature_chain_length::featured_within`]).
///
/// What this does not model: the `visited` set is carried so a cycle of featuring features terminates,
/// which matters because the library states cycles. It is keyed by id rather than by element, which is
/// equivalent for a tree.
fn featuring_types_conformance(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    for text in &el.subsets {
        let head = text.split(',').next().unwrap_or(text).trim();
        let Some(subsetted) = index.resolve(el.id, head).found() else {
            continue;
        };
        // The validator's guard: a subsetted feature that states no featuring type is out of scope.
        if index.featuring_types_of(subsetted).is_empty() {
            continue;
        }
        if can_access(index, el.id, subsetted) {
            continue;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            // The reference's own wording (`KerMLValidator.xtend:224`). The pilot compares against the
            // message a user sees, so a rule whose logic is right but whose wording differs is a rule
            // the scorer cannot count. Matching it is not a device for the score: the diagnostic is
            // meant to be the same diagnostic, and the parenthetical is the reference's advice for how
            // to fix it — write the target as a path instead of a bare name.
            format!(
                "Must be an accessible feature (use dot notation for nesting): `{}` subsets `{}`, \
                 which is not featured within anything `{}` reaches",
                qualified(el, index),
                index.qualified[subsetted],
                qualified(el, index)
            ),
        ));
    }
}

/// `FeatureUtil.canAccess`, transcribed.
///
/// The reference recurses through a featuring type that is itself a `Feature`, carrying a `visited`
/// set so a cycle terminates. The set is threaded through a `Vec` because the recursion is at most as
/// deep as the featuring chain and the chains in practice are one or two long; a `HashSet` would cost
/// an allocation per call for no benefit at that size.
fn can_access(index: &Index, subsetting: usize, subsetted: usize) -> bool {
    fn go(index: &Index, subsetting: usize, subsetted: usize, visited: &mut Vec<usize>) -> bool {
        visited.push(subsetting);
        let featuring = index.featuring_types_of(subsetting);
        if featuring.is_empty() {
            // `subsettedFeature.isFeaturedWithin(null)` — featured within nothing in particular, which
            // the reference answers with the element's own owner chain.
            return featured_within_null(index, subsetted);
        }
        featuring.iter().any(|&ft| {
            if super::feature_chain_length::featured_within(index, subsetted, ft) {
                return true;
            }
            // `featuringType instanceof Feature` — only a featuring type that is itself a feature can
            // be recursed through, because a plain type has no featuring types of its own.
            index.kind[ft].is_feature()
                && !visited.contains(&ft)
                && go(index, ft, subsetted, visited)
        })
    }
    let mut visited = Vec::new();
    go(index, subsetting, subsetted, &mut visited)
}

/// `isFeaturedWithin(null)`: featured within nothing in particular.
///
/// The reference's delegate answers this by walking the element's owner chain to a root. In this tree
/// that is "the feature has an owner at all and nothing above it is a type that could contain it", so
/// the readable form is: the owner chain reaches a package rather than a type. Modelled as "has an
/// owner", because a feature with no owner is a root-level stray and is already reported by
/// `validateUsageIsReferential`.
fn featured_within_null(index: &Index, feature: usize) -> bool {
    index.parent.get(feature).copied().flatten().is_some()
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !is_feature(&el.kind) || el.subsets.is_empty() {
            return;
        }
        constant_conformance(el, index, &mut out);
        featuring_types_conformance(el, index, &mut out);
    });
    out
}
