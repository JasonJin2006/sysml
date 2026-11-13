//! A redefinition keeps the end-ness and constancy it inherits —
//! `validateRedefinitionEndConformance` and `validateSubsettingConstantConformance`.

use sysml_semantics::{check, Severity};

fn sem074(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM074")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_redefinition_that_drops_end_is_rejected() {
    let src = r#"
package P {
	part def Frame;
	part def Holder {
		end part side : Frame;
		part notAnEnd :> side;
	}
}
"#;
    let found = sem074(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateRedefinitionEndConformance")),
        "got {found:?}"
    );
}

#[test]
fn a_redefinition_that_keeps_end_is_accepted() {
    let src = r#"
package P {
	part def Frame;
	part def Holder {
		end part side : Frame;
		end part also :> side;
	}
}
"#;
    assert!(sem074(src).is_empty(), "got {:?}", sem074(src));
}

#[test]
fn a_variable_subsets_a_constant_is_rejected() {
    let src = r#"
package P {
	part def Holder {
		constant attribute limit = 10;
		var attribute loose :> limit;
	}
}
"#;
    let found = sem074(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateSubsettingConstantConformance")),
        "got {found:?}"
    );
}

#[test]
fn a_variable_subsets_a_plain_attribute_is_accepted() {
    let src = r#"
package P {
	part def Holder {
		attribute limit = 10;
		var attribute loose :> limit;
	}
}
"#;
    assert!(sem074(src).is_empty(), "got {:?}", sem074(src));
}

#[test]
fn an_unresolved_target_is_left_alone() {
    let src = r#"
package P {
	part def Holder {
		end part side : Frame;
		part notAnEnd :> Nowhere;
	}
}
"#;
    assert!(
        sem074(src).is_empty(),
        "a dangling target is a reference problem, got {:?}",
        sem074(src)
    );
}
