//! The `var` / `constant` / `portion` / `end` rules, each proved by a model that breaks it.
//!
//! These four flags constrain each other, and `validateRedefinitionEndConformance` compares a
//! feature with the one it redefines. Every test here has a counterpart asserting the legal
//! spelling is not reported, because a rule that fires on correct models is worse than no rule.

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

fn sem034(src: &str) -> Vec<(String, String, String)> {
    issues(src)
        .into_iter()
        .filter(|(c, _, _)| c == "SEM034")
        .collect()
}

// -- validateRedefinitionEndConformance -------------------------------------

#[test]
fn a_redefinition_of_an_end_that_is_not_an_end_is_rejected() {
    let src = r#"
package P {
	part def A;
	connection def Link { end base : A; }
	part def Holder {
		part :>> P::Link::base;
	}
}
"#;
    let found = sem034(src);
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("redefines the `end` feature")),
        "redefining an end without writing `end` must be reported, got {found:?}"
    );
}

#[test]
fn a_redefinition_of_an_end_that_is_an_end_is_accepted() {
    let src = r#"
package P {
	part def A;
	connection def Link { end base : A; }
	part def Holder {
		end part :>> P::Link::base;
	}
}
"#;
    assert!(
        sem034(src).is_empty(),
        "an `end` redefining an `end` conforms"
    );
}

#[test]
fn a_redefinition_of_a_non_end_is_not_this_rule() {
    let src = r#"
package P {
	part def A;
	part def Holder {
		part ordinary : A;
		part copy :>> ordinary;
	}
}
"#;
    assert!(
        sem034(src).is_empty(),
        "`ordinary` is not an end, so its redefinition is not this rule's subject"
    );
}

// -- the three flag contradictions ------------------------------------------

#[test]
fn an_end_that_is_a_variable_must_be_constant() {
    let src = r#"
package P {
	part def A;
	part def Holder { end var e : A; }
}
"#;
    let found = sem034(src);
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("must also be constant")),
        "got {found:?}"
    );
}

#[test]
fn an_end_that_is_a_constant_variable_is_accepted() {
    let src = r#"
package P {
	part def A;
	part def Holder { end constant var e : A; }
}
"#;
    assert!(
        sem034(src).is_empty(),
        "`end constant var` satisfies both rules"
    );
}

#[test]
fn a_constant_that_is_not_a_variable_is_rejected() {
    let src = r#"
package P {
	attribute def AD;
	part def Holder { constant attribute k : AD; }
}
"#;
    let found = sem034(src);
    assert!(
        found
            .iter()
            .any(|(_, _, msg)| msg.contains("must also be a variable")),
        "got {found:?}"
    );
}

#[test]
fn a_constant_variable_is_accepted() {
    let src = r#"
package P {
	attribute def AD;
	part def Holder { constant var k : AD; }
}
"#;
    assert!(
        sem034(src).is_empty(),
        "`constant var` is the legal spelling"
    );
}

#[test]
fn a_variable_portion_is_rejected() {
    for word in ["portion", "snapshot", "timeslice"] {
        let src =
            format!("package P {{\n\tpart def A;\n\tpart def Holder {{ {word} var q : A; }}\n}}\n");
        let found = sem034(&src);
        assert!(
            found
                .iter()
                .any(|(_, _, msg)| msg.contains("is not a variable")),
            "`{word} var` must be reported, got {found:?}"
        );
    }
}

#[test]
fn a_portion_that_is_not_variable_is_accepted() {
    let src = r#"
package P {
	part def A;
	part def Holder { portion part q : A; }
}
"#;
    assert!(sem034(src).is_empty(), "a plain portion is legal");
}
