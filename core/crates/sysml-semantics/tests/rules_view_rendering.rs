//! A view states at most one rendering, and a rendering belongs to a view.

use sysml_semantics::{check, Severity};

fn sem043(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM043")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_view_with_two_renderings_is_rejected() {
    let src = r#"
package P {
	view def V {
		render r1;
		render r2;
	}
}
"#;
    assert_eq!(sem043(src).len(), 1, "got {:?}", sem043(src));
}

#[test]
fn a_view_with_one_rendering_is_accepted() {
    let src = r#"
package P {
	view def V {
		render r1;
	}
}
"#;
    assert!(sem043(src).is_empty());
}

#[test]
fn a_rendering_outside_a_view_is_rejected() {
    let src = r#"
package P {
	part def Holder {
		render r;
	}
}
"#;
    let found = sem043(src);
    assert!(
        found
            .iter()
            .any(|(_, _, m)| m.contains("validateViewRenderingMembershipOwningType")),
        "got {found:?}"
    );
}

#[test]
fn the_owning_type_report_names_the_owner() {
    let src = r#"
package P {
	part def Holder {
		render r;
	}
}
"#;
    let (_, _, msg) = sem043(src).into_iter().next().expect("one violation");
    assert!(msg.contains("Holder"), "must name the owner: {msg}");
    assert!(msg.contains("part def"), "must name its kind: {msg}");
}

#[test]
fn each_surplus_rendering_is_reported_on_its_own_declaration() {
    // The view is legal in shape; the declarations past the first are what a reader removes, so
    // each one carries the report. `ViewRendering_invalid.sysml.xt:45,52,54` asserts at
    // `render rendering r2;`, `r4` and `r5` for exactly this reason.
    let src = r#"
package P {
	rendering def R;
	view def V {
		render r1 : R;
		render r2 : R;
		render r3 : R;
	}
}
"#;
    let found = sem043(src);
    assert_eq!(found.len(), 2, "two surplus renderings: {found:?}");
    assert!(
        found.iter().any(|(_, at, _)| at.ends_with("r2")),
        "got {found:?}"
    );
    assert!(
        found.iter().any(|(_, at, _)| at.ends_with("r3")),
        "got {found:?}"
    );
}

#[test]
fn a_plain_rendering_usage_does_not_count_towards_the_bound() {
    // `rendering r2 : R` is an ordinary usage member, not a `ViewRenderingMembership`
    // (`SysML.xtext:2335-2343` is the only production that makes one). Counting it reported the
    // pilot's legal body at `ViewRendering_invalid.sysml.xt:46`.
    let src = r#"
package P {
	rendering def R;
	view def V {
		render r1 : R;
		rendering r2 : R;
	}
}
"#;
    assert!(sem043(src).is_empty(), "got {:?}", sem043(src));
}

#[test]
fn a_rendering_definition_in_a_view_body_does_not_count_either() {
    let src = r#"
package P {
	rendering def R;
	view def V {
		rendering def R2;
		render r : R;
	}
}
"#;
    assert!(sem043(src).is_empty(), "got {:?}", sem043(src));
}
