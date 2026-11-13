//! `derived` — the properties the specification derives, computed once instead of per rule.
//!
//! Each function here answers a question the OCL asks by a *derived* property: `inputParameters`,
//! `featureTarget`, `chainingFeature`, `crossFeature`, `result`, `referencedFeatureTarget`. Without
//! them, every rule that needed one re-derived it from whatever child shapes happened to be
//! nearby, and the readings drifted — `case_membership` decided "is this an input parameter" one
//! way and a loop rule would have decided it another.
//!
//! Everything takes an id and answers with ids, so a caller composes two properties without
//! walking the tree twice: `feature_target(index, id)` is what
//! `referenced_feature_target(index, id)` follows through, and both are what the rules read.
//!
//! # What is not here, and why
//!
//! `result` for an *expression* needs the expression to be an element; the tree carries
//! expressions as [`Expr`](sysml_model::Expr) values inside a declaration, so a
//! `FeatureReferenceExpression` has no id to ask about. The two properties below that name
//! `result`-adjacent things read the return *parameter* of a declaration that owns one, which is
//! what the notation writes (`return : T = v`).
//!
//! `chainingFeature` is the `chains` clause the declaration wrote. A dotted type path (`a.b` used
//! as a type) is a different thing and is deliberately not read as a chain: the two share
//! `Element::type_ref`, which is the projection defect `feature_chain_length` already records.

use sysml_model::{Direction, Kind};
use sysml_resolve::{Index, Resolution};

/// The input parameters of a declaration — `deriveTypeInput`.
///
/// "A feature whose direction is `in` or `inout`" (sysmlv2-model `generated.rs:8111-8115`), plus
/// the subject: `SubjectMembership` is a `ParameterMembership` (`generated.rs:1755`) whose
/// `parameterDirection()` is `_'in'` (`:9294-9297`), and the tree does not derive that direction
/// onto the subject element. A `return` parameter is an output and is not in this list.
///
/// The list is what `ActionUsage::inputParameters()->size()` counts. The notation's *implicit*
/// inputs — a loop's sequence and loop variable, an `if`'s condition — are not elements yet, so a
/// count over this list is a count of what the model wrote (Layer 3 adds the rest).
pub fn input_parameters(index: &Index, id: usize) -> Vec<usize> {
    let Some(children) = index.children.get(id) else {
        return Vec::new();
    };
    children
        .iter()
        .copied()
        .filter(|&child| {
            index.kind[child] == Kind::Subject
                || matches!(
                    index.direction_of(child),
                    Some(Direction::In) | Some(Direction::InOut)
                )
        })
        .collect()
}

/// The input parameters of a declaration, counting the ones it inherits.
///
/// The metamodel's `ActionUsage::inputParameters()` is `input->select(f | f.owner = self)`
/// (`generated.rs:9816-9818`), which counts owned inputs only. This list is wider: the local
/// parameters plus inherited ones. An `accept a : T` writes its payload `a`, and its receiver is
/// inherited from `AcceptAction` in the library (`Transfers.kerml:254-265`: `inout feature payload`,
/// `in feature receiver`). Counting only the local members would leave the receiver out of every
/// accept action.
///
/// The walk follows the implicit library base and any written supertypes, nearest first, and stops
/// at a cycle: `Parts::Part` and `Ports::Port` reach each other through the library, and an
/// unguarded walk never terminates.
pub fn input_parameters_with_inherited(index: &Index, id: usize) -> Vec<usize> {
    let mut out = input_parameters(index, id);
    let mut seen = std::collections::HashSet::new();
    seen.insert(id);
    let mut frontier = index.supertypes_of_public(id);
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for owner in frontier {
            if !seen.insert(owner) {
                continue;
            }
            for param in input_parameters(index, owner) {
                // A redefinition in the subtype replaces the inherited one; the name decides.
                if !out.iter().any(|&p| index.name[p] == index.name[param]) {
                    out.push(param);
                }
            }
            next.extend(index.supertypes_of_public(owner));
        }
        frontier = next;
    }
    out
}

