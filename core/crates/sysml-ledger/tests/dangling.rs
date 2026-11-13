//! A decision about an element the model no longer has.
//!
//! The command line refuses `--element` for a name the model does not declare, so a record can only
//! become dangling afterwards — a rename leaves it behind. That is the case this pins: the record
//! was true when it was written, and now describes a version of the model nobody can open.
//!
//! It matters because the ledger is what a reader uses to judge how much thought went into a number.
//! A list of decisions that includes three about elements that do not exist reads as coverage.

use sysml_ledger::{audit, Decision, Ledger, Origin};

fn decision(id: &str, element: &str) -> Decision {
    Decision {
        id: id.to_string(),
        element: element.to_string(),
        value: String::new(),
        origin: Origin::Assumed,
        why: "because".to_string(),
        falsifier: "if it differs".to_string(),
        inputs: Vec::new(),
        question: None,
        decided_by: "agent".to_string(),
        approved_by: None,
        cites: Vec::new(),
    }
}

const MODEL: &str = r#"
package P {
    private import ScalarValues::*;
    part def S {
        attribute x : Real = 1;
    }
}
"#;

#[test]
fn a_decision_about_a_renamed_element_is_reported() {
    let loaded = sysml_syntax::load(&MODEL).expect("parses");
    let ledger = Ledger {
        decisions: vec![
            decision("D001", "P::S::x"),
            // `ipRating` became `dockIpRating` and this record stayed behind.
            decision("D002", "P::S::ipRating"),
        ],
        ..Ledger::default()
    };
    let audit = audit(&loaded.roots, &ledger);
    let dangling: Vec<&str> = audit
        .issues
        .iter()
        .filter(|i| i.code == "PROV021")
        .map(|i| i.at.as_str())
        .collect();
    assert_eq!(dangling, vec!["P::S::ipRating"]);
    // An error, not a warning: a ledger that describes a model nobody can open is not a note.
    let issue = audit
        .issues
        .iter()
        .find(|i| i.code == "PROV021")
        .expect("reported");
    assert_eq!(issue.severity, sysml_ledger::Severity::Error);
    assert!(
        issue.message.contains("D002"),
        "the message must name the record: {}",
        issue.message
    );
    assert!(
        issue.message.contains("retract"),
        "and how to fix it: {}",
        issue.message
    );
}

#[test]
fn a_decision_about_a_part_definition_is_not_dangling() {
    // Not every decision is about a value: `part def Dock` gets one for existing at all.
    let loaded = sysml_syntax::load(&MODEL).expect("parses");
    let ledger = Ledger {
        decisions: vec![decision("D001", "P::S"), decision("D002", "P::S::x")],
        ..Ledger::default()
    };
    let audit = audit(&loaded.roots, &ledger);
    assert!(
        !audit.issues.iter().any(|i| i.code == "PROV021"),
        "{:?}",
        audit.issues.iter().map(|i| i.code).collect::<Vec<_>>()
    );
}
