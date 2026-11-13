//! `specialization-kinds` — what a definition is allowed to specialize.
//!
//! The specification states the rule once per metaclass, as a prohibition between two metaclasses:
//!
//!   * `validateBehaviorSpecialization` — "A Behavior must not specialize a Structure."
//!   * `validateStructureSpecialization` — "A Structure must not specialize a Behavior."
//!
//! Both are decided from `Element::supertype_ref` plus the resolved kind of the target, which the
//! tree already carries. They are here together because they are one question asked twice: a
//! cyclic prohibition between the same two families.
//!
//! `validateAssociationStructureIntersection` holds by construction: `assoc struct` is one
//! [`Kind::AssocStructureDef`] with both `Association` and `Structure` metaclasses. Its ledger entry
//! lives in `conformance.rs`; no separate intersection diagnostic is needed here.
//!
//! `validateClassSpecialization` permits an association supertype only for an
//! `AssociationStructure`. A plain KerML class reaches the `Class` arm below and is rejected for
//! an association supertype. The combined kind reaches `Structure`, so specializing an association
//! is allowed while specializing a behavior still violates `validateStructureSpecialization`.
//!
//! # The families
//!
//! `Behavior`, `Structure`, `Class`, `DataType` and `Association` are the metamodel's own classes,
//! asked through [`Kind::is_metaclass`](sysml_model::Kind::is_metaclass). That is the relation the
//! specification's OCL names, and the one the projection cannot answer from
//! [`Group`](sysml_model::Group): `rendering def` and `metadata def` are structures in the
//! metamodel and `Group::Other` in the view groups, so a group-keyed reading skipped them.
//!
//! The order of the questions matters, because `Structure` and `Behavior` are both `Class`es in
//! KerML. `Behavior` is asked first, then `Structure`, then `Class`: a `part def` is a structure
//! for this rule's purposes, while a KerML `class` — which specializes neither — reaches the
//! `Class` arm.
//!
//! The report code this group reports under is `SEM035`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM035";

/// A resolved supertype's kind, and which family it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Behavior,
    Structure,
    /// A KerML `class`. Its own rules forbid specializing a DataType or a bare Association.
    Class,
    /// A KerML `datatype`.
    DataType,
    /// A KerML `association`.
    Association,
    /// A kind in none of these: a package, a requirement. A specialization of one is not what any
    /// rule here forbids.
    Neither,
}

fn family(kind: &Kind) -> Family {
    if kind.is_metaclass("Behavior") {
        Family::Behavior
    } else if kind.is_metaclass("Structure") {
        Family::Structure
    } else if kind.is_metaclass("Class") {
        Family::Class
    } else if kind.is_metaclass("DataType") {
        Family::DataType
    } else if kind.is_metaclass("Association") {
        Family::Association
    } else {
        Family::Neither
    }
}

fn family_name(family: Family) -> &'static str {
    match family {
        Family::Behavior => "a behavior",
        Family::Structure => "a structure",
        Family::Class => "a class",
        Family::DataType => "a datatype",
        Family::Association => "an association",
        Family::Neither => "none of these",
    }
}

/// The family an element belongs to, for the antecedent of each rule.
fn own_family(kind: &Kind) -> Family {
    family(kind)
}

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The specializations the source wrote with `:`, in authored order.
///
/// `Element::supertypes` is not that list: the lowering appends every redefinition target to it as
/// well (`lower.rs:384-397`), because a redefined feature must inherit the members of the feature
/// it replaces. That is the right call for member lookup and the wrong input for this rule — a
/// `:>>` redefinition is not an `ownedSpecialization`, and treating one as such reported a legal
/// `part anEngine :>> alternative` in the official 10b model as a structure specializing a
/// behaviour. So the redefinition targets are subtracted here, leaving only what `:` wrote.
fn written_specializations(el: &Element) -> Vec<String> {
    let written: Vec<String> = if el.supertypes.is_empty() {
        el.supertype_ref.iter().cloned().collect()
    } else {
        el.supertypes.clone()
    };
    written
        .into_iter()
        .filter(|text| !el.redefines.iter().any(|r| r == text))
        .collect()
}

/// `validateBehaviorSpecialization` and `validateStructureSpecialization`, as one walk.
///
/// A behaviour specializing a structure violates the first; a structure specializing a behaviour
/// violates the second. The message names the rule that applies rather than a generic one, so a
/// report says which clause of the specification was broken.
fn check(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    let own = own_family(&el.kind);
    if own == Family::Neither {
        return;
    }
    for text in written_specializations(el) {
        let head = text.split(',').next().unwrap_or(&text).trim();
        let Some(target) = index.resolve(el.id, head).found() else {
            // An unresolved supertype is reported by the reference pass; judging its family here
            // would guess.
            continue;
        };
        let target_family = family(&index.kind[target]);
        let forbidden = match (own, target_family) {
            (Family::Behavior, Family::Structure) => Some("validateBehaviorSpecialization"),
            (Family::Structure, Family::Behavior) => Some("validateStructureSpecialization"),
            // A plain Class cannot specialize a DataType or an Association. An
            // AssociationStructure reaches Family::Structure instead, so its association
            // specialization is exempt without losing its prohibition against Behavior.
            (Family::Class, Family::DataType) => Some("validateClassSpecialization"),
            (Family::Class, Family::Association) => Some("validateClassSpecialization"),
            // `validateDataTypeSpecialization`: a DataType must not specialize a Class or an
            // Association.
            (Family::DataType, Family::Class) => Some("validateDataTypeSpecialization"),
            (Family::DataType, Family::Association) => Some("validateDataTypeSpecialization"),
            _ => None,
        };
        let Some(rule) = forbidden else { continue };
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is {} specializing {} `{}`; {rule} forbids it",
                qualified(el, index),
                family_name(own),
                family_name(target_family),
                index.qualified[target]
            ),
        ));
    }
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| check(el, index, &mut out));
    out
}
