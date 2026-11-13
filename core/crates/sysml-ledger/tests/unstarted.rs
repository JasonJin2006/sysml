//! Telling "the ledger is empty" apart from "the ledger disagrees with the model".
//!
//! The shipped example failed its own gate on first run: its semantics were clean and its
//! ledger had never been started, but `check` answered 1 — the same code it uses for a violated
//! rule. A caller reading only the exit code cannot tell which of the two it is looking at, and
//! the two want opposite actions: fix the model, or write the first ledger entry.

use sysml_ledger::{audit, Decision, Ledger, Origin};

fn decision(id: &str, element: &str, origin: Origin) -> Decision {
    Decision {
        id: id.to_string(),
        element: element.to_string(),
        value: String::new(),
        origin,
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
    part def S { attribute x : Real = 1; }
}
"#;

fn run(ledger: &Ledger) -> sysml_ledger::Audit {
    let loaded = sysml_syntax::load_project(&[MODEL], true).expect("parses");
    audit(&loaded.roots, ledger)
}

#[test]
fn an_empty_ledger_has_not_been_started() {
    let emptied = run(&Ledger::default());
    assert!(emptied.error_count() > 0, "the gate still fails");
    assert!(
        emptied.not_started(),
        "an empty ledger is unfinished, not wrong: {:?}",
        emptied.issues.iter().map(|i| i.code).collect::<Vec<_>>()
    );
}

#[test]
fn a_ledger_that_disagrees_has_been_started() {
    // One entry for the value, and nothing for the part definition. The remaining errors are
    // still "no record", but the ledger is no longer empty — reading it before extending it is
    // the only way to find out whether the entry that exists is the right one.
    let mut ledger = Ledger::default();
    ledger
        .decisions
        .push(decision("D001", "P::S::x", Origin::Assumed));
    let partial = run(&ledger);
    assert!(
        !partial.not_started(),
        "one recorded value means the ledger has begun"
    );
}

#[test]
fn a_contradiction_is_never_unfinished() {
    // The ledger says one number, the model says another. This is the case `not_started` must
    // never swallow: the caller would be told to start a ledger that already exists and is
    // wrong. A single non-"missing" code is enough to disqualify.
    let mut ledger = Ledger::default();
    let mut row = decision("D001", "P::S::x", Origin::Assumed);
    row.value = "999".to_string();
    ledger.decisions.push(row);
    let contradicted = run(&ledger);
    let codes: Vec<&str> = contradicted.issues.iter().map(|i| i.code).collect();
    assert!(
        codes.contains(&"PROV016"),
        "this test is about a contradiction, so it must produce one: {codes:?}"
    );
    assert!(
        !contradicted.not_started(),
        "a ledger that contradicts the model has been started and must be read, not restarted"
    );
}

const JUDGED: &str = r#"
package P {
    private import ScalarValues::*;
    part def D { attribute e : Real = 5; }
    requirement R {
        subject d : D;
        require constraint { d.e >= 3.0 }
    }
}
"#;

fn roots_of(src: &str) -> Vec<sysml_model::Element> {
    sysml_syntax::load_project(&[src], true)
        .expect("parses")
        .roots
}

#[test]
fn the_fingerprint_moves_when_a_number_moves() {
    // A verdict depends on the values, not only on the shape. A fingerprint over kinds and names
    // said the model had not changed when `= 5` became `= 1`, which is the one fact deciding
    // whether `d.e >= 3.0` holds.
    let before = roots_of(JUDGED);
    let after = roots_of(&JUDGED.replace("= 5;", "= 1;"));
    assert_ne!(
        sysml_ledger::fingerprint(&before),
        sysml_ledger::fingerprint(&after)
    );
}

#[test]
fn the_fingerprint_does_not_move_for_an_irrelevant_edit() {
    // Whitespace is not a fact. A comment added to the file must not mark every judgement stale,
    // or the staleness stops being a signal and becomes a nuisance that gets ignored.
    let before = roots_of(JUDGED);
    let after = roots_of(&format!(
        "{JUDGED}
// a note to self
"
    ));
    assert_eq!(
        sysml_ledger::fingerprint(&before),
        sysml_ledger::fingerprint(&after)
    );
}

#[test]
fn a_judgement_against_an_older_model_is_stale() {
    let before = roots_of(JUDGED);
    let after = roots_of(&JUDGED.replace("= 5;", "= 1;"));
    let mut ledger = Ledger::default();
    ledger.judgements.push(sysml_ledger::Judgement {
        id: "J001".to_string(),
        requirement: "P::R".to_string(),
        verdict: "pass".to_string(),
        detail: String::new(),
        fingerprint: sysml_ledger::fingerprint(&before),
        judged_by: "agent".to_string(),
        at: String::new(),
    });
    let now = sysml_ledger::fingerprint(&after);
    let stale = sysml_ledger::stale(&ledger, &now);
    assert_eq!(stale.len(), 1, "the number moved under the verdict");
    assert_eq!(
        stale[0].verdict, "pass",
        "the old outcome is reported as it was"
    );
    assert_eq!(stale[0].current, now);
}

#[test]
fn a_judgement_against_the_current_model_is_not_stale() {
    let roots = roots_of(JUDGED);
    let now = sysml_ledger::fingerprint(&roots);
    let mut ledger = Ledger::default();
    ledger.judgements.push(sysml_ledger::Judgement {
        id: "J001".to_string(),
        requirement: "P::R".to_string(),
        verdict: "pass".to_string(),
        detail: String::new(),
        fingerprint: now.clone(),
        judged_by: "agent".to_string(),
        at: String::new(),
    });
    assert!(sysml_ledger::stale(&ledger, &now).is_empty());
}

#[test]
fn a_stale_judgement_reaches_the_gate_as_a_warning() {
    // The failure this catches: a model edited after a judgement reported
    // `gate 0 error(s), 0 warning(s)`. The verdict happened to still hold, so nothing else
    // noticed, and the ledger kept a passing grade from a model that no longer exists.
    let roots = roots_of(JUDGED);
    let mut ledger = Ledger::default();
    ledger
        .decisions
        .push(decision("D001", "P::D::e", Origin::Assumed));
    ledger
        .decisions
        .push(decision("D002", "P::D", Origin::Assumed));
    ledger.market = "EU".to_string();
    ledger.jurisdictions = "EU".to_string();
    ledger.judgements.push(sysml_ledger::Judgement {
        id: "J001".to_string(),
        requirement: "P::R".to_string(),
        verdict: "pass".to_string(),
        detail: String::new(),
        fingerprint: "0000000000000000".to_string(),
        judged_by: "agent".to_string(),
        at: String::new(),
    });
    let audit = sysml_ledger::audit(&roots, &ledger);
    assert!(
        audit.issues.iter().any(|i| i.code == "PROV020"),
        "got {:?}",
        audit.issues.iter().map(|i| i.code).collect::<Vec<_>>()
    );
    assert_eq!(
        audit
            .issues
            .iter()
            .filter(|i| i.code == "PROV020")
            .map(|i| i.severity)
            .collect::<Vec<_>>(),
        [sysml_ledger::Severity::Warning],
        "a stale judgement asks for a re-run; it does not claim the model is wrong"
    );
}

#[test]
fn re_judging_clears_the_stale_warning() {
    // The warning has to be clearable by doing what it asks. If the old record keeps raising it
    // after the requirement has been judged again, people learn to ignore the code — and then it
    // stops catching the case it exists for.
    let roots = roots_of(JUDGED);
    let current = sysml_ledger::fingerprint(&roots);
    let mut ledger = Ledger::default();
    for (id, against) in [("J001", "0000000000000000"), ("J002", current.as_str())] {
        ledger.judgements.push(sysml_ledger::Judgement {
            id: id.to_string(),
            requirement: "P::R".to_string(),
            verdict: "pass".to_string(),
            detail: String::new(),
            fingerprint: against.to_string(),
            judged_by: "agent".to_string(),
            at: String::new(),
        });
    }
    assert!(
        sysml_ledger::stale(&ledger, &current).is_empty(),
        "a later judgement of the same requirement against this model supersedes the earlier one"
    );
}
