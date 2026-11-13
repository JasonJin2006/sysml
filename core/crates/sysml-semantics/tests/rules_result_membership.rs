//! A function states at most one result —
//! `validateExpressionResultExpressionMembership` and
//! `validateFunctionResultExpressionMembership`.

use sysml_semantics::{check, Severity};

fn sem058(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM058")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn two_returns_in_a_calc_are_rejected() {
    let src = r#"
package P {
	calc def C {
		return : Integer = 1;
		return : Integer = 2;
	}
}
"#;
    assert_eq!(sem058(src).len(), 1, "got {:?}", sem058(src));
}

#[test]
fn one_return_in_a_calc_is_accepted() {
    let src = r#"
package P {
	calc def C {
		return : Integer = 1;
	}
}
"#;
    assert!(sem058(src).is_empty());
}

#[test]
fn a_calc_with_no_return_is_accepted() {
    // The rule is an upper bound, not a requirement.
    let src = r#"
package P {
	calc def C {
		in x : Integer;
	}
}
"#;
    assert!(sem058(src).is_empty(), "got {:?}", sem058(src));
}

#[test]
fn the_report_names_the_function() {
    let src = r#"
package P {
	calc def Total {
		return : Integer = 1;
		return : Integer = 2;
	}
}
"#;
    let (_, _, msg) = sem058(src).into_iter().next().expect("one violation");
    assert!(msg.contains("Total"), "must name the function: {msg}");
}

#[test]
fn two_named_returns_are_counted_too() {
    // `return a : Integer = 1` names the slot `a`, not `return`, so counting by name alone missed
    // every named result and reported only the anonymous ones.
    let src = r#"
package P {
	calc def C {
		return a : Integer = 1;
		return b : Integer = 2;
	}
}
"#;
    assert_eq!(sem058(src).len(), 1, "got {:?}", sem058(src));
}

#[test]
fn one_named_return_is_accepted() {
    let src = r#"
package P {
	calc def C {
		return a : Integer = 1;
	}
}
"#;
    assert!(sem058(src).is_empty(), "got {:?}", sem058(src));
}

#[test]
fn a_function_with_two_result_expressions_is_rejected() {
    // `ResultExpressionMembership_Invalid.kerml.xt:26`: `function G :> F { 2 }` inherits the
    // result expression `F` writes, so it states two.
    let src = r#"
package P {
	function F {
		1
	}
	function G :> F {
		2
	}
}
"#;
    let found = sem058(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].1.contains("G"),
        "reported on the second expression, got {found:?}"
    );
}

#[test]
fn an_expression_inheriting_two_results_is_reported_on_itself() {
    // `ResultExpressionMembership_Invalid.kerml.xt:34`: `expr g :> f;` writes no body of its own.
    let src = r#"
package P {
	function F {
		1
	}
	expr f : F {
		1
	}
	expr g :> f;
}
"#;
    let found = sem058(src);
    assert_eq!(found.len(), 2, "got {found:?}");
    assert!(found.iter().any(|x| x.1 == "P::g"), "got {found:?}");
}

#[test]
fn a_function_with_one_result_expression_is_accepted() {
    let src = r#"
package P {
	function F {
		1
	}
}
"#;
    assert!(
        sem058(src).is_empty(),
        "one result is legal, got {:?}",
        sem058(src)
    );
}

#[test]
fn each_surplus_return_is_reported_on_its_own_declaration() {
    // `CalculationUsage_Invalid2.sysml.xt:45`: the assertion sits on `return r2 : T;`, not on the
    // calculation that owns it.
    let src = r#"
package P {
	attribute def T;
	calc def C1 {
		return r1 : T;
		return r2 : T;
	}
}
"#;
    let found = sem058(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].1, "P::C1::r2", "got {found:?}");
}
