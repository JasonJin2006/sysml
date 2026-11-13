//! One test per provenance rule: what each code rejects, and what it accepts.
//!
//! The tests in `research.rs` prove the citation gate works. They do not name the codes, so a
//! rule that stopped firing — or fired for the wrong reason — could still pass them. Each test
//! here asserts on the code itself.

use sysml_ledger::{audit, Decision, Ledger, Origin, Question, QuestionState, Severity, Source};
use sysml_model::{Element, Kind};

/// The model side of the ledger: one attribute with a value, so a decision has something to
/// account for. `audit` only judges rows whose element exists in the tree.
fn model() -> Vec<Element> {
    let mut package = Element::new(Kind::Package, "P", 1);
    let mut owner = Element::new(Kind::PartDef, "S", 2);
    let mut attribute = Element::new(Kind::AttributeUsage, "x", 3);
    attribute.value_text = Some("1".to_string());
    owner.children.push(attribute);
    package.children.push(owner);
    vec![package]
}

fn ledger(decisions: Vec<Decision>) -> Ledger {
    Ledger {
        model: "m.sysml".to_string(),
        questions: Vec::new(),
        decisions,
        judgements: Vec::new(),
        sources: vec![Source {
            id: "S001".to_string(),
            kind: "datasheet".to_string(),
            title: "datasheet".to_string(),
            locator: String::new(),
            quote: "mass 4 kg".to_string(),
            accessed: String::new(),
            jurisdiction: String::new(),
            added_by: "agent".to_string(),
        }],
        market: "EU".to_string(),
        jurisdictions: "EU".to_string(),
    }
}

fn decision(id: &str, origin: Origin) -> Decision {
    Decision {
        id: id.to_string(),
        element: "P::S::x".to_string(),
        value: "1".to_string(),
        origin,
        why: "because".to_string(),
        falsifier: String::new(),
        inputs: Vec::new(),
        question: None,
        decided_by: "agent".to_string(),
        approved_by: None,
        cites: Vec::new(),
    }
}

fn codes(l: &Ledger) -> Vec<&'static str> {
    audit(&model(), l).issues.iter().map(|i| i.code).collect()
}

fn issue<'a>(l: &'a Ledger, code: &str) -> Option<sysml_ledger::LedgerIssue> {
    audit(&model(), l)
        .issues
        .into_iter()
        .find(|i| i.code == code)
}

#[test]
fn a_spec_value_without_a_reason_is_flagged() {
    let mut d = decision("D001", Origin::Spec);
    d.why = String::new();
    d.cites = vec!["S001".to_string()];
    let found = codes(&ledger(vec![d]));
    assert!(found.contains(&"PROV002"), "got {found:?}");
}

#[test]
fn an_assumption_without_a_falsifier_is_an_error() {
    let mut d = decision("D002", Origin::Assumed);
    d.approved_by = Some("human:jason".to_string());
    let found = issue(&ledger(vec![d]), "PROV005").expect("PROV005 fires");
    assert_eq!(found.severity, Severity::Error);
}

#[test]
fn an_assumption_nobody_approved_is_an_error() {
    let mut d = decision("D003", Origin::Assumed);
    d.falsifier = "if the datasheet says otherwise".to_string();
    let found = issue(&ledger(vec![d]), "PROV006").expect("PROV006 fires");
    assert_eq!(found.severity, Severity::Error);
}

#[test]
fn a_derivation_from_an_assumption_is_flagged() {
    // A value presented as computed, whose input is a guess: the reader sees a derivation and
    // never learns that the derivation rests on an assumption. Both rows have to be on the same
    // element the model carries, or `PROV001` fires first and masks this rule.
    let mut assumption = decision("D004", Origin::Assumed);
    assumption.falsifier = "if measured otherwise".to_string();
    assumption.approved_by = Some("human:jason".to_string());
    let mut derived = decision("D005", Origin::Derived);
    derived.inputs = vec!["P::S::x".to_string()];
    let found = codes(&ledger(vec![assumption, derived]));
    assert!(found.contains(&"PROV015"), "got {found:?}");
}

#[test]
fn a_derivation_with_no_inputs_is_flagged() {
    let mut d = decision("D006", Origin::Derived);
    d.inputs = Vec::new();
    let found = codes(&ledger(vec![d]));
    assert!(found.contains(&"PROV003"), "got {found:?}");
}

#[test]
fn claiming_a_person_answered_an_open_question_is_flagged() {
    let mut d = decision("D007", Origin::Asked);
    d.question = Some("Q001".to_string());
    let mut l = ledger(vec![d]);
    l.questions = vec![Question {
        id: "Q001".to_string(),
        at: "P::S".to_string(),
        text: "what is it".to_string(),
        blocks: Vec::new(),
        state: QuestionState::Open,
        answer: None,
        answered_by: None,
        asked_by: "agent".to_string(),
        raised_at: None,
    }];
    let found = codes(&l);
    assert!(found.contains(&"PROV009"), "got {found:?}");
}

#[test]
fn a_jurisdiction_with_no_search_scope_is_flagged() {
    let mut l = ledger(vec![decision("D008", Origin::Spec)]);
    l.jurisdictions = String::new();
    let found = codes(&l);
    assert!(
        found.contains(&"PROV018") || found.contains(&"PROV019"),
        "got {found:?}"
    );
}

