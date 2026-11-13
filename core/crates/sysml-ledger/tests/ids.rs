//! Ledger behaviour that the first version got wrong.
//!
//! Both cases here were found by an agent using the tool in anger, not by a test: it hit a
//! duplicate id after retracting an entry, and it had no way to record that a person had
//! accepted an assumption.

use sysml_ledger::{Decision, Ledger, Origin, Question, QuestionState};

fn decision(id: &str, origin: Origin) -> Decision {
    Decision {
        id: id.to_string(),
        element: "P::S::m".to_string(),
        value: "1".to_string(),
        origin,
        why: "because".to_string(),
        falsifier: "if the measurement differs".to_string(),
        inputs: Vec::new(),
        question: None,
        decided_by: "agent".to_string(),
        approved_by: None,
        cites: Vec::new(),
    }
}

#[test]
fn a_retracted_entry_does_not_free_its_id_for_reuse() {
    // The first implementation numbered by `len() + 1`. Remove one of three entries and the
    // next id collided with a live one, which makes every reference to it ambiguous — the exact
    // defect the tool exists to prevent, occurring inside the tool.
    let mut ledger = Ledger::default();
    ledger.decisions.push(decision("D001", Origin::Assumed));
    ledger.decisions.push(decision("D002", Origin::Assumed));
    ledger.decisions.push(decision("D003", Origin::Assumed));
    assert_eq!(ledger.next_decision_id(), "D004");

    ledger.remove_decision("D002");
    assert_eq!(
        ledger.next_decision_id(),
        "D004",
        "removing an entry must not hand its number to the next one"
    );
}

#[test]
fn ids_are_numbered_from_the_highest_in_use_not_the_count() {
    let mut ledger = Ledger::default();
    ledger.questions.push(Question {
        id: "Q007".to_string(),
        at: "P::S".to_string(),
        text: "which?".to_string(),
        blocks: Vec::new(),
        state: QuestionState::Open,
        answer: None,
        answered_by: None,
        asked_by: "agent".to_string(),
        raised_at: None,
    });
    assert_eq!(ledger.next_question_id(), "Q008");
}

#[test]
fn a_non_numeric_id_does_not_stop_the_sequence() {
    let mut ledger = Ledger::default();
    ledger
        .decisions
        .push(decision("D-hand-written", Origin::Spec));
    ledger.decisions.push(decision("D002", Origin::Spec));
    assert_eq!(ledger.next_decision_id(), "D003");
}

#[test]
fn only_an_assumption_can_be_approved() {
    let mut ledger = Ledger::default();
    ledger.decisions.push(decision("D001", Origin::Spec));
    let err = ledger.approve("D001", "human:jason").unwrap_err();
    assert!(
        err.contains("not an assumption"),
        "a specification value needs no approval, got: {err}"
    );
}

#[test]
fn an_approval_is_not_overwritten() {
    // Who accepted the risk is the one fact that must not be replaceable: a second name would
    // erase the person who actually said yes.
    let mut ledger = Ledger::default();
    ledger.decisions.push(decision("D001", Origin::Assumed));
    ledger
        .approve("D001", "human:jason")
        .expect("first approval");
    let err = ledger.approve("D001", "human:someone-else").unwrap_err();
    assert!(err.contains("already approved"), "got: {err}");
    assert_eq!(
        ledger.decisions[0].approved_by.as_deref(),
        Some("human:jason")
    );
}

#[test]
fn approving_something_that_is_not_there_is_an_error() {
    let mut ledger = Ledger::default();
    assert!(ledger.approve("D999", "human:jason").is_err());
}

#[test]
fn an_approval_must_name_a_person() {
    // The check lives beside `approve` rather than in the CLI, because the CLI had a second way
    // to write the same field: `decide --approved-by` took any string, and a bare name silenced
    // the gate that reports unapproved assumptions. One writer bypassing the rule is the same
    // as having no rule.
    for named in ["Jason", "agent", "human:", ""] {
        let err = match sysml_ledger::check_approver(named) {
            Ok(()) => panic!("`{named}` was accepted as an approver"),
            Err(message) => message,
        };
        assert!(err.contains("human:<name>"), "got: {err}");
    }
    sysml_ledger::check_approver("human:jason").expect("a named person is accepted");
}
