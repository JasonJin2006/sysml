//! `validateInvocationExpressionInstantiatedType`: a call names something callable.

use sysml_semantics::{check, Severity};

fn sem083(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM083")
        .map(|i| i.message.clone())
        .collect()
}

#[test]
fn calling_a_part_definition_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	attribute bad = Wheel(2);
}
"#;
    assert_eq!(sem083(src).len(), 1, "got {:?}", sem083(src));
}

#[test]
fn calling_a_calculation_is_accepted() {
    let src = r#"
package P {
	calc def Double { in x : Integer; return : Integer = x; }
	attribute good = Double(2);
}
"#;
    assert!(sem083(src).is_empty(), "got {:?}", sem083(src));
}

#[test]
fn calling_a_library_function_is_accepted() {
    // The kernel function library declares its operators as KerML types.
    let src = r#"
package P {
	attribute xs = (1, 2, 3);
	attribute total = NumericalFunctions::sum(xs);
}
"#;
    assert!(sem083(src).is_empty(), "got {:?}", sem083(src));
}

#[test]
fn an_unresolved_callee_is_left_to_the_reference_check() {
    let src = r#"
package P {
	attribute x = Nowhere(1);
}
"#;
    assert!(
        sem083(src).is_empty(),
        "a dangling callee is a reference problem, got {:?}",
        sem083(src)
    );
}

#[test]
fn a_call_inside_a_constraint_is_judged_too() {
    let src = r#"
package P {
	part def Wheel;
	requirement def R {
		require constraint { Wheel(2) > 0 }
	}
}
"#;
    assert_eq!(sem083(src).len(), 1, "got {:?}", sem083(src));
}
