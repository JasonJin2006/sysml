use sysml_model::Kind;
use sysml_syntax::load_project;

#[test]
fn flow_connector_short_names_survive_lowering() {
    for statement in [
        "flow <f> fuel from a to b;",
        "flow <f> fuel : Transfer from a to b;",
        "flow <f> fuel of payload : Fuel from a to b;",
        "message <f> fuel : Transfer from a to b;",
    ] {
        let src = format!("package P {{ {statement} }}");
        let loaded = load_project(&[&src], false).expect(&src);
        let members = &loaded.roots[0].children;
        assert_eq!(members.len(), 1, "{src}");
        let flow = &members[0];
        assert_eq!(flow.kind, Kind::FlowUsage, "{src}");
        assert_eq!(flow.name, "fuel", "{src}");
        assert_eq!(flow.short_name.as_deref(), Some("f"), "{src}");
        assert_eq!(flow.from.as_deref(), Some("a"), "{src}");
        assert_eq!(flow.to.as_deref(), Some("b"), "{src}");
    }
}

#[test]
fn named_flow_payloads_keep_their_identity_types_and_body() {
    let loaded = load_project(
        &["package P { flow <f> fuel : Transfer, OtherTransfer \
           of <p> payload : Fuel from a to b { item carried; } }"],
        false,
    )
    .expect("model parses");
    let members = &loaded.roots[0].children;
    assert_eq!(members.len(), 1);
    let flow = &members[0];
    assert_eq!(flow.type_refs, ["Transfer", "OtherTransfer"]);
    assert_eq!(flow.children.len(), 2);
    let payload = &flow.children[0];
    assert_eq!(payload.kind, Kind::PayloadFeature);
    assert_eq!(payload.name, "payload");
    assert_eq!(payload.short_name.as_deref(), Some("p"));
    assert_eq!(payload.type_ref.as_deref(), Some("Fuel"));
    assert_eq!(flow.children[1].name, "carried");
}

#[test]
fn short_only_flow_and_payload_names_do_not_become_clause_keywords() {
    let loaded = load_project(
        &["package P { flow <f> of <p> : Fuel from a to b; }"],
        false,
    )
    .expect("model parses");
    let members = &loaded.roots[0].children;
    assert_eq!(members.len(), 1);
    let flow = &members[0];
    assert_eq!(flow.name, "f");
    assert_eq!(flow.short_name.as_deref(), Some("f"));
    assert_eq!(flow.children.len(), 1);
    assert_eq!(flow.children[0].name, "p");
    assert_eq!(flow.children[0].short_name.as_deref(), Some("p"));
    assert_eq!(flow.from.as_deref(), Some("a"));
    assert_eq!(flow.to.as_deref(), Some("b"));
}

#[test]
fn payload_types_and_bounds_belong_to_the_payload_not_its_name() {
    for (statement, name, ty, bound) in [
        ("message m of Publish[1];", "", "Publish", "[1]"),
        (
            "flow f of payload : Fuel[0..*];",
            "payload",
            "Fuel",
            "[0..*]",
        ),
        ("flow of Types::Fuel[1];", "", "Types::Fuel", "[1]"),
        ("flow of [1] Types::Fuel;", "", "Types::Fuel", "[1]"),
        ("flow f of payload[1] : Fuel;", "payload", "Fuel", "[1]"),
    ] {
        let loaded = load_project(&[statement], false).expect(statement);
        assert_eq!(loaded.roots.len(), 1, "{statement}");
        let flow = &loaded.roots[0];
        assert_eq!(flow.kind, Kind::FlowUsage, "{statement}");
        assert!(flow.from.is_none() && flow.to.is_none(), "{statement}");
        assert_eq!(flow.children.len(), 1, "{statement}");
        let payload = &flow.children[0];
        assert_eq!(payload.kind, Kind::PayloadFeature, "{statement}");
        assert_eq!(payload.name, name, "{statement}");
        assert_eq!(payload.type_ref.as_deref(), Some(ty), "{statement}");
        assert_eq!(payload.multiplicity.as_deref(), Some(bound), "{statement}");
    }
}

#[test]
fn zero_end_messages_do_not_invent_sibling_members() {
    // The official 17b sequence model writes these messages without event endpoints.
    let loaded = load_project(
        &["package Sequence { message publish_message of Publish[1]; \
           message subscribe_message of Subscribe[1]; message deliver_message of Deliver[1]; \
           flow <f> fuel[2] : Transfer of <p> payload : Fuel[1] { item carried; } }"],
        false,
    )
    .expect("model parses");
    let members = &loaded.roots[0].children;
    assert_eq!(members.len(), 4);
    assert_eq!(
        members
            .iter()
            .map(|el| el.name.as_str())
            .collect::<Vec<_>>(),
        [
            "publish_message",
            "subscribe_message",
            "deliver_message",
            "fuel"
        ]
    );
    let flow = &members[3];
    assert_eq!(flow.multiplicity.as_deref(), Some("[2]"));
    assert_eq!(flow.children[0].multiplicity.as_deref(), Some("[1]"));
    assert_eq!(flow.children[0].short_name.as_deref(), Some("p"));
    assert_eq!(flow.children[1].name, "carried");
}
