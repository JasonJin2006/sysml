//! `port-nested-composite` — a port's payload features are not composite.
//!
//! `validatePortDefinitionOwnedUsagesNotComposite` and
//! `validatePortUsageNestedUsagesNotComposite`: `ownedUsage->reject(oclIsKindOf(PortUsage))
//! ->forAll(not isComposite)`, and the same over `nestedUsage`.
//!
//! ```text
//! port def FuelOut {
//!     part fuel : Fuel;              // rejected: the port carries it, it does not own it
//!     ref part carried : Fuel;       // accepted
//!     port inner : FuelIn;           // a nested port is exempt
//! }
//! ```
//!
//! # How it reads
//!
//! `isComposite` is [`Element::is_composite`], the effective value the lowering fills from the
//! specification's derivation. It is not the written `composite` word: no SysML usage can write
//! that word (`SysML.xtext:556-566`), and the pilot model that states both violations —
//! `PortUsage_Invalid.sysml.xt:35` and `:50` — writes plain `part b2 : B;` and `part b4 : B;`.
//! Reading the effective value is what lets this check reject exactly the models those two
//! assertions name.
//!
//! Nested ports are skipped by kind — the OCL's `reject(oclIsKindOf(PortUsage))` is exactly that
//! exemption — and an inherited copy is skipped as well: a port copied onto a part usage from its
//! definition is not authored there, and the definition that wrote it is where it is judged.
//!
//! The report code this group reports under is `SEM066`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM066";

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
        if !matches!(el.kind, Kind::PortDef | Kind::PortUsage) {
            return;
        }
        let owner_kind = el.kind.keyword();
        let owner_name = qualified(el, index);
        for child in &el.children {
            // A port is exempt: `reject(oclIsKindOf(PortUsage))`. `PortDef` is here for the same
            // reason: a port definition nested in a port is a port, and the lowering keeps a
            // definition where the grammar allows one.
            if matches!(child.kind, Kind::PortDef | Kind::PortUsage) {
                continue;
            }
            if child.inherited || !child.is_composite {
                continue;
            }
            out.push(Violation::new(
                CODE,
                child,
                qualified(child, index),
                format!(
                    "`{}` is composite inside `{owner_name}`, a {owner_kind}; a port's members are \
                     carried by the port, not owned completely \
                     (validatePortDefinitionOwnedUsagesNotComposite / \
                     validatePortUsageNestedUsagesNotComposite)",
                    qualified(child, index)
                ),
            ));
        }
    });
    out
}
