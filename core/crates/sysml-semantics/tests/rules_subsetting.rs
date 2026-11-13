//! `validateSubsettingConstantConformance`, proved both ways.
//!
//! `SEM036` is the group code for the `Subsetting` rules, and it carries more than one: the group also
//! reports `validateSubsettingFeaturingTypes`. Filtering on the code alone therefore collects both, so
//! these tests narrow to the rule they are about by matching its name in the message. The alternative —
//! a code per rule — would break the convention every other group follows (56 codes carry 129 rules,
//! and the commonest shape is two rules to a code).

use sysml_semantics::{check, Severity};

/// The `SEM036` messages that name `validateSubsettingConstantConformance`.
fn constant_conformance(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM036")
        .filter(|i| i.message.contains("validateSubsettingConstantConformance"))
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_variable_subsetting_a_constant_is_rejected() {
    let src = r#"
package P {
	part def A { constant part f : A; }
	part def B { var part g subsets A::f; }
}
"#;
    let found = constant_conformance(src);
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("validateSubsettingConstantConformance")),
        "got {found:?}"
    );
}

#[test]
fn a_constant_variable_subsetting_a_constant_is_accepted() {
    let src = r#"
package P {
	part def A { constant part f : A; }
	part def B { constant var part g subsets A::f; }
}
"#;
    assert!(
        constant_conformance(src).is_empty(),
        "the subsetting feature is constant too"
    );
}

#[test]
fn a_variable_subsetting_a_non_constant_is_accepted() {
    let src = r#"
package P {
	part def A { part f : A; }
	part def B { var part g subsets A::f; }
}
"#;
    assert!(
        constant_conformance(src).is_empty(),
        "the antecedent needs the subsetted feature to be constant"
    );
}

#[test]
fn a_specialization_is_not_a_subsetting() {
    // `:>` and `subsets` are different relations and only the second is this rule's subject.
    let src = r#"
package P {
	part def A { constant part f : A; }
	part def B { var part g :> A::f; }
}
"#;
    assert!(
        constant_conformance(src).is_empty(),
        "a `:>` specialization must not be judged by the Subsetting rule"
    );
}