/// The return parameter of a declaration — `Function::result` / `Expression::result`, read where the
/// notation writes it.
///
/// `return : T = v` lowers to [`Kind::Parameter`] with `clause = "return"`, which is the marker
/// [`sysml-syntax`]'s lowering sets for a `ReturnParameterMembership`. A declaration with no
/// `return` has no result, and the OCL's `result.owningType = self` has nothing to be true of.
pub fn result_parameter(index: &Index, id: usize) -> Option<usize> {
    index
        .children
        .get(id)?
        .iter()
        .copied()
        .find(|&child| index.clause_of(child) == Some("return"))
}

/// The chaining features of a feature — `deriveFeatureChainingFeature`, `chainingFeature =
/// ownedFeatureChaining.chainingFeature`.
///
/// The notation is `chains a.b`; the tree keeps the targets as authored text, and this resolves
/// them from the owning scope — through [`Index::resolve_path`] when the text is a dotted path,
/// because `chains c.b` names the `b` inside `c`'s type. Unresolved entries are dropped rather
/// than reported: the reference pass reports them, and a chain with a hole would make every count
/// below wrong in an unknown direction.
pub fn chaining_features(index: &Index, id: usize) -> Vec<usize> {
    let scope = index.clause_scope_of(id);
    let mut out = Vec::new();
    for text in index.chains_of(id) {
        let head = text.split(',').next().unwrap_or(text).trim();
        if head.is_empty() {
            continue;
        }
        // `chains a.b.c` names three features in order, and the specification's `chainingFeature`
        // is the sequence of them — not the last one. Resolving the whole path answers "where does
        // this chain end", which is `featureTarget`, and reading that as the chain made
        // `validateFeatureChainingFeatureConformance` compare a feature with itself: the rule needs
        // each step, and `isFeaturedWithin` is a statement about consecutive steps.
        let mut current = None;
        for segment in head.split('.').map(str::trim).filter(|s| !s.is_empty()) {
            let found = match current {
                None => index.resolve(scope, segment),
                Some(from) => {
                    let name = sysml_model::NameRef::new(segment, index.line[id]);
                    index.resolve_path(from, &name)
                }
            };
            match found {
                Resolution::Found(target) => {
                    out.push(target);
                    current = Some(target);
                }
                // A hole makes every later step unanswerable, and a chain with a hole would make
                // the counts below wrong in an unknown direction. The reference pass reports the
                // unresolved name itself.
                _ => break,
            }
        }
    }
    out
}

/// `Feature::featureTarget` — `if chainingFeature->isEmpty() then self else
/// chainingFeature->last() endif` (`deriveFeatureFeatureTarget`).
pub fn feature_target(index: &Index, id: usize) -> usize {
    chaining_features(index, id).pop().unwrap_or(id)
}

