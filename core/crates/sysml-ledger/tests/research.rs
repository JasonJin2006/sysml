//! Research must be a gate, not an honour system.
//!
//! The five-dimension run produced models where every value came from the model's training
//! knowledge. Nothing in the tool would have noticed: `spec` was accepted with no source at
//! all, so a number remembered from training and a number read out of a datasheet looked
//! identical in the ledger.

use sysml_ledger::{audit, Decision, Ledger, Origin, Source};

fn ledger_with(decisions: Vec<Decision>, sources: Vec<Source>) -> Ledger {
    Ledger {
        model: "m.sysml".to_string(),
        questions: Vec::new(),
        decisions,
        judgements: Vec::new(),
        sources,
        market: String::new(),
        jurisdictions: String::new(),
    }
}

fn spec(id: &str, cites: Vec<&str>) -> Decision {
    Decision {
        id: id.to_string(),
        element: "P::S::x".to_string(),
        value: "1".to_string(),
        origin: Origin::Spec,
        why: "from the datasheet".to_string(),
        falsifier: String::new(),
        inputs: Vec::new(),
        question: None,
        decided_by: "agent".to_string(),
        approved_by: None,
        cites: cites.into_iter().map(str::to_string).collect(),
    }
}

fn source(id: &str, quote: &str) -> Source {
    Source {
        id: id.to_string(),
        kind: "datasheet".to_string(),
        title: "a datasheet".to_string(),
        locator: "https://example.invalid".to_string(),
        quote: quote.to_string(),
        accessed: "2026-10-25".to_string(),
        jurisdiction: String::new(),
        added_by: "agent".to_string(),
    }
}

const MODEL: &str = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute x : Real = 1; }
}
"#;

fn run(ledger: &Ledger) -> sysml_ledger::Audit {
    let loaded = sysml_syntax::load_project(&[MODEL], true).expect("parses");
    audit(&loaded.roots, ledger)
}

#[test]
fn a_spec_value_with_no_citation_is_an_error() {
    // "I know this" and "I read this" have to be distinguishable, or the ledger is decorative.
    let ledger = ledger_with(vec![spec("D001", vec![])], vec![]);
    let a = run(&ledger);
    assert!(
        a.issues.iter().any(|i| i.code == "PROV012"),
        "got {:?}",
        a.issues.iter().map(|i| i.code).collect::<Vec<_>>()
    );
    assert!(
        a.error_count() > 0,
        "it must fail the gate, not merely warn"
    );
}

#[test]
fn a_spec_value_citing_a_registered_source_passes() {
    let ledger = ledger_with(
        vec![spec("D001", vec!["S001"])],
        vec![source("S001", "x = 1")],
    );
    let a = run(&ledger);
    assert!(
        a.issues
            .iter()
            .all(|i| i.code != "PROV012" && i.code != "PROV013"),
        "got {:?}",
        a.issues.iter().map(|i| i.code).collect::<Vec<_>>()
    );
}

#[test]
fn citing_a_source_that_does_not_exist_is_an_error() {
    let ledger = ledger_with(vec![spec("D001", vec!["S404"])], vec![]);
    let a = run(&ledger);
    assert!(
        a.issues.iter().any(|i| i.code == "PROV013"),
        "got {:?}",
        a.issues.len()
    );
}

#[test]
fn a_source_with_no_quote_backs_nothing() {
    // A URL alone does not let a reader check whether the number was read correctly, which is
    // the only question a citation is there to answer.
    let ledger = ledger_with(
        vec![spec("D001", vec!["S001"])],
        vec![source("S001", "   ")],
    );
    let a = run(&ledger);
    assert!(
        a.issues.iter().any(|i| i.code == "PROV014"),
        "an empty quote must not count as a source"
    );
}

#[test]
fn an_assumption_needs_no_citation() {
    // It asserts nothing about the world, so there is nothing outside the model to cite. The
    // falsifier is what it owes instead.
    let mut d = spec("D001", vec![]);
    d.origin = Origin::Assumed;
    d.falsifier = "if the measured value differs".to_string();
    let ledger = ledger_with(vec![d], vec![]);
    let a = run(&ledger);
    assert!(
        a.issues.iter().all(|i| i.code != "PROV012"),
        "an assumption is not a spec claim"
    );
}

#[test]
fn sources_are_numbered_without_reuse() {
    let mut ledger = Ledger::default();
    ledger.sources.push(source("S003", "q"));
    assert_eq!(ledger.next_source_id(), "S004");
}
