//! `include-target` — `include` names a use case.
//!
//! `validateIncludeUseCaseUsageReference`: `referencedFeatureTarget() <> null implies
//! referencedFeatureTarget().oclIsKindOf(UseCaseUsage)`. An include says "this case performs that
//! one too", so the thing named must be a case a caller could enter.
//!
//! ```text
//! use case Park;
//! use case Driving {
//!     include Park;          // legal
//! }
//! part def Chassis;
//! use case Broken {
//!     include Chassis;       // rejected: a part definition is not a case
//! }
//! ```
//!
//! # What it took to read
//!
//! `use case X` lowered to `Kind::UseCaseDef` whether or not `def` followed, so the tree held no
//! use-case *usage* at all and the rule had nothing to accept. Splitting them was the fix.
//!
//! The include statement itself borrows the use-case kind — it is a member of one — so
//! `Element::clause` marks it as `include`. Without that marker a later rule about use cases would
//! count every include as one.
//!
//! An unresolved target is skipped: a dangling name is a reference problem.
//!
//! The report code this group reports under is `SEM076`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM076";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether a kind is a use case a caller can enter.
fn is_use_case(kind: &Kind) -> bool {
    matches!(kind, Kind::UseCaseUsage)
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.clause.as_deref() != Some("include") {
            return;
        }
        // `include use case i : T;` makes a usage typed by `T`. It refers to no case, so there is no
        // reference to check here: the typing is judged by the rule for use case usages.
        if el.kind == Kind::UseCaseUsage {
            return;
        }
        let Some(target) = el.type_ref.as_deref() else {
            return;
        };
        let head = target.split(',').next().unwrap_or(target).trim();
        let Resolution::Found(found) = index.resolve(el.id, head) else {
            return;
        };
        if is_use_case(&index.kind[found]) {
            return;
        }
        let found_kind = index.kind[found].keyword();
        let found_name = index
            .qualified
            .get(found)
            .cloned()
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| head.to_string());
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`include` names `{found_name}`, a {found_kind}; it must name a use case \
                 (validateIncludeUseCaseUsageReference)"
            ),
        ));
    });
    out
}