/// `Feature::type` — `deriveFeatureType`, the only derived property three enforced rules read as a
/// set (`validateAssociationEndTypes`, `validateFeatureCrossFeatureType`,
/// `validateMetadataFeatureMetaclass`).
///
/// ```text
/// type = let types : OrderedSet(Types) = OrderedSet{self}->
///            closure(typingFeatures()).typing.type->asOrderedSet() in
///        types->reject(t1 | types->exist(t2 | t2 <> t1 and t2.specializes(t1)))
/// ```
///
/// Two facts are easy to lose, and both are lost in a projection onto the written `: T` clause:
///
///   * the union is taken over the **closure** of `typingFeatures()`, so a feature that writes no
///     type still has one when it subsets a typed feature — `end b subsets a` where `a : C` is of
///     type `C`;
///   * "with all redundant supertypes removed": `p : A, B` where `B :> A` is of type `{B}`, not
///     `{A, B}`. Reading the written clause makes one type look like two.
///
/// `typingFeatures()` is `subsetting->reject(oclIsKindOf(CrossSubsetting)).subsettedFeatures`, so
/// the edges followed here are the three KerML kinds of subsetting this tree records separately —
/// plain subsetting, reference subsetting, and redefinition — and not `crosses`, which the body
/// rejects explicitly.
///
/// A redeclaration inside a subtype states no clause at all: `end x : C2;` in an association that
/// already has an `end x : C1;` implicitly redefines it, and the semantic layer materialises that
/// redefinition before any rule runs. This tree does not, so the inherited member of the same name
/// is followed by hand — the case `validateAssociationEndTypes` was written around
/// (`AssociationTest_EndTypes_invalid.kerml.xt:33`).
///
/// Not modelled: conjugation. `~A` would take its type from the conjugator's original type, and the
/// tree has no conjugated type to ask; no rule among the three reads a conjugated feature's type.
pub fn types(index: &Index, id: usize) -> Vec<usize> {
    // `closure(typingFeatures())`: the feature itself, then everything reachable by subsetting.
    let mut reachable = vec![id];
    let mut seen = std::collections::HashSet::from([id]);
    let mut cursor = 0;
    while cursor < reachable.len() {
        let current = reachable[cursor];
        cursor += 1;
        for next in typing_features(index, current) {
            if seen.insert(next) {
                reachable.push(next);
            }
        }
    }

    let mut found: Vec<usize> = Vec::new();
    for feature in reachable {
        for text in index.types_of(feature) {
            // A slot can hold a comma list when the parser recorded the clause whole; `A, B` is two
            // types either way, and `validateAssociationEndTypes` counts them.
            for part in text.split(',') {
                let head = part.trim();
                if head.is_empty() {
                    continue;
                }
                if let Resolution::Found(target) = index.resolve(feature, head) {
                    if !found.contains(&target) {
                        found.push(target);
                    }
                }
            }
        }
    }

    // "with all redundant supertypes removed": a type another one in the set specializes is not a
    // separate type of this feature.
    found
        .iter()
        .copied()
        .filter(|&one| {
            !found
                .iter()
                .any(|&other| other != one && index.is_subtype(other, one))
        })
        .collect()
}

/// `typingFeatures()` — the features whose types a feature inherits.
///
/// The three relations this tree keeps apart are all `Subsetting` subclasses in KerML
/// (`OwnedSubsetting`, `ReferenceSubsetting`, `RedefinitionSubsetting`), which is why all three are
/// followed. `crosses` is the fourth and is excluded by the derivation's own `reject`.
///
/// The last chaining feature is appended when it is not already among them, because a chain's tail
/// is what the feature denotes: `p chains a.b` is typed like `b`.
fn typing_features(index: &Index, id: usize) -> Vec<usize> {
    let scope = index.clause_scope_of(id);
    let mut out = Vec::new();
    let clause = index
        .subsets_of(id)
        .iter()
        .chain(index.redefines_of(id))
        .chain(index.references_of(id));
    for text in clause {
        let head = text.split(',').next().unwrap_or(text).trim();
        if head.is_empty() {
            continue;
        }
        if let Resolution::Found(target) = index.resolve(scope, head) {
            if target != id && !out.contains(&target) {
                out.push(target);
            }
        }
    }
    for target in index
        .implicit_subsetting_targets(id)
        .into_iter()
        .chain(index.implicit_redefinition_targets(id))
    {
        if target != id && !out.contains(&target) {
            out.push(target);
        }
    }
    // The implicit redefinition a redeclaration creates without writing `:>>`.
    if let Some(owner) = index.parent(id) {
        // An owned crossing inherits its type from the owning end, and reuses the outer closure's cycle guard.
        if owned_cross_feature(index, owner) == Some(id) && !out.contains(&owner) {
            out.push(owner);
        }
        let name = &index.name[id];
        if !name.is_empty() {
            if let Some(inherited) = index.inherited_member_of(owner, name) {
                if inherited != id && !out.contains(&inherited) {
                    out.push(inherited);
                }
            }
        }
    }
    if let Some(last) = chaining_features(index, id).last().copied() {
        if !out.contains(&last) {
            out.push(last);
        }
    }
    out
}

