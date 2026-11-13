//! The two specialization-family rules, each proved by a model that breaks it.
//!
//! `validateBehaviorSpecialization` and `validateStructureSpecialization` are one prohibition
//! asked in both directions: a behaviour may not specialize a structure, and a structure may not
//! specialize a behaviour. Both are decided from what the source wrote with `:`.
//!
//! The redefinition case has its own test because it is the shape that produced a false positive
//! on the official corpus: `Element::supertypes` carries redefinition targets as well
//! (`lower.rs:384-397`), and a rule reading that list reported a legal
//! `part anEngine :>> alternative` in `10b-Trade-off Among Alternative Configurations.sysml`.

use sysml_semantics::{check, Severity};

fn issues(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

fn sem035(src: &str) -> Vec<(String, String, String)> {
    issues(src)
        .into_iter()
        .filter(|(c, _, _)| c == "SEM035")
        .collect()
}

#[test]
fn a_behavior_specializing_a_structure_is_rejected() {
    let src = r#"
package P {
	part def Frame;
	action def Work :> Frame;
}
"#;
    let found = sem035(src);
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("validateBehaviorSpecialization")),
        "an action specializing a part must be reported under validateBehaviorSpecialization, \
         got {found:?}"
    );
}

#[test]
fn a_structure_specializing_a_behavior_is_rejected() {
    let src = r#"
package P {
	action def Work;
	part def Frame :> Work;
}
"#;
    let found = sem035(src);
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("validateStructureSpecialization")),
        "a part specializing an action must be reported under validateStructureSpecialization, \
         got {found:?}"
    );
}

#[test]
fn specializing_within_one_family_is_accepted() {
    let src = r#"
package P {
	part def Base;
	part def Derived :> Base;
	action def Step;
	action def Step2 :> Step;
}
"#;
    assert!(
        sem035(src).is_empty(),
        "a part specializing a part and an action specializing an action are both legal"
    );
}

#[test]
fn specializing_a_data_type_or_requirement_is_not_this_rule() {
    let src = r#"
package P {
	private import ScalarValues::*;
	attribute def Amount;
	part def Rig { attribute total : Amount; }
	requirement def Needs { subject s : Rig; }
	requirement def Needs2 :> Needs;
}
"#;
    assert!(
        sem035(src).is_empty(),
        "a requirement specializing a requirement is in neither family and must not be reported"
    );
}

#[test]
fn a_redefinition_is_not_a_specialization() {
    // The official 10b model writes `part anEngine :>> alternative` inside an action that has a
    // `part alternative`. Treating the redefinition target as an `ownedSpecialization` reported
    // that legal model as a structure specializing a behaviour.
    let src = r#"
package P {
	part def Part1;
	action def Study {
		part alternative : Part1;
	}
	action def Study2 :> Study {
		part anEngine :>> alternative;
	}
}
"#;
    assert!(
        sem035(src).is_empty(),
        "`:>>` redefines a feature; it is not an `ownedSpecialization` and must not be judged"
    );
}

#[test]
fn the_rule_names_the_element_and_the_target() {
    let src = r#"
package P {
	part def Frame;
	action def Work :> Frame;
}
"#;
    let found = sem035(src);
    let (_, at, msg) = found.first().expect("one violation");
    assert_eq!(
        at, "P::Work",
        "the report names the offending element by qualified name"
    );
    assert!(
        msg.contains("P::Frame"),
        "and it names the target it may not specialize: {msg}"
    );
}

// -- the KerML classifiers ---------------------------------------------------
//
// `class`, `datatype`, `struct` and `assoc` used to lower to one `Kind::KermlType`, which
// discarded the only thing that tells them apart and left these rules undecidable.

#[test]
fn a_class_specializing_a_datatype_is_rejected() {
    let src = r#"
package P {
	datatype Measured;
	class Reading :> Measured;
}
"#;
    let found = sem035(src);
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("validateClassSpecialization")),
        "got {found:?}"
    );
}

#[test]
fn a_datatype_specializing_a_class_is_rejected() {
    let src = r#"
package P {
	class General;
	datatype Specific :> General;
}
"#;
    let found = sem035(src);
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("validateDataTypeSpecialization")),
        "got {found:?}"
    );
}

#[test]
fn a_datatype_specializing_an_association_is_rejected() {
    let src = r#"
package P {
	part def A;
	part def B;
	assoc Links { end x : A; end y : B; }
	datatype Bad :> Links;
}
"#;
    let found = sem035(src);
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("validateDataTypeSpecialization")),
        "a datatype may not specialize an association, got {found:?}"
    );
}

