//! The metaclass table and the projection's mapping onto it.
//!
//! The table is generated and checked against the published crate's hierarchy at generation time.
//! These tests check what generation cannot: that the hand-written `Kind → metaclass` mapping names
//! only metaclasses that exist, that it is total over [`Kind::ALL`], that the five gaps are exactly
//! the five recorded, and that the inheritance questions a rule asks get the metamodel's answer.

use sysml_model::metaclass::{ancestors, is_a, is_metaclass, Meta, METACLASSES};
use sysml_model::Kind;

#[test]
fn the_table_is_not_empty_and_names_are_unique() {
    assert!(METACLASSES.len() > 150, "{} rows", METACLASSES.len());
    let mut seen: Vec<&str> = METACLASSES.iter().map(|m| m.name).collect();
    let before = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(before, seen.len(), "a metaclass is listed twice");
}

#[test]
fn every_supertype_is_a_row() {
    for Meta { name, supers } in METACLASSES {
        for parent in supers.iter() {
            assert!(
                is_metaclass(parent),
                "`{name}` names supertype `{parent}`, which is not in the table"
            );
        }
    }
}

#[test]
fn no_metaclass_specializes_itself() {
    for row in METACLASSES {
        assert!(
            !row.supers.contains(&row.name),
            "`{}` lists itself as a supertype",
            row.name
        );
    }
}

#[test]
fn the_hierarchy_has_no_cycle() {
    // A cycle would make `ancestors` loop forever and `is_a` answer true for unrelated classes.
    fn walk(name: &'static str, path: &mut Vec<&'static str>) {
        assert!(!path.contains(&name), "cycle: {:?} -> {name}", path.clone());
        path.push(name);
        for parent in ancestors(name) {
            assert!(is_metaclass(parent));
        }
        for parent in sysml_model::metaclass::meta(name)
            .map(|m| m.supers)
            .unwrap_or(&[])
        {
            walk(parent, path);
        }
        path.pop();
    }
    for row in METACLASSES {
        walk(row.name, &mut Vec::new());
    }
}

#[test]
fn inheritance_answers_what_the_rules_ask() {
    // The questions the specification's OCL asks, with the answers the metamodel declares.
    assert!(is_a("PartUsage", "Usage"));
    assert!(is_a("PartUsage", "Feature"));
    assert!(is_a("PartUsage", "Type"));
    assert!(is_a("PartUsage", "Element"));
    assert!(is_a("PartUsage", "ItemUsage"));
    assert!(is_a("PartUsage", "OccurrenceUsage"));
    assert!(is_a("PartUsage", "Namespace"));
    assert!(!is_a("PartUsage", "Definition"));
    assert!(!is_a("PartUsage", "Behavior"));

    assert!(is_a("ActionDefinition", "Definition"));
    assert!(is_a("ActionDefinition", "Behavior"));
    assert!(is_a("ActionDefinition", "OccurrenceDefinition"));
    assert!(!is_a("ActionDefinition", "Usage"));

    assert!(is_a("Class", "Classifier"));
    assert!(is_a("DataType", "Classifier"));
    assert!(!is_a("Class", "DataType"));

    assert!(is_a("Expression", "Step"));
    assert!(is_a("InvocationExpression", "Expression"));
    assert!(is_a("FeatureReferenceExpression", "Expression"));

    assert!(is_a("EndFeatureMembership", "FeatureMembership"));
    assert!(is_a("FeatureMembership", "OwningMembership"));
    assert!(!is_a("FeatureMembership", "ParameterMembership"));
    assert!(is_a("ReturnParameterMembership", "ParameterMembership"));
    assert!(is_a("ParameterMembership", "FeatureMembership"));

    assert!(is_a("AssociationStructure", "Association"));
    assert!(is_a("AssociationStructure", "Structure"));
}

#[test]
fn an_unknown_name_is_an_instance_of_nothing() {
    assert!(!is_a("Bogus", "Element"));
    assert!(!is_metaclass("Bogus"));
    assert!(ancestors("Bogus").is_empty());
}

#[test]
fn every_kind_maps_to_metaclasses_that_exist() {
    for kind in Kind::ALL {
        for name in kind.metaclasses() {
            assert!(
                is_metaclass(name),
                "{kind:?} maps to `{name}`, which the metamodel does not declare"
            );
        }
    }
    // The list is pinned: a variant added to the enum and forgotten here would leave this loop
    // checking less than the enum contains. `Other` is absent because it carries a payload.
    assert_eq!(
        Kind::ALL.len(),
        92,
        "Kind::ALL is out of step with the enum"
    );
}

#[test]
fn exactly_five_kinds_stand_for_no_metaclass() {
    let all: Vec<Kind> = Kind::ALL
        .iter()
        .cloned()
        .chain([Kind::Other("x".to_string())])
        .collect();
    let empty: Vec<&Kind> = all.iter().filter(|k| k.metaclasses().is_empty()).collect();
    let mut names: Vec<String> = empty.iter().map(|k| format!("{k:?}")).collect();
    names.sort();
    assert_eq!(
        names,
        ["Alias", "Other(\"x\")", "PseudoState", "Unsupported", "Variant"],
        "every other kind must stand for at least one metaclass; an empty mapping is a recorded gap"
    );
    for kind in empty {
        assert!(
            !kind.is_metaclass("Element"),
            "{kind:?} stands for nothing, so it cannot be an Element either"
        );
    }
}

#[test]
fn an_association_structure_has_both_classifier_roles_with_one_kind() {
    let kind = Kind::AssocStructureDef;
    assert_eq!(kind.metaclasses(), ["AssociationStructure"]);
    for metaclass in ["Association", "Structure", "Class", "Classifier", "Type"] {
        assert!(kind.is_metaclass(metaclass), "{metaclass}");
    }
    assert!(!kind.is_metaclass("Feature"));
    assert!(!kind.is_metaclass("AttributeUsage"));
    assert!(kind.is_definition());
    assert_eq!(kind.keyword(), "assoc struct");
}

#[test]
fn a_usage_is_a_usage_and_a_definition_is_not() {
    assert!(Kind::PartDef.is_metaclass("PartDefinition"));
    assert!(Kind::PartDef.is_metaclass("Definition"));
    assert!(!Kind::PartDef.is_metaclass("Usage"));
    assert!(Kind::PartUsage.is_metaclass("PartUsage"));
    assert!(Kind::PartUsage.is_metaclass("Usage"));
    assert!(!Kind::PartUsage.is_metaclass("Definition"));
    assert!(Kind::PortUsage.is_metaclass("Feature"));
    assert!(Kind::Connection.is_metaclass("Connector"));
    assert!(Kind::Connection.is_metaclass("ConnectionUsage"));
    assert!(Kind::Import.is_metaclass("Import"));
    assert!(Kind::Actor.is_metaclass("PartUsage"));
    assert!(Kind::Transition.is_metaclass("TransitionUsage"));
    assert!(Kind::AssertConstraint.is_metaclass("AssertConstraintUsage"));
    assert!(Kind::AssertConstraint.is_metaclass("ConstraintUsage"));
    assert!(!Kind::AssertConstraint.is_metaclass("RequirementUsage"));
}