/// The first owned crossing feature, which is owned through an OwningMembership rather than
/// a FeatureMembership. Prefix cross names and `member feature` share this representation.
pub fn owned_cross_feature(index: &Index, id: usize) -> Option<usize> {
    if !index.is_end_of(id) {
        return None;
    }
    let owner = index.parent(id)?;
    if !index.kind[owner].is_type() {
        return None;
    }
    index.children.get(id)?.iter().copied().find(|&child| {
        index.kind[child].is_feature()
            && index.membership_of(child) == sysml_model::Membership::Owning
    })
}

/// `Feature::crossFeature` — the second chaining feature of the crossed feature
/// (`deriveFeatureCrossFeature`).
///
/// `OwnedCrossSubsetting` is either `crossedFeature = [Feature | QualifiedName]` or
/// `ownedRelatedElement += OwnedFeatureChain` (`KerML.xtext:703-707`), and the specification's
/// sentence is "the second `chaining_feature` of the `crossed_feature`". Reading the clause as a
/// dotted access asked the wrong question: `crosses y.b` does not mean "`b` is a member of `y`'s
/// type", it means the crossed feature *is* the chain `y` then `b`, so `crossFeature` is `b`.
///
/// ```text
/// end y : C2 crosses y.b;      // crossed feature is the chain y.b; crossFeature is b
/// end z : C2 crosses w;        // a single name, no chain, so no crossFeature
/// ```
///
/// The first segment is looked up in scope and each later one as a member of what the previous
/// denotes — the same walk [`chaining_features`] does, because the notation is the same one. A
/// single name has no second element, so it answers `None`, which is what the OCL's
/// `crossFeature <> null` guard expects.
pub fn cross_feature(index: &Index, id: usize) -> Option<usize> {
    let scope = index.clause_scope_of(id);
    let crossed = index.crosses_of(id).first()?;
    let mut segments = crossed
        .split(',')
        .next()
        .unwrap_or(crossed)
        .split('.')
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let first = segments.next()?;
    if segments.next().is_none() {
        return None;
    }
    let Resolution::Found(crossed_feature) = index.resolve(scope, first) else {
        return None;
    };
    let mut current = crossed_feature;
    let mut second = None;
    for segment in crossed
        .split('.')
        .map(str::trim)
        .skip(1)
        .filter(|s| !s.is_empty())
    {
        // A `.` access asks what the previous step denotes *provides*: `resolve_path` with one
        // segment asks instead what that name means in the previous step's scope, which walks
        // outward and binds a feature the crossed end says nothing about. The reference pass
        // reports the same path unresolved, so the two would disagree about one text.
        let step = index.member_of_access(current, segment);
        let Resolution::Found(next) = step else {
            return None;
        };
        if second.is_none() {
            second = Some(next);
        }
        current = next;
    }
    second
}

/// What a declaration's reference subsetting names — `referencedFeatureTarget()`, followed through
/// [`feature_target`].
///
/// `satisfy r2 by p` reaches the tree as a relationship whose target end is `r2` and whose source
/// end is `p` (`sysml-syntax/src/lower.rs:788-795`: `to = requirement`, `from = by`). The
/// referenced feature is the one the declaration points *at*, so it is the target end. A
/// declaration with no target end references nothing — `satisfy req : R1` names its *type* rather
/// than a feature, and reading the type as the referenced feature would report a legal model
/// against a rule whose own `referencedFeatureTarget() <> null` antecedent excludes it.
pub fn referenced_feature_target(index: &Index, id: usize) -> Option<usize> {
    let target = index.end_text(id, false)?;
    let head = target.split(',').next().unwrap_or(target).trim();
    if head.is_empty() {
        return None;
    }
    match index.resolve(index.clause_scope_of(id), head) {
        Resolution::Found(found) if found != id => Some(feature_target(index, found)),
        _ => None,
    }
}
