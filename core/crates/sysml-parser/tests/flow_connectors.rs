use sysml_parser::ast::{BodyMember, RelationOp, Statement};
use sysml_parser::parse;

#[test]
fn named_flows_keep_the_payload_and_both_ends_in_one_statement() {
    for header in [
        "flow fuel",
        "flow <f> fuel",
        "flow <f>",
        "flow fuel : Transfer",
        "flow <f> fuel : Transfer, OtherTransfer",
        "message <f> fuel : Transfer",
    ] {
        let src = format!("{header} of <p> payload : Fuel from a.b to c.d;");
        let parsed = parse(&src).expect(&src);
        assert!(
            parsed.document.errors().is_empty(),
            "{src}: {:?}",
            parsed.document.errors()
        );
        assert_eq!(parsed.document.members.len(), 1, "{src}");
        let BodyMember::Statement(Statement::Flow { head, payload, .. }) =
            &parsed.document.members[0]
        else {
            panic!("expected one flow statement: {src}");
        };
        if header.contains("<f>") {
            assert_eq!(head.name.short.as_deref(), Some("f"), "{src}");
        }
        assert_eq!(
            head.name.text,
            if header == "flow <f>" { "" } else { "fuel" },
            "{src}"
        );
        assert_eq!(
            head.targets_of(RelationOp::Typing).len(),
            if header.contains("OtherTransfer") {
                2
            } else if header.contains("Transfer") {
                1
            } else {
                0
            },
            "{src}"
        );
        let payload = payload.as_ref().expect("payload");
        assert_eq!(payload.name.text, "payload", "{src}");
        assert_eq!(payload.name.short.as_deref(), Some("p"), "{src}");
        assert_eq!(
            payload
                .target_of(RelationOp::Typing)
                .map(|t| t.text.as_str()),
            Some("Fuel"),
            "{src}"
        );
        assert_eq!(head.from.as_deref(), Some("a.b"), "{src}");
        assert_eq!(head.to.as_deref(), Some("c.d"), "{src}");
    }
}

#[test]
fn a_short_name_does_not_consume_the_next_flow_clause() {
    for src in [
        "flow <f> of <p> : Fuel from a to b;",
        "flow of <p> : Fuel from a to b;",
    ] {
        let parsed = parse(src).expect(src);
        assert!(
            parsed.document.errors().is_empty(),
            "{src}: {:?}",
            parsed.document.errors()
        );
        assert_eq!(parsed.document.members.len(), 1, "{src}");
        let BodyMember::Statement(Statement::Flow { head, payload, .. }) =
            &parsed.document.members[0]
        else {
            panic!("expected a flow: {src}");
        };
        assert!(head.name.text.is_empty(), "{src}");
        let payload = payload.as_ref().expect("payload");
        assert!(payload.name.text.is_empty(), "{src}");
        assert_eq!(payload.name.short.as_deref(), Some("p"), "{src}");
        assert_eq!(head.from.as_deref(), Some("a"), "{src}");
        assert_eq!(head.to.as_deref(), Some("b"), "{src}");
    }
}

#[test]
fn flow_lookahead_stops_before_declaration_bodies_and_following_members() {
    for src in [
        "flow <f> fuel : Transfer { flow of payload : Fuel from a to b; }",
        "abstract flow <f> fuel : Transfer[0..*] { flow from a to b; }",
        "abstract flow <f> fuel : Transfer from a to b;",
        "flow fuel : Transfer; flow of payload : Fuel from a to b;",
    ] {
        let parsed = parse(src).expect(src);
        assert!(
            parsed.document.errors().is_empty(),
            "{src}: {:?}",
            parsed.document.errors()
        );
        let head = match &parsed.document.members[0] {
            BodyMember::Declaration(declaration) => &declaration.head,
            BodyMember::Statement(Statement::Flow { head, .. }) if !src.contains('{') => head,
            other => panic!("lookahead read across the declaration boundary: {src}: {other:?}"),
        };
        assert_eq!(head.name.text, "fuel", "{src}");
        if src.starts_with("abstract") {
            assert!(head.prefixes.is_abstract, "{src}");
        }
        if src.contains("<f>") {
            assert_eq!(head.name.short.as_deref(), Some("f"), "{src}");
        }
        assert_eq!(
            parsed.document.members.len(),
            if src.contains("; flow") { 2 } else { 1 },
            "{src}"
        );
    }
}

#[test]
fn a_payload_flow_may_omit_both_ends() {
    for src in [
        "message m of Publish[1];",
        "flow f of payload : Fuel[0..*];",
        "flow of Types::Fuel[1];",
        "flow of [1] Types::Fuel;",
        "flow <f> fuel[2] : Transfer of <p> payload[1] : Fuel;",
        "flow f of payload : Fuel { item carried; }",
    ] {
        let parsed = parse(src).expect(src);
        assert!(
            parsed.document.errors().is_empty(),
            "{src}: {:?}",
            parsed.document.errors()
        );
        assert_eq!(parsed.document.members.len(), 1, "{src}");
        let BodyMember::Statement(Statement::Flow { head, payload, .. }) =
            &parsed.document.members[0]
        else {
            panic!("expected a flow: {src}");
        };
        assert!(payload.is_some(), "{src}");
        assert!(head.from.is_none() && head.to.is_none(), "{src}");
    }
}

#[test]
fn an_incomplete_flow_end_group_is_still_an_error() {
    for src in [
        "flow f of payload : Fuel from a;",
        "message m of Publish[1] from a;",
        "flow a; flow of Fuel from a to;",
    ] {
        let source = format!("{src} item following;");
        let parsed = parse(&source).expect("body errors are recoverable");
        assert!(
            !parsed.document.errors().is_empty(),
            "must reject incomplete ends: {src}"
        );
        let Some(BodyMember::Declaration(following)) = parsed.document.members.last() else {
            panic!("error recovery lost the following member: {src}");
        };
        assert_eq!(following.head.name.text, "following", "{src}");
    }
}
