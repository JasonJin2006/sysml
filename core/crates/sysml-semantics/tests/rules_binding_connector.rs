//! `validateBindingConnectorIsBinary`: a binding joins exactly two features.

use sysml_semantics::{check, Severity};

fn sem060(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM060")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_three_ended_binding_is_rejected() {
    let src = r#"
package P {
	part p;
	part q;
	part r;
	bind p = q = r;
}
"#;
    assert_eq!(sem060(src).len(), 1, "got {:?}", sem060(src));
}

#[test]
fn a_two_ended_binding_is_accepted() {
    let src = r#"
package P {
	part p;
	part q;
	bind p = q;
}
"#;
    assert!(sem060(src).is_empty());
}

#[test]
fn the_third_end_does_not_become_a_stray_member() {
    // Before the parser collected extra ends, `= r` fell into the body and the model gained an
    // element it never wrote.
    let src = r#"
package P {
	part p;
	part q;
	part r;
	bind p = q = r;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let p = loaded.roots.iter().find(|e| e.name == "P").expect("P");
    let names: Vec<_> = p.children.iter().map(|c| c.name.clone()).collect();
    assert!(
        !names.iter().any(|n| n == "<anonymous>"),
        "no stray member, got {names:?}"
    );
}

#[test]
fn a_connection_end_that_nests_into_a_feature_with_colons_is_rejected() {
    // `c::aa` names `aa` inside what `c` is. `::` is for namespaces; a feature's members are
    // reached with `.`, which the pilot states as "Must be an accessible feature (use dot notation
    // for nesting)" (`FeaturePath_Invalid.sysml.xt:61`).
    let src = r#"
package P {
	part def F {
		part a;
	}
	part def B {
		part f : F;
	}
	part def C {
		part aa;
	}
	part b : B;
	part c : C;
	connect b.f.a to c::aa;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let found: Vec<String> = check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| {
            i.severity == Severity::Error && i.message.contains("use dot notation for nesting")
        })
        .map(|i| i.message.clone())
        .collect();
    assert_eq!(found.len(), 1, "got {found:?}");
}

#[test]
fn a_binding_end_that_nests_into_a_feature_with_colons_is_rejected() {
    // A binding is a binary relationship whose ends are written like a connection's, and the pilot
    // reports the same accessibility error for `bind B0::p0.myDefIn = …`
    // (`BindingConnector_redefine.sysml.xt:106`). Reading only `Kind::Connection` left every
    // binding end unjudged.
    let src = r#"
package P {
	part def B {
		part y {
			part z;
		}
		bind y::z = y::z;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let found: Vec<String> = check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.message.contains(": binding end"))
        .map(|i| i.message.clone())
        .collect();
    assert_eq!(found.len(), 2, "one per end, got {found:?}");
}

/// Every error the model draws, as (message, line).
fn ends(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "REF001")
        .map(|i| i.message.clone())
        .collect()
}

#[test]
fn a_connection_end_that_names_a_part_of_a_definition_is_rejected() {
    // `A::x` names a part of the type `A` and `y::z` a part of `B`'s own part. An end names a
    // feature of an instance, and neither is one (`Connector_Invalid.sysml.xt:45`).
    let found = ends(
        r#"
package P {
	part def A {
		part x;
	}
	part def B {
		part y {
			part z;
		}
		connect A::x to y::z;
	}
}
"#,
    );
    assert_eq!(found.len(), 2, "both ends, got {found:?}");
}

#[test]
fn a_flow_end_that_names_a_reference_of_a_definition_is_rejected() {
    // `flow XXX from B::myOut to C::myIn` — the ends are refs of the definitions `B` and `C`.
    // Flows were not walked at all, and `flow XXX from …` did not parse, so this line carried five
    // assertions and none of them could be met (`Relationship_invalid_relatedElement1.sysml.xt:54`).
    let found = ends(
        r#"
package P {
	part def A {
		part def B {
			out ref myOut;
		}
		part def C {
			in ref myIn;
		}
		flow XXX from B::myOut to C::myIn;
	}
}
"#,
    );
    assert!(
        found.iter().filter(|m| m.contains(": flow end")).count() == 2,
        "both flow ends, got {found:?}"
    );
}

#[test]
fn an_end_that_reaches_out_of_its_instance_is_rejected() {
    // `z` is a part of `B`, not of `y`. Written inside `y`, the bare name resolves by walking out
    // of the instance, and the pilot reports it (`Connector_Invalid.sysml.xt:50`). `x`, which `y`
    // inherits from its type, is not this case.
    let found = ends(
        r#"
package P {
	part def A {
		part x;
	}
	part def B {
		part y : A {
			connect x to z;
		}
		part z;
	}
}
"#,
    );
    assert_eq!(found.len(), 1, "only `z`, got {found:?}");
    assert!(found[0].contains("`z`"), "got {found:?}");
}

/// Every SEM060 finding, warning or error: the conformance half is a warning, so `sem060` above
/// (errors only) does not see it.
fn sem060_any(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.code == "SEM060")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn binding_ends_with_unrelated_types_are_warned_about() {
    // `BindingConnector_invalid0.sysml.xt:41`: both ends are dotted paths, and the single-name
    // lookup never resolved `b1.a`, so the conformance check below it never ran.
    let src = r#"
package P {
	part def A1;
	part def A2;
	part b {
		part b1 { out ref a : A1; }
		part b2 { in ref a2 : A2; }
		bind b1.a = b2.a2;
	}
}
"#;
    let found = sem060_any(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("unrelated types") && m.contains("A1")),
        "got {found:?}"
    );
}

#[test]
fn binding_ends_of_the_same_type_are_accepted() {
    let src = r#"
package P {
	part def A1;
	part b {
		part b1 { out ref a : A1; }
		part b2 { in ref a1 : A1; }
		bind b1.a = b2.a1;
	}
}
"#;
    assert!(sem060_any(src).is_empty(), "got {:?}", sem060_any(src));
}
