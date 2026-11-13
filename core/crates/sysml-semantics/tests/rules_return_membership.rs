//! A result membership belongs to a function or an expression —
//! `validateReturnParameterMembershipOwningType` and
//! `validateResultExpressionMembershipOwningType`.

use sysml_semantics::{check, Severity};

fn sem069(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM069")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_return_in_a_part_definition_is_rejected() {
    let src = r#"
package P {
	part def Holder {
		return : Integer = 1;
	}
}
"#;
    assert_eq!(sem069(src).len(), 1, "got {:?}", sem069(src));
}

#[test]
fn a_return_in_a_calc_is_accepted() {
    let src = r#"
package P {
	calc def Total {
		return : Integer = 1;
	}
}
"#;
    assert!(sem069(src).is_empty(), "got {:?}", sem069(src));
}

#[test]
fn a_case_has_a_result_too() {
    let src = r#"
package P {
	analysis case Mass {
		return : Integer = 1;
	}
}
"#;
    assert!(
        sem069(src).is_empty(),
        "a case's result is what its return states, got {:?}",
        sem069(src)
    );
}

#[test]
fn an_ordinary_parameter_is_not_judged() {
    // The rule is about result memberships; an `in` parameter is a different one, and telling
    // them apart is what `clause = "return"` does.
    let src = r#"
package P {
	part def Holder {
		in x : Integer;
	}
}
"#;
    assert!(sem069(src).is_empty(), "got {:?}", sem069(src));
}
