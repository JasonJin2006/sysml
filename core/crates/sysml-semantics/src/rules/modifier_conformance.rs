//! `modifier-conformance` — a redefinition keeps the end-ness and constancy it inherits.
//!
//! Two rules with one shape: a modifier that the redefined feature carries must appear on the
//! redefinition too.
//!
//!   * `validateRedefinitionEndConformance` — `redefinedFeature.isEnd implies
//!     redefiningFeature.isEnd`. An end is one side of a connector; a redefinition that drops the
//!     `end` is no longer a side, so the connector would lose the thing it was attached to.
//!   * `validateSubsettingConstantConformance` — `subsettedFeature.isConstant and
//!     subsettingFeature.isVariable` is a contradiction. A constant is a value fixed at its
//!     declaration; a subset of it cannot be one that changes.
//!
//! ```text
//! part def Holder {
//!     end part side : Frame;
//!     part notAnEnd :> side;          // rejected: `side` is an end
//!
//!     constant attribute limit = 10;
//!     var attribute loose :> limit;   // rejected: `limit` is constant
//! }
//! ```
//!
//! # How it reads
//!
//! `Element::redefines` holds the target of `:>>`; `:>` reaches `Element::supertypes` (the
//! lowering puts an inherited feature there, whether the notation spelled it `specializes` or
//! `subsets`), and an explicit `subsets` clause is in `Element::subsets`. All are authored text,
//! resolved from the element's own scope. An unresolved target
//! is skipped — a dangling name is a reference problem, and judging its modifiers would guess.
//!
//! The report code this group reports under is `SEM074`.

use sysml_model::Element;
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM074";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

fn bare(name: &str) -> &str {
    let mut text = name.trim();
    if let Some(rest) = text.strip_prefix('[') {
        if let Some(close) = rest.find(']') {
            text = rest[close + 1..].trim();
        }
    }
    text
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        // End-ness is inherited through redefinition.
        if !el.is_end {
            // `:>>` targets are SEM034's; this family keeps the `:>` form it does not read.
            let redefined: Vec<String> = el
                .redefines
                .iter()
                .map(|text| bare(text.split(',').next().unwrap_or(text)).to_string())
                .collect();
            for text in el.supertypes.iter() {
                let head = bare(text.split(',').next().unwrap_or(text));
                if redefined.iter().any(|r| r == head) {
                    continue;
                }
                let Resolution::Found(target) = index.resolve(el.id, head) else {
                    continue;
                };
                if !index.is_end_of(target) {
                    continue;
                }
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` redefines `{head}`, which is an `end`, but is not one itself \
                         (validateRedefinitionEndConformance)",
                        qualified(el, index)
                    ),
                ));
            }
        }
        // Constancy is inherited through subsetting, and contradicts a variable.
        if el.modifiers.is_variable {
            // The `subsets` spelling is SEM036's (`validateSubsettingConstantConformance`, the
            // specification's own rule). This family keeps the `:`/`:>` form, which SEM036 does
            // not read — reporting both spellings is what gave one line two findings.
            let written: Vec<String> = el
                .subsets
                .iter()
                .map(|text| bare(text.split(',').next().unwrap_or(text)).to_string())
                .collect();
            for text in el.supertypes.iter() {
                let head = bare(text.split(',').next().unwrap_or(text));
                if written.iter().any(|w| w == head) {
                    continue;
                }
                let Resolution::Found(target) = index.resolve(el.id, head) else {
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
                        "`{}` is a variable that subsets `{head}`, which is constant \
                         (validateSubsettingConstantConformance)",
                        qualified(el, index)
                    ),
                ));
            }
        }
    });
    out
}
