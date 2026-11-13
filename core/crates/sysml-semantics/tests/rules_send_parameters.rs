//! Failing-model tests for `validateSendActionParameters` (`SEM123`).
//!
//! A send action owns at least three input parameters: its payload, sender and receiver. The
//! notation gives a send three exactly when it writes `to`, or when its body declares the missing
//! ones (`SendNode`, `SenderReceiverPart`, `SysML.xtext:1499-1521`).

use sysml_semantics::check;

/// The `SEM123` messages for a model whose action body holds `statement`.
fn sem123(statement: &str) -> Vec<String> {
    let src = format!(
        "package P {{\n\tpart def Q {{ port p; }}\n\tattribute def S;\n\taction def A {{\n\t\tpart q : Q;\n\t\tattribute s : S;\n\t\t{statement}\n\t}}\n}}\n"
    );
    let loaded = sysml_syntax::load_project(&[src.as_str()], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.code == "SEM123")
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn a_send_without_a_receiver_is_reported() {
    for statement in ["send s;", "send s via q.p;", "send;"] {
        let found = sem123(statement);
        assert_eq!(
            found.len(),
            1,
            "`{statement}` owns fewer than three input parameters: {found:?}"
        );
    }
}

#[test]
fn a_send_that_names_its_receiver_is_not_reported() {
    for statement in [
        "send s to q;",
        "send s via q.p to q;",
        "send to q;",
        "send S() to q;",
        "send new S() to q;",
    ] {
        let found = sem123(statement);
        assert!(
            found.is_empty(),
            "`{statement}` owns three input parameters: {found:?}"
        );
    }
}

#[test]
fn input_parameters_declared_in_the_body_count() {
    let found = sem123("send s via q.p { in extra; }");
    assert!(
        found.is_empty(),
        "the payload, the sender and the body's parameter make three: {found:?}"
    );
}

#[test]
fn the_message_says_how_many_the_send_owns() {
    let found = sem123("send s via q.p;");
    assert!(found.iter().any(|m| m.contains("owns 2")), "{found:?}");
}
