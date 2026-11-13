//! `feature-chain-length` — a chain passes through at least two features.
//!
//! `validateFeatureChainingFeatureNotOne`: `chainingFeature->size() <> 1`. A chain is a path from
//! one feature to another through the features in between; a path with a single feature on it
//! names that feature and nothing more, which is what a plain type reference already does.
//!
//! ```text
//! feature a : A;
//! feature two chains a.b;      // legal: a path through two features
//! feature one chains a;        // rejected: that is just `a`
//! ```
//!
//! # What it took to read
//!
//! `chains` had no arm in the relation parser, so the word and its path fell into the body as
//! separate members and the feature stated no chain at all. `Element::chains` holds them now.
//!
//! # What is not covered
//!
//! `validateFeatureChainingFeaturesNotSelf` shares the tree and is reported here. The conformance
//! rule below reads the chain resolved as a sequence of features rather than as authored text,
//! which is why it needed [`crate::derived::chaining_features`].
//!
//! # The other half
//!
//! `validateFeatureChainingFeaturesNotSelf` shares the tree: a chain names the features it passes
//! through, so a segment equal to the feature's own name is a self-reference. Both rules report
//! under `SEM082` because they read the same clause.
//!
//! The report code this group reports under is `SEM082`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM082";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind == Kind::Transition {
            // A succession's ends are separate; `chains` on one is not the same construct.
            return;
        }
        // validateFeatureChainingFeaturesNotSelf: `chainingFeature->excludes(self)`. A chain names
        // the features it passes *through*, so a segment equal to the feature's own name says the
        // feature reaches itself. The pilot implementation's `feature h chains f.h;` is the case.
        // A chain already reported for passing through itself is not reported again for the
        // relation between its steps: the pilot asserts one error at `feature h chains f.h;`, and
        // two diagnostics for one clause would be two answers to the same question.
        let mut reported = false;
        for entry in &el.chains {
            for segment in entry.split('.') {
                if segment.trim() == el.name && !el.name.is_empty() {
                    out.push(Violation::new(
                        CODE,
                        el,
                        qualified(el, index),
                        format!(
                            "`{}` chains `{entry}`, which passes through itself (validateFeatureChainingFeaturesNotSelf)",
                            qualified(el, index)
                        ),
                    ));
                    reported = true;
                    break;
                }
            }
        }
        // validateFeatureChainingFeatureConformance: each step must be featured within the one
        // before it. `chaining_features` resolves every segment, and when the previous segment has
        // no type it falls back to the enclosing scope — so a chain can resolve and still name a
        // feature the step before it does not provide.
        //
        //   part def W;              // W declares no `m`
        //   attribute m : Integer;   // ... but the package does
        //   part b : W;
        //   part x chains b.m;       // resolves to the package's `m`, which `b` does not feature
        //
        // The first step is not judged: there is nothing before it to be featured within.
        let chain = crate::derived::chaining_features(index, el.id);
        if reported {
            return;
        }
        for step in chain.windows(2) {
            if featured_within(index, step[1], step[0]) {
                continue;
            }
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` chains `{}`, and `{}` is not featured within `{}` \
                     (validateFeatureChainingFeatureConformance)",
                    qualified(el, index),
                    el.chains.join(", "),
                    index.qualified[step[1]],
                    index.qualified[step[0]],
                ),
            ));
            break;
        }
        // One entry in `chains`, and that entry is a single name: `chains a.b` writes one entry
        // whose text is a path of two, and passes. `chains a` is one entry with nothing after it.
        if el.chains.len() != 1 {
            return;
        }
        let only = el.chains[0].trim();
        if only.is_empty() || only.contains('.') {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` chains `{}`, a single feature; a chain passes through at least two \
                 (validateFeatureChainingFeatureNotOne)",
                qualified(el, index),
                el.chains[0]
            ),
        ));
    });
    out
}

/// Whether `feature` is featured within `container` — `Feature::isFeaturedWithin`.
///
/// The relation is "the container owns this feature, or the container specializes a type that does".
/// A usage carries its type in the same slot a definition carries its supertypes
/// (`supertypes_of_public` reads both), so one walk answers for either shape.
///
/// # The direction, which was wrong
///
/// The question is whether `container` reaches the owner, so the subtype test is
/// `is_subtype(container, owner)` — "is the container a subtype of the type that owns the feature".
/// The previous version walked the container's supertypes and asked `is_subtype(owner, t)`, which
/// reads as "is the owner a subtype of one of the container's supertypes". Every `part def` in this
/// kernel specializes the library's `Parts::Part`, so `Parts::Part` is always among a container's
/// supertypes and `is_subtype(owner, Parts::Part)` is always true — the predicate answered `true` for
/// any two part definitions. Measured before the fix: `featured_within(A::a, B)` returned `true` with
/// `is_subtype(A, B) == false`, because `super_of(B) == [Parts::Part]` and `A` specializes it too
/// (trace it with `SYSML_TRACE_SUBSETS`).
///
/// Walking the container's own supertypes is still needed, because a feature owned by `A` is featured
/// within `B` when `B :> A` — that is the case `is_subtype(container, owner)` answers directly, since
/// `A` is among `B`'s supertypes. So the test is a single direction, not a search over both.
pub(crate) fn featured_within(index: &Index, feature: usize, container: usize) -> bool {
    let Some(owner) = index.parent.get(feature).copied().flatten() else {
        return false;
    };
    if owner == container {
        return true;
    }
    // `container` reaches `owner` when it is a subtype of it: `B :> A` puts `A` in `B`'s closure.
    index.is_subtype(container, owner)
}
