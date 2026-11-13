//! `validateFeatureChainExpressionConformance`: a chained access reads what the base provides.
//!
//! `part v : V; v->mass` reads `mass` out of `v`, so `mass` has to be something `V` declares.
//! Both directions are proved, plus the two shapes that must stay silent: a sequence operation,
//! which is a function of the base's value rather than a member of its type, and a chain whose base
//! does not resolve at all, whose name the reference pass already reports.

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

fn reported(src: &str) -> bool {
    sem095(src)
        .iter()
        .any(|(_, _, m)| m.contains("validateFeatureChainExpressionConformance"))
}

#[test]
fn a_chain_reading_a_feature_the_base_lacks_is_rejected() {
    let src = r#"
package P {
	private import ScalarValues::*;
	part def V {
		attribute mass : Real;
	}
	part v : V;
	attribute bad : Real = v->nowhere;
}
"#;
    assert!(reported(src), "got {:?}", sem095(src));
}

#[test]
fn a_chain_reading_a_feature_of_the_type_is_accepted() {
    let src = r#"
package P {
	private import ScalarValues::*;
	part def V {
		attribute mass : Real;
	}
	part v : V;
	attribute good : Real = v->mass;
}
"#;
    assert!(!reported(src), "got {:?}", sem095(src));
}

#[test]
fn a_chain_reading_an_inherited_feature_is_accepted() {
    // `b` is declared on `Base`, and `d` is typed by `D :> Base`. Asking the base's own members and
    // then one level of its type stopped before the specialization closure, so this legal access was
    // reported as "provides no such feature".
    let src = r#"
package P {
	part def Base {
		attribute b;
	}
	part def D :> Base;
	part d : D;
	attribute x = d->b;
}
"#;
    assert!(!reported(src), "got {:?}", sem095(src));
}

#[test]
fn a_sequence_operation_is_not_a_feature_access() {
    // `frontWheel->#(1)` indexes: `#` is a kernel-library function of the base's value, and its
    // spelling is a symbol no type could declare.
    let src = r#"
package P {
	private import ScalarValues::*;
	part def Wheel {
		attribute radius : Real;
	}
    part wheels : Wheel[2];
	attribute first : Real = wheels->#(1)->radius;
}
"#;
    assert!(!reported(src), "got {:?}", sem095(src));
}

#[test]
fn a_library_function_reached_by_name_is_not_a_feature_access() {
    // `parts->size()` names a function the language provides rather than a member of `parts`' type.
    // Deciding that by resolving the name against the library needs an import to reach it, and this
    // model imports nothing, so the chain was reported as reading a feature `parts` does not have.
    // The language's own function table answers the question without an import.
    let src = r#"
package P {
	attribute parts : Integer[0..*];
	attribute n : Integer = parts->size();
	attribute total : Integer = parts->sum();
}
"#;
    assert!(!reported(src), "got {:?}", sem095(src));
}

#[test]
fn an_unresolved_base_is_not_judged_here() {
    // The reference pass reports `missing`; a rule about what it provides would say it twice.
    let src = r#"
package P {
	private import ScalarValues::*;
	attribute x : Real = missing->mass;
}
"#;
    assert!(!reported(src), "got {:?}", sem095(src));
}
