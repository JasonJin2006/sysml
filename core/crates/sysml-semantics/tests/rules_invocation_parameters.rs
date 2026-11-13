//! `validateInvocationExpressionParameterRedefinition` and its duplicate twin, proved both ways.
//!
//! The relation is `inp.ownedRedefinition.redefinedFeature`: a named argument states which parameter
//! of what is invoked it fills, and the rules are stated over that. Before the name was kept at
//! parse time every argument was an anonymous value, and neither rule had anything to read.

use sysml_semantics::{check, Severity};

fn sem095(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM095")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

fn reported(src: &str, rule: &str) -> bool {
    sem095(src).iter().any(|(_, _, m)| m.contains(rule))
}

const CALC: &str = r#"
package P {
	private import ScalarValues::*;
	calc def F {
		in x : Real;
		in y : Real;
		return : Real = x + y;
	}
"#;

#[test]
fn each_argument_naming_a_parameter_is_accepted() {
    let src = format!("{CALC}\tattribute ok = F(x = 1, y = 2);\n}}\n");
    assert!(!reported(
        &src,
        "validateInvocationExpressionParameterRedefinition"
    ));
    assert!(!reported(
        &src,
        "validateInvocationExpressionNoDuplicateParameterRedefinition"
    ));
}

#[test]
fn an_argument_naming_nothing_is_rejected() {
    // `z` is not a parameter of `F`, so `intersection(parameters)` is empty and the count is zero.
    let src = format!("{CALC}\tattribute bad = F(z = 1, y = 2);\n}}\n");
    assert!(reported(
        &src,
        "validateInvocationExpressionParameterRedefinition"
    ));
}

#[test]
fn a_call_whose_only_argument_names_nothing_is_rejected() {
    // The guard skipped a call that resolved to no argument at all, which is exactly the case where
    // no name refers to a parameter. With one unknown name beside a known one the call was
    // reported; alone, it was silent.
    let src = format!("{CALC}\tattribute bad = F(z = 1);\n}}\n");
    assert!(reported(
        &src,
        "validateInvocationExpressionParameterRedefinition"
    ));
}

#[test]
fn a_constructor_argument_naming_an_inherited_feature_is_accepted() {
    // `b` belongs to `Base`, and `D :> Base`. Reading only the instantiated type's own children
    // made the name unknown, which the count would then have reported as a parameter that does not
    // exist.
    let src = r#"
package P {
	private import ScalarValues::*;
	part def Base {
		attribute b : Real;
	}
	part def D :> Base;
	attribute ok = new D(b = 1);
}
"#;
    assert!(!reported(
        src,
        "validateConstructorExpressionNoDuplicateFeatureRedefinition"
    ));
    assert!(!reported(
        src,
        "validateInvocationExpressionParameterRedefinition"
    ));
}

#[test]
fn two_arguments_naming_one_parameter_are_rejected() {
    let src = format!("{CALC}\tattribute dup = F(x = 1, x = 2);\n}}\n");
    assert!(reported(
        &src,
        "validateInvocationExpressionNoDuplicateParameterRedefinition"
    ));
}

#[test]
fn a_constructor_takes_the_same_relation() {
    let src = r#"
package P {
	private import ScalarValues::*;
	part def Wheel {
		attribute mass : Real;
		attribute radius : Real;
	}
	attribute dup = new Wheel(mass = 1, mass = 2);
}
"#;
    assert!(reported(
        src,
        "validateConstructorExpressionNoDuplicateFeatureRedefinition"
    ));
}

#[test]
fn a_positional_argument_is_not_judged() {
    // A positional argument writes no redefinition, and the rule is stated over the redefinition.
    let src = format!("{CALC}\tattribute pos = F(1, 2);\n}}\n");
    assert!(!reported(
        &src,
        "validateInvocationExpressionParameterRedefinition"
    ));
}
