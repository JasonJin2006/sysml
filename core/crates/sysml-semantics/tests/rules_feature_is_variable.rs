//! `validateFeatureIsVariable`: `var` needs an owner that can change.

use sysml_semantics::{check, Severity};

fn sem077(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM077")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_variable_in_a_package_is_rejected() {
    let src = r#"
package P {
	var attribute top : Real;
}
"#;
    assert_eq!(sem077(src).len(), 1, "got {:?}", sem077(src));
}

#[test]
fn a_variable_inside_a_data_type_is_rejected() {
    let src = r#"
package P {
	attribute def D { var desc : Real; }
}
"#;
    assert_eq!(sem077(src).len(), 1, "got {:?}", sem077(src));
}

#[test]
fn a_variable_inside_an_occurrence_is_accepted() {
    let src = r#"
package P {
	occurrence def Engine { var part rpm : Real; }
}
"#;
    assert!(sem077(src).is_empty(), "got {:?}", sem077(src));
}

#[test]
fn a_variable_inside_a_part_is_accepted() {
    // A part is an occurrence in SysML: the library specializes it from the KerML occurrence, and
    // the model writes no clause for it.
    let src = r#"
package P {
	part def Drawing { var part scale : Real; }
}
"#;
    assert!(sem077(src).is_empty(), "got {:?}", sem077(src));
}

#[test]
fn a_return_slot_is_not_judged() {
    // A result slot is a variable by construction; the model never wrote `var`.
    let src = r#"
package P {
	calc def C { return : Integer = 1; }
}
"#;
    assert!(sem077(src).is_empty(), "got {:?}", sem077(src));
}

#[test]
fn a_constant_is_not_judged() {
    let src = r#"
package P {
	attribute def D { constant attribute c : Real = 1; }
}
"#;
    assert!(sem077(src).is_empty(), "got {:?}", sem077(src));
}

// -- validateFeatureValueIsInitial -----------------------------------------

/// The messages this group reports that name `validateFeatureValueIsInitial`.
fn initial(src: &str) -> Vec<String> {
    sem077(src)
        .into_iter()
        .filter(|(_, _, m)| m.contains("validateFeatureValueIsInitial"))
        .map(|(_, at, m)| format!("{at}: {m}"))
        .collect()
}

#[test]
fn an_initial_value_on_a_variable_is_accepted() {
    let src = r#"
package P {
	occurrence def Engine {
		var part rpm : Real := 0;
	}
}
"#;
    assert!(
        initial(src).is_empty(),
        "`var ... := v` is what `:=` is for: {:?}",
        initial(src)
    );
}

#[test]
fn an_initial_value_on_a_non_variable_is_rejected() {
    let src = r#"
package P {
	part def Engine {
		part rpm : Real := 0;
	}
}
"#;
    let found = initial(src);
    assert_eq!(
        found.len(),
        1,
        "`:=` assigns, so the feature has to be a variable: {found:?}"
    );
}

#[test]
fn a_plain_default_value_is_not_an_initial_one() {
    // `= v` states a default; only `:=` states an assignment.
    let src = r#"
package P {
	part def Engine {
		part rpm : Real = 0;
	}
}
"#;
    assert!(
        initial(src).is_empty(),
        "`= v` is not `:=`: {:?}",
        initial(src)
    );
}

// -- validateFeatureValueOverriding ----------------------------------------

/// The messages this group reports that name `validateFeatureValueOverriding`.
fn overriding(src: &str) -> Vec<String> {
    sem077(src)
        .into_iter()
        .filter(|(_, _, m)| m.contains("validateFeatureValueOverriding"))
        .map(|(_, at, m)| format!("{at}: {m}"))
        .collect()
}

/// A redefinition may restate a value only over a `default` one.
///
/// `isDefault` is true **only when the word `default` was written** — the grammar is
/// `'=' | isInitial ?= ':=' | isDefault ?= 'default' ('=' | ':=')?` (`KerML.xtext:744-750`), so a
/// plain `= 0` sets neither flag. This test previously wrote a plain `= 0` and called it a default
/// value, which pinned the wrong reading: under the OCL's `forAll(isDefault)` a plain value is not a
/// default, so that model *is* a violation. The spelling below is the one the rule actually allows.
#[test]
fn a_value_over_a_default_value_is_accepted() {
    let src = r#"
package P {
	part def Base { part p : Real default = 0; }
	part def Derived :> Base { part :>> p = 1; }
}
"#;
    assert!(overriding(src).is_empty(), "got {:?}", overriding(src));
}

/// The same shape with a plain `= 0` is rejected, because `= 0` is not a default.
///
/// Kept beside the test above so the pair states the whole rule: the two models differ only in the
/// word `default`, and that word is the entire antecedent.
#[test]
fn a_value_over_a_plain_value_is_rejected() {
    let src = r#"
package P {
	part def Base { part p : Real = 0; }
	part def Derived :> Base { part :>> p = 1; }
}
"#;
    let found = overriding(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].contains("P::Base::p"), "got {found:?}");
    assert!(
        found[0].contains("not a default"),
        "the message must say why a plain value does not qualify: {found:?}"
    );
}

#[test]
fn a_value_over_an_initial_value_is_rejected() {
    // The redefined feature states a one-time assignment, so a redefinition may not build on it.
    let src = r#"
package P {
	part def Base { part p : Real := 0; }
	part def Derived :> Base { part :>> p := 1; }
}
"#;
    let found = overriding(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].contains("P::Base::p"), "got {found:?}");
}

#[test]
fn the_closure_follows_two_redefinitions() {
    // `closure(redefinition.redefinedFeature)` is transitive: `Derived` redefines `Middle`, which
    // redefines `Base`, and it is `Base`'s initial value that the rule reaches.
    //
    // `Middle` writes `default = 1` on purpose. With a plain `= 1` it would be an offender in its own
    // right, and `Derived` would then be reported against `Middle` — a correct answer to a different
    // question. Making the intermediate value a default leaves the `:=` at the bottom of the closure as
    // the only thing that can be blamed, which is what this test is about.
    let src = r#"
package P {
	part def Base { part p : Real := 0; }
	part def Middle :> Base { part :>> p default = 1; }
	part def Derived :> Middle { part :>> p = 2; }
}
"#;
    let found = overriding(src);
    // Both `Middle` and `Derived` sit above `Base`'s initial value, so both are reported: the rule
    // quantifies over the whole closure, not just the immediate redefinition.
    assert_eq!(found.len(), 2, "got {found:?}");
    assert!(
        found.iter().all(|m| m.contains("P::Base::p")),
        "the offender named is the one with the initial value: {found:?}"
    );
}

#[test]
fn a_feature_without_a_value_is_not_judged() {
    let src = r#"
package P {
	part def Base { part p : Real := 0; }
	part def Derived :> Base { part :>> p; }
}
"#;
    assert!(overriding(src).is_empty(), "got {:?}", overriding(src));
}
