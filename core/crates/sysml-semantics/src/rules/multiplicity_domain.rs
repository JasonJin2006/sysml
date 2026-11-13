//! `multiplicity-domain` — a multiplicity's featuring types follow the feature or classifier that
//! owns it.
//!
//! `validateFeatureMultiplicityDomain`: a feature with a multiplicity has the same featuring types
//! as that multiplicity, compared as sets. `validateClassifierMultiplicityDomain`: a classifier's
//! multiplicity has no featuring type, so its domain is implicitly `Base::Anything`.
//!
//! ```text
//! classifier C {
//!     multiplicity M [0..1] { featuring of M by X; }   // rejected: a classifier's multiplicity
//! }                                                    // has no featuring type
//!
//! class C2 {
//!     feature f {
//!         multiplicity M [0..1] { featuring of M by X; }  // rejected: M has {C2, X}, f has {C2}
//!     }
//! }
//! ```
//!
//! The featuring types are `Index::featuring_types_of`: the `featuring … by` statements that name
//! the element, plus the implicit ones. The reference reports the two rules with the messages
//! "Multiplicity must not have a featuring type" and "Multiplicity must have same featuring types
//! as it feature" (`KerMLValidator.xtend:576-580`, `:611-615`).
//!
//! The report codes are `SEM120` for the classifier rule and `SEM119` for the feature rule.

use std::collections::HashSet;

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const FEATURE_CODE: &str = "SEM119";
const CLASSIFIER_CODE: &str = "SEM120";

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        let owner = el.id;
        let owner_kind = &index.kind[owner];
        let children = index.children.get(owner).map(Vec::as_slice).unwrap_or(&[]);
        for &child in children {
            if index.kind[child] != Kind::MultiplicityDef {
                continue;
            }
            let domain = index.featuring_types_of(child);
            if owner_kind.is_feature() {
                let owner_domain = index.featuring_types_of(owner);
                let domain_set: HashSet<usize> = domain.iter().copied().collect();
                let owner_set: HashSet<usize> = owner_domain.iter().copied().collect();
                if domain_set != owner_set {
                    out.push(Violation::new(
                        FEATURE_CODE,
                        el,
                        index.qualified[owner].clone(),
                        "Multiplicity must have same featuring types as it feature",
                    ));
                }
            } else if owner_kind.is_type() && !domain.is_empty() {
                out.push(Violation::new(
                    CLASSIFIER_CODE,
                    el,
                    index.qualified[owner].clone(),
                    "Multiplicity must not have a featuring type",
                ));
            }
        }
    });
    out
}
