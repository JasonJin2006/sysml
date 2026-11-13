//! A send keeps its `via` and `to` ends apart, its payload may be any expression, and its own body
//! is kept (`SendNode`, `SenderReceiverPart`, `SysML.xtext:1499-1533`). An accept's `via` names its
//! receiver (`AcceptParameterPart`, `:1450-1453`).

use sysml_model::{Direction, Element, Kind};

fn roots(src: &str) -> Vec<Element> {
    sysml_syntax::load_project(&[src], false)
        .expect("model parses")
        .roots
}

fn collect<'a>(els: &'a [Element], kind: &Kind, out: &mut Vec<&'a Element>) {
    for el in els {
        if &el.kind == kind {
            out.push(el);
        }
        collect(&el.children, kind, out);
    }
}

fn the_one(roots: &[Element], kind: Kind) -> Element {
    let mut found = Vec::new();
    collect(roots, &kind, &mut found);
    assert_eq!(found.len(), 1, "expected one {kind:?}: {found:?}");
    found[0].clone()
}

fn syntax_errors(roots: &[Element]) -> Vec<String> {
    let mut found = Vec::new();
    collect(roots, &Kind::Unsupported, &mut found);
    found
        .into_iter()
        .filter(|e| e.name == "syntax error")
        .map(|e| format!("line {}: {:?}", e.line, e.constraint))
        .collect()
}

fn model(statement: &str) -> String {
    format!(
        "package P {{\n\tpart def Q {{ port p; attribute sig; }}\n\tattribute def S;\n\taction def A {{\n\t\tpart q : Q;\n\t\tattribute s : S;\n\t\t{statement}\n\t}}\n}}\n"
    )
}

#[test]
fn a_send_keeps_its_sender_and_its_receiver_apart() {
    let roots = roots(&model("send s via q.p to q;"));
    assert!(
        syntax_errors(&roots).is_empty(),
        "{:?}",
        syntax_errors(&roots)
    );
    let send = the_one(&roots, Kind::SendActionUsage);
    assert_eq!(send.sender.as_deref(), Some("q.p"));
    assert_eq!(send.receiver.as_deref(), Some("q"));
    assert_eq!(
        send.addressed.as_deref(),
        Some("q"),
        "the lifeline end is the receiver"
    );
    assert!(send.has_payload);
}

#[test]
fn a_send_without_a_payload_parses() {
    for (statement, sender, receiver) in [
        ("send via q.p;", Some("q.p"), None),
        ("send to q;", None, Some("q")),
    ] {
        let roots = roots(&model(statement));
        assert!(
            syntax_errors(&roots).is_empty(),
            "`{statement}`: {:?}",
            syntax_errors(&roots)
        );
        let send = the_one(&roots, Kind::SendActionUsage);
        assert!(!send.has_payload, "`{statement}` writes no payload");
        assert_eq!(send.sender.as_deref(), sender, "`{statement}`");
        assert_eq!(send.receiver.as_deref(), receiver, "`{statement}`");
    }
}

#[test]
fn a_send_payload_may_be_any_expression() {
    for statement in ["send S() to q;", "send q.sig to q;", "send 5 to q;"] {
        let roots = roots(&model(statement));
        assert!(
            syntax_errors(&roots).is_empty(),
            "`{statement}`: {:?}",
            syntax_errors(&roots)
        );
        let send = the_one(&roots, Kind::SendActionUsage);
        assert!(send.has_payload, "`{statement}` writes a payload");
        assert_eq!(send.receiver.as_deref(), Some("q"), "`{statement}`");
    }
}

#[test]
fn an_invoked_payload_names_its_type_as_the_constructor_form_does() {
    let roots = roots(&model("send S() to q;"));
    let send = the_one(&roots, Kind::SendActionUsage);
    assert_eq!(send.type_ref.as_deref(), Some("S"));
}

#[test]
fn a_send_body_is_kept_as_its_members() {
    let roots = roots(&model("send s via q.p { in extra; }"));
    assert!(
        syntax_errors(&roots).is_empty(),
        "{:?}",
        syntax_errors(&roots)
    );
    let send = the_one(&roots, Kind::SendActionUsage);
    let extra = send
        .children
        .iter()
        .find(|c| c.name == "extra")
        .expect("the body's parameter is a member of the send");
    assert_eq!(extra.direction, Some(Direction::In));
}

#[test]
fn an_accept_names_its_receiver_with_via() {
    let roots = roots(&model("accept sig2 : S via q.p;"));
    assert!(
        syntax_errors(&roots).is_empty(),
        "{:?}",
        syntax_errors(&roots)
    );
    let accept = the_one(&roots, Kind::AcceptActionUsage);
    assert_eq!(accept.receiver.as_deref(), Some("q.p"));
    assert_eq!(accept.sender, None);
}
