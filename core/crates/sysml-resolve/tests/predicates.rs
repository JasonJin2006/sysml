//! Names written inside a requirement predicate, and whether they resolve.
//!
//! Before the expression layer existed, `require constraint { d.endurancee >= 20 }` lowered to
//! a string that nothing looked at: the resolver checked typings and `satisfy` ends, so a
//! predicate could name a feature the model never declared and no tool said a word. These
//! tests pin the opposite.

use sysml_resolve::{Index, Resolution};

fn check(src: &str) -> (Vec<(String, Resolution)>, Index) {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    let refs = index
        .references(&loaded.roots)
        .into_iter()
        .filter(|r| r.in_predicate)
        .map(|r| (r.text, r.target))
        .collect();
    (refs, index)
}

fn predicate_names(src: &str) -> Vec<(String, Resolution)> {
    check(src).0
}

#[test]
fn a_misspelt_feature_in_a_predicate_is_unresolved() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Drone { attribute endurance : Real; }
    requirement def R {
        subject d : Drone;
        require constraint { d.endurancee >= 20 }
    }
}
"#;
    let names = predicate_names(src);
    assert_eq!(names.len(), 1);
    assert_eq!(names[0].0, "d.endurancee");
    assert_eq!(
        names[0].1,
        Resolution::Unresolved,
        "a name the model does not declare must not resolve"
    );
}

#[test]
fn the_correctly_spelt_feature_resolves_through_the_subject_type() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Drone { attribute endurance : Real; }
    requirement def R {
        subject d : Drone;
        require constraint { d.endurance >= 20 }
    }
}
"#;
    let (names, index) = check(src);
    assert_eq!(names.len(), 1);
    let Resolution::Found(id) = &names[0].1 else {
        panic!("expected a resolution, got {:?}", names[0].1);
    };
    assert_eq!(
        index.qualified[*id], "P::Drone::endurance",
        "the path must land on the feature the type declares, not merely avoid an error"
    );
}

#[test]
fn a_subject_is_resolvable_in_its_own_right() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Drone { attribute endurance : Real; }
    requirement def R {
        subject d : Drone;
        require constraint { d.endurance >= 20 }
    }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    let refs = index.references(&loaded.roots);
    let subject = refs
        .iter()
        .find(|r| r.text == "Drone" && !r.in_predicate)
        .expect("the subject's typing is reported as a reference");
    assert!(subject.target.is_resolved());
}

#[test]
fn a_unit_bracket_is_not_a_reference() {
    let src = r#"
package P {
    private import ScalarValues::*;
    requirement def R {
        require constraint { x >= 20 [min] }
    }
}
"#;
    let names = predicate_names(src);
    let texts: Vec<&str> = names.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(
        texts,
        vec!["x"],
        "`min` is a unit identifier, not a model element to resolve"
    );
}

#[test]
fn names_inside_a_binary_expression_are_all_reached() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Drone {
        attribute energy : Real;
        attribute power : Real;
        attribute endurance : Real;
    }
    requirement def R {
        subject d : Drone;
        require constraint { d.energy / d.power >= d.endurance }
    }
}
"#;
    let names = predicate_names(src);
    let texts: Vec<&str> = names.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(texts, vec!["d.energy", "d.power", "d.endurance"]);
    for (text, target) in &names {
        assert!(
            target.is_resolved(),
            "{text} should resolve, got {target:?}"
        );
    }
}

#[test]
fn one_broken_name_does_not_hide_the_others() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Drone {
        attribute energy : Real;
        attribute power : Real;
    }
    requirement def R {
        subject d : Drone;
        require constraint { d.energy / d.pwer >= 1 }
    }
}
"#;
    let names = predicate_names(src);
    let by_text: Vec<(&str, bool)> = names
        .iter()
        .map(|(t, r)| (t.as_str(), r.is_resolved()))
        .collect();
    assert_eq!(
        by_text,
        vec![("d.energy", true), ("d.pwer", false)],
        "the good name still resolves while the misspelt one is reported"
    );
}

#[test]
fn a_subject_of_the_wrong_type_makes_the_path_unresolved() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Drone { attribute endurance : Real; }
    part def Battery { attribute capacity : Real; }
    requirement def R {
        subject b : Battery;
        require constraint { b.endurance >= 20 }
    }
}
"#;
    let names = predicate_names(src);
    assert_eq!(
        names[0].1,
        Resolution::Unresolved,
        "`endurance` belongs to Drone, not to the declared subject's type"
    );
}
