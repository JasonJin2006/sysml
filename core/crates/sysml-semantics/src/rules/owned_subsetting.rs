//! `owned-subsetting` — a feature states at most one reference and at most one cross subsetting.
//!
//! Two of a feature's subsettings are singled out by the specification:
//!
//! ```text
//! part q references p;     // the feature *is* p, seen from here
//! part r crosses A;        // the feature reaches across A
//! ```
//!
//!   * `validateFeatureOwnedReferenceSubsetting` — at most one `references`.
//!   * `validateFeatureOwnedCrossSubsetting` — at most one `crosses`.
//!
//! Both say "this one", not "these": a second reference would leave the feature pointing two ways
//! with no rule for which wins, and the notation has no syntax for a set of references.
//!
//! # What it took to read
//!
//! The parser already produced `RelationOp::References` and `RelationOp::Crosses`; neither reached
//! the tree. They are now `Element::references` and `Element::crosses`, kept apart from `subsets`
//! because the bound is on these two kinds specifically — folding them into the general list would
//! have lost which is which.
//!
//! The report code this group reports under is `SEM056`.

use sysml_model::Element;
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM056";

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
        for (targets, clause, rule) in [
            (
                &el.references,
                "references",
                "validateFeatureOwnedReferenceSubsetting",
            ),
            (
                &el.crosses,
                "crosses",
                "validateFeatureOwnedCrossSubsetting",
            ),
        ] {
            if targets.len() <= 1 {
                continue;
            }
            // One report per clause past the first, each naming that clause. The OCL is one
            // condition per feature (`ownedSubsetting->selectByKind(ReferenceSubsetting)->size() <=
            // 1`, `generated.rs:7224`), but every clause past the first is a separate owned
            // subsetting the model should not have written, and the reference anchors a report on
            // each (`ReferenceSubsetting_Invalid.kerml.xt:33` asserts at `c` and at `d`). Reporting
            // as the reference does was decided for this project: the pilot is the measure, and a
            // reader told which clauses are redundant can delete them.
            for extra in targets.iter().skip(1) {
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` states {} `{clause}` clauses; `{extra}` is one more than a feature \
                         may have ({rule})",
                        qualified(el, index),
                        targets.len()
                    ),
                ));
            }
        }
    });
    out
}