/// A model with a part definition and a requirement, so the element-scoped rules have something
/// to walk.
fn model_with_requirement() -> Vec<Element> {
    let mut package = Element::new(Kind::Package, "P", 1);
    package.children.push(Element::new(Kind::PartDef, "S", 2));
    package
        .children
        .push(Element::new(Kind::RequirementDef, "R", 3));
    let mut attribute = Element::new(Kind::AttributeUsage, "x", 4);
    attribute.value_text = Some("1".to_string());
    package.children[0].children.push(attribute);
    vec![package]
}

#[test]
fn a_model_value_with_no_ledger_entry_is_an_error() {
    let found = codes(&ledger(Vec::new()));
    assert!(found.contains(&"PROV001"), "got {found:?}");
}

#[test]
fn a_derivation_from_something_neither_modelled_nor_recorded_is_an_error() {
    let mut d = decision("D010", Origin::Derived);
    d.inputs = vec!["P::S::nowhere".to_string()];
    let found = codes(&ledger(vec![d]));
    assert!(found.contains(&"PROV004"), "got {found:?}");
}

#[test]
fn an_answered_claim_with_no_question_is_an_error() {
    let mut d = decision("D011", Origin::Asked);
    d.question = None;
    let found = codes(&ledger(vec![d]));
    assert!(found.contains(&"PROV007"), "got {found:?}");
}

#[test]
fn an_answered_claim_pointing_at_no_question_is_an_error() {
    let mut d = decision("D012", Origin::Asked);
    d.question = Some("Q999".to_string());
    let found = codes(&ledger(vec![d]));
    assert!(found.contains(&"PROV008"), "got {found:?}");
}

#[test]
fn a_requirement_blocked_by_an_open_question_is_a_warning() {
    let mut l = ledger(vec![decision("D013", Origin::Spec)]);
    l.questions = vec![Question {
        id: "Q001".to_string(),
        at: "P::R".to_string(),
        text: "which market".to_string(),
        blocks: vec!["P::R".to_string()],
        state: QuestionState::Open,
        answer: None,
        answered_by: None,
        asked_by: "agent".to_string(),
        raised_at: None,
    }];
    let found = audit(&model_with_requirement(), &l)
        .issues
        .into_iter()
        .find(|i| i.code == "PROV010")
        .expect("PROV010 fires");
    assert_eq!(found.severity, Severity::Warning);
}

#[test]
fn two_questions_with_one_id_are_an_error() {
    let mut l = ledger(Vec::new());
    let q = Question {
        id: "Q001".to_string(),
        at: "P::S".to_string(),
        text: "what".to_string(),
        blocks: Vec::new(),
        state: QuestionState::Open,
        answer: None,
        answered_by: None,
        asked_by: "agent".to_string(),
        raised_at: None,
    };
    l.questions = vec![q.clone(), q];
    let found = codes(&l);
    assert!(found.contains(&"PROV011"), "got {found:?}");
}

#[test]
fn a_part_definition_with_no_decision_of_its_own_is_an_error() {
    let mut l = ledger(vec![decision("D014", Origin::Spec)]);
    l.decisions[0].element = "P::S::x".to_string();
    let found = codes(&l);
    assert!(found.contains(&"PROV017"), "got {found:?}");
}

#[test]
fn a_decision_about_an_element_the_model_lost_is_an_error() {
    // The rename that started this rule: three decisions kept naming `ipRating` after it became
    // `dockIpRating`, and nothing noticed.
    let mut d = decision("D015", Origin::Spec);
    d.element = "P::S::gone".to_string();
    let found = codes(&ledger(vec![d]));
    assert!(found.contains(&"PROV021"), "got {found:?}");
}

#[test]
fn a_spec_value_that_cites_nothing_is_an_error() {
    let d = decision("D016", Origin::Spec);
    let found = codes(&ledger(vec![d]));
    assert!(found.contains(&"PROV012"), "got {found:?}");
}

#[test]
fn citing_a_source_that_is_not_registered_is_an_error() {
    let mut d = decision("D017", Origin::Spec);
    d.cites = vec!["S999".to_string()];
    let found = codes(&ledger(vec![d]));
    assert!(found.contains(&"PROV013"), "got {found:?}");
}

#[test]
fn citing_a_source_with_no_quote_is_an_error() {
    let mut d = decision("D018", Origin::Spec);
    d.cites = vec!["S001".to_string()];
    let mut l = ledger(vec![d]);
    l.sources[0].quote = String::new();
    let found = codes(&l);
    assert!(found.contains(&"PROV014"), "got {found:?}");
}

#[test]
fn a_ledger_value_that_disagrees_with_the_model_is_an_error() {
    let mut d = decision("D019", Origin::Spec);
    d.cites = vec!["S001".to_string()];
    d.value = "2".to_string();
    let found = codes(&ledger(vec![d]));
    assert!(found.contains(&"PROV016"), "got {found:?}");
}

#[test]
fn a_judgement_against_an_older_model_is_a_warning() {
    let mut l = ledger(Vec::new());
    l.judgements = vec![sysml_ledger::Judgement {
        id: "J001".to_string(),
        requirement: "P::R".to_string(),
        verdict: "pass".to_string(),
        detail: String::new(),
        fingerprint: "an-older-model".to_string(),
        judged_by: "agent".to_string(),
        at: String::new(),
    }];
    let found = audit(&model_with_requirement(), &l)
        .issues
        .into_iter()
        .find(|i| i.code == "PROV020")
        .expect("PROV020 fires");
    assert_eq!(found.severity, Severity::Warning);
}
