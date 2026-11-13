//! A port's non-port members are not composite.
//!
//! The check reads the effective `isComposite` the lowering fills, not the written word, so the
//! models that state a violation are the ordinary ones: `part b2 : B;` inside a port definition,
//! which is what the pilot suite asserts (`PortUsage_Invalid.sysml.xt:35`, `:50`).

use sysml_semantics::{check, Severity};

fn sem066(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM066")
        .map(|i| i.message.clone())
        .collect()
}

fn has(src: &str, needle: &str) -> bool {
    sem066(src).iter().any(|m| m.contains(needle))
}

#[test]
fn a_composite_part_inside_a_port_definition_is_rejected() {
    let src = r#"
package P {
	part def A;
	port def PD {
		composite part bad : A;
	}
}
"#;
    assert!(has(src, "composite inside"), "got {:?}", sem066(src));
}

#[test]
fn a_plain_part_inside_a_port_definition_is_rejected() {
    let src = r#"
package P {
	part def A;
	port def PD {
		part b2 : A;
	}
}
"#;
    // The pilot model's exact case: no modifier is written, and the usage is composite because a
    // port definition owns it. `PortUsage_Invalid.sysml.xt:35` expects the violation here.
    let found = sem066(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].contains("composite inside"), "got {found:?}");
}

#[test]
fn the_pilot_model_ports_both_violations_at_the_right_elements() {
    // The two assertions the OMG pilot suite states, at `part b2 : B;` in a port definition and
    // `part b4 : B;` nested in a port usage. Both elements are named, so a message that lands on
    // the wrong member does not pass.
    let src = r#"
package 'Port Example' {
	part def B;
	port def pd1 {
		port p1 : pd1;
		ref b1 : B;
		part b2 : B;
	}
	part def FA {
		port two_port_def_types : pd1 {
			port p2 : pd1;
			ref b3 : B;
			part b4 : B;
		}
	}
}
"#;
    let found = sem066(src);
    assert_eq!(found.len(), 2, "got {found:?}");
    assert!(
        found.iter().any(|m| m.contains("::b2` is composite")),
        "got {found:?}"
    );
    assert!(
        found.iter().any(|m| m.contains("::b4` is composite")),
        "got {found:?}"
    );
}

#[test]
fn an_attribute_member_of_a_port_is_not_composite() {
    // `validateAttributeUsageIsReference`: an attribute usage is referential, so a port that
    // carries one carries a value, not a part.
    let src = r#"
package P {
	port def PD {
		attribute level : Real;
	}
}
"#;
    assert!(sem066(src).is_empty(), "got {:?}", sem066(src));
}

#[test]
fn a_nested_port_is_exempt() {
    let src = r#"
package P {
	port def PD {
		composite port nested : PD;
	}
}
"#;
    assert!(sem066(src).is_empty(), "got {:?}", sem066(src));
}

#[test]
fn a_directed_member_is_referential_by_construction() {
    let src = r#"
package P {
	port def PD {
		in payload : Real;
	}
}
"#;
    assert!(
        sem066(src).is_empty(),
        "`in x` is referential, got {:?}",
        sem066(src)
    );
}

#[test]
fn a_reference_member_is_accepted() {
    let src = r#"
package P {
	part def B;
	port def PD {
		ref b1 : B;
	}
}
"#;
    assert!(sem066(src).is_empty(), "got {:?}", sem066(src));
}

#[test]
fn a_composite_part_elsewhere_is_not_judged() {
    let src = r#"
package P {
	part def A;
	part def Holder {
		composite part x : A;
	}
}
"#;
    assert!(
        sem066(src).is_empty(),
        "only a port's members are bounded, got {:?}",
        sem066(src)
    );
}
