//! The wording and count of the reference's connector featuring check.
//!
//! `validateConnectorTypeFeaturing` reports a related feature that is not accessible from the
//! connector's context, once per end, with the message "Must be an accessible feature (use dot
//! notation for nesting)" (`KerMLValidator.xtend:271`, `:1031`). The kernel reports it under
//! `REF001` for connection, binding and flow ends.

use sysml_semantics::check;

const WORDING: &str = "Must be an accessible feature (use dot notation for nesting)";

/// The `REF001` messages a model produces.
fn ref001(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.code == "REF001")
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn a_connection_end_written_with_colons_uses_the_reference_wording() {
    let found = ref001(
        "package P {\n\tpart def W { part hub; }\n\tpart car {\n\t\tpart w : W;\n\t\tpart e;\n\t\tconnection c connect w::hub to e;\n\t}\n}\n",
    );
    assert_eq!(found.len(), 1, "one end, one finding: {found:?}");
    assert!(found[0].starts_with(WORDING), "{found:?}");
}

#[test]
fn a_binding_end_written_with_colons_uses_the_reference_wording() {
    let found = ref001(
        "package P {\n\tpart def W { part hub; }\n\tpart car {\n\t\tpart w : W;\n\t\tpart e;\n\t\tbind w::hub = e;\n\t}\n}\n",
    );
    assert_eq!(found.len(), 1, "one end, one finding: {found:?}");
    assert!(found[0].starts_with(WORDING), "{found:?}");
}

#[test]
fn a_flow_end_reached_with_colons_draws_both_reference_messages() {
    // The reference reports two findings on such an end: it cannot identify the flow end
    // (`validateFlowEndSubsetting`) and the related feature is not accessible
    // (`validateConnectorTypeFeaturing`). `Relationship_invalid_relatedElement1.sysml.xt:47-54`
    // asserts both, at each end.
    let found = ref001(
        "package P {\n\tpart def W { part hub { out attribute x : ScalarValues::Real; } }\n\tpart car {\n\t\tpart w : W;\n\t\tpart e { in attribute y : ScalarValues::Real; }\n\t\tflow of ScalarValues::Real from w::hub.x to e.y;\n\t}\n}\n",
    );
    assert_eq!(found.len(), 2, "two findings for the one end: {found:?}");
    assert!(
        found
            .iter()
            .any(|m| m.starts_with("Cannot identify flow end (use dot notation)")),
        "{found:?}"
    );
    assert!(found.iter().any(|m| m.starts_with(WORDING)), "{found:?}");
}

#[test]
fn a_connection_end_outside_its_instance_uses_the_reference_wording() {
    let found = ref001(
        "package P {\n\tpart other;\n\tpart car {\n\t\tpart e;\n\t\tconnection c connect other to e;\n\t}\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with(WORDING), "{found:?}");
}

#[test]
fn a_connection_whose_ends_are_reached_with_dots_is_silent() {
    let found = ref001(
        "package P {\n\tpart def W { part hub; }\n\tpart car {\n\t\tpart w : W;\n\t\tpart e;\n\t\tconnection c connect w.hub to e;\n\t}\n}\n",
    );
    assert!(found.is_empty(), "{found:?}");
}
