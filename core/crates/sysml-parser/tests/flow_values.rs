use sysml_parser::ast::{BodyMember, RelationOp, Statement};
use sysml_parser::parse;

#[test]
fn flow_values_are_expressions_with_independent_operator_spans() {
    let src =
        "message <m> m default := source + 1 of <p> p : Fuel default = amount * 2 from a to b;";
    let parsed = parse(src).unwrap();
    assert!(
        parsed.document.errors().is_empty(),
        "{:?}",
        parsed.document.errors()
    );
    assert_eq!(parsed.document.members.len(), 1);
    let BodyMember::Statement(Statement::Flow { head, payload, .. }) = &parsed.document.members[0]
    else {
        panic!("expected a flow");
    };
    let payload = payload.as_ref().unwrap();
    assert!(head.value.is_some() && payload.value.is_some());
    assert!(head.is_default && head.is_initial);
    assert!(payload.is_default && !payload.is_initial);
    assert_eq!(
        parsed.slice(head.value_span.unwrap()),
        Some("default := source + 1")
    );
    assert_eq!(
        parsed.slice(payload.value_span.unwrap()),
        Some("default = amount * 2")
    );
    assert_eq!(head.from.as_deref(), Some("a"));
    assert_eq!(head.to.as_deref(), Some("b"));
}

#[test]
fn a_bare_payload_type_and_a_payload_identification_are_distinct() {
    for (clause, name, typed) in [
        ("Fuel", "", true),
        ("Types::Fuel[1]", "", true),
        ("Fuel = amount", "Fuel", false),
        (": Fuel = amount", "", true),
        ("= amount", "", false),
    ] {
        let src = format!("flow of {clause};");
        let parsed = parse(&src).unwrap();
        assert!(
            parsed.document.errors().is_empty(),
            "{src}: {:?}",
            parsed.document.errors()
        );
        assert_eq!(parsed.document.members.len(), 1, "{src}");
        let BodyMember::Statement(Statement::Flow {
            payload: Some(payload),
            ..
        }) = &parsed.document.members[0]
        else {
            panic!("expected a payload: {src}");
        };
        assert_eq!(payload.name.text, name, "{src}");
        assert_eq!(
            payload.target_of(RelationOp::Typing).is_some(),
            typed,
            "{src}"
        );
    }
}

#[test]
fn flow_value_lookahead_never_reads_the_next_member_or_a_declaration_body() {
    for src in [
        "flow f : Transfer { attribute value = 1; flow of Fuel; } item following;",
        "flow f : Transfer; flow g = source of Fuel; item following;",
        "flow f = (if flag ? leftValue else rightValue) of p : Fuel = amount; item following;",
        "message m default { source } of : Fuel { item carried; } item following;",
        "flow f = lookup(\"of\", a) of p : Fuel = amount; item following;",
        "flow f default frame.localClock of : Fuel; item following;",
    ] {
        let parsed = parse(src).unwrap();
        assert!(
            parsed.document.errors().is_empty(),
            "{src}: {:?}",
            parsed.document.errors()
        );
        let Some(BodyMember::Declaration(following)) = parsed.document.members.last() else {
            panic!("following member lost: {src}");
        };
        assert_eq!(following.head.name.text, "following", "{src}");
    }
}

#[test]
fn incomplete_values_and_specializations_report_errors_without_losing_siblings() {
    for bad in [
        "flow f = ;",
        "flow f := ;",
        "flow f default ;",
        "flow f of p = ;",
        "message m of p : Fuel default := ;",
        "flow f of p : ;",
        "flow f of p : Fuel, ;",
        "flow f of p :> ;",
        "flow f of : Fuel :>> ;",
        "flow f of <p>;",
        "flow f of p[1] = amount;",
        "flow f of p : Fuel[1][2];",
        "flow f of [1] Fuel[2];",
        "flow f of p : Fuel[1;",
        "flow f of p : Fuel a to b;",
        "flow f = source a to b;",
    ] {
        let src = format!("package P {{ {bad} item following; }}");
        let parsed = parse(&src).unwrap();
        assert!(!parsed.document.errors().is_empty(), "must reject: {bad}");
        let BodyMember::Declaration(package) = &parsed.document.members[0] else {
            panic!("package lost: {bad}");
        };
        let Some(BodyMember::Declaration(following)) = package.body.last() else {
            panic!("following member lost: {bad}");
        };
        assert_eq!(following.head.name.text, "following", "{bad}");
    }
}

#[test]
fn value_only_flows_and_succession_flows_keep_their_values() {
    for src in [
        "flow f = source;",
        "message m default source;",
        "flow := source;",
        "succession flow f = source of p : Fuel = amount;",
    ] {
        let parsed = parse(src).unwrap();
        assert!(
            parsed.document.errors().is_empty(),
            "{src}: {:?}",
            parsed.document.errors()
        );
        assert_eq!(parsed.document.members.len(), 1, "{src}");
        let BodyMember::Statement(Statement::Flow { head, .. }) = &parsed.document.members[0]
        else {
            panic!("expected a flow: {src}");
        };
        assert!(head.value.is_some(), "{src}");
        assert!(head.from.is_none() && head.to.is_none(), "{src}");
    }
}