#[test]
fn specializing_within_one_classifier_family_is_accepted() {
    let src = r#"
package P {
	class C1;
	class C2 :> C1;
	datatype D1;
	datatype D2 :> D1;
}
"#;
    assert!(
        sem035(src).is_empty(),
        "a class specializing a class and a datatype specializing a datatype are both legal"
    );
}

#[test]
fn each_classifier_keyword_is_kept_on_the_tree() {
    let src = r#"
package P {
	class C;
	datatype D;
	struct S;
	assoc A;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let all = &loaded.roots;
    for (name, want) in [
        ("C", "class"),
        ("D", "datatype"),
        ("S", "struct"),
        ("A", "association"),
    ] {
        let found = all
            .iter()
            .flat_map(|r| r.children.iter())
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("no element named {name}"));
        assert_eq!(
            found.kind.keyword(),
            want,
            "`{name}` must keep its authored classifier keyword"
        );
    }
}

#[test]
fn a_kerml_behavior_specializing_a_structure_is_rejected() {
    // `Specialization_invalid.kerml.xt:57`: `behavior B1 specializes S;` — a Behavior must not
    // specialize a Structure (`validateBehaviorSpecialization`). Both halves of the question were
    // `KermlType` before `behavior` had its own kind, so the rule could not be asked.
    let src = r#"
package P {
	struct S;
	behavior B1 specializes S;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let issues = check(&loaded.roots, &loaded.library).issues;
    let found: Vec<&str> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM035")
        .map(|i| i.message.as_str())
        .collect();
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].contains("validateBehaviorSpecialization"),
        "got {found:?}"
    );
}

#[test]
fn a_kerml_structure_specializing_a_behavior_is_rejected() {
    // `Specialization_invalid.kerml.xt:61`: the other direction.
    let src = r#"
package P {
	behavior B1;
	struct S specializes B1;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let issues = check(&loaded.roots, &loaded.library).issues;
    let found: Vec<&str> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM035")
        .map(|i| i.message.as_str())
        .collect();
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].contains("validateStructureSpecialization"),
        "got {found:?}"
    );
}

#[test]
fn a_class_specializing_an_association_is_rejected() {
    // `class C2 specializes C1, D1, A1;` with `abstract assoc A1`
    // (`Specialization_invalid.kerml.xt:49`). A plain class is not the AssociationStructure
    // that the rule exempts.
    let src = r#"
package P {
	classifier C1;
	part def A;
	part def B;
	assoc A1 { end x : A; end y : B; }
	class C2 specializes C1, A1;
}
"#;
    let found = sem035(src);
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("validateClassSpecialization")),
        "a class may not specialize an association, got {found:?}"
    );
}

#[test]
fn an_association_structure_specializing_an_association_is_accepted() {
    let src = r#"
package P {
    abstract assoc Base;
    abstract assoc struct Derived :> Base;
}
"#;
    assert!(sem035(src).is_empty(), "got {:?}", sem035(src));
}

#[test]
fn an_association_structure_specializing_a_behavior_is_rejected() {
    let src = r#"
package P {
    behavior Work;
    abstract assoc struct Link :> Work;
}
"#;
    let found = sem035(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("validateStructureSpecialization"),
        "got {found:?}"
    );
}

#[test]
fn a_behavior_specializing_an_association_structure_is_rejected() {
    let src = r#"
package P {
    abstract assoc struct Link;
    behavior Work :> Link;
}
"#;
    let found = sem035(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("validateBehaviorSpecialization"),
        "got {found:?}"
    );
}

#[test]
fn each_written_supertype_is_judged_on_its_own() {
    // Two forbidden targets in one clause are two reports: the pilot states two errors on
    // `class C2 specializes C1, D1, A1;` (`Specialization_invalid.kerml.xt:49`), and
    // `datatype D2 specializes D1, C1, A1;` likewise (`:40`). A walk that stopped at the first
    // forbidden target matched one of the two and was counted as matching the line, which hid the
    // half it never asked about.
    let src = r#"
package P {
	classifier C1;
	datatype D1;
	part def A;
	part def B;
	assoc A1 { end x : A; end y : B; }
	class C2 specializes C1, D1, A1;
}
"#;
    let found = sem035(src);
    let class_reports: Vec<&(String, String, String)> = found
        .iter()
        .filter(|(_, _, msg)| msg.contains("validateClassSpecialization"))
        .collect();
    assert_eq!(
        class_reports.len(),
        2,
        "the datatype and the association are two reports, got {found:?}"
    );
}
