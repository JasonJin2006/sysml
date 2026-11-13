//! The ladder as the canvas asks for it.
//!
//! The canvas and `sysml next` answer one question — which stage the model is in — so they call one
//! function. These tests pin the shape the canvas reads, because a field renamed on the server side
//! would not fail anything else: the strip would simply stop appearing, which is the kind of break
//! nobody reports.

use sysml_studio::document::Session;
use sysml_studio::next::Stage;
use sysml_studio::translate::Lang;
use sysml_view::{Mode, View};

fn session(tag: &str, text: &str) -> Session {
    let dir = std::env::temp_dir().join(format!("sysml-ladder-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.sysml");
    std::fs::write(&path, text).unwrap();
    // A stale ledger from an earlier run would change the answer; each test starts without one.
    let _ = std::fs::remove_file(sysml_ledger::Ledger::path_for(&path));
    Session::open(vec![path], View::General, Mode::Extended).expect("opens")
}

const NO_REQUIREMENT: &str = r#"package P {
    private import ScalarValues::*;
    part def Thing { attribute x : Real = 1; }
}
"#;

const WITH_REQUIREMENT: &str = r#"package P {
    private import ScalarValues::*;
    part def Thing { attribute x : Real = 1; }
    requirement R1 {
        subject t : Thing;
        require constraint { t.x >= 0.5 }
    }
}
"#;

#[test]
fn the_canvas_reads_the_same_fields_the_command_line_writes() {
    let plan = session("shape", WITH_REQUIREMENT)
        .next(Lang::En)
        .expect("plans");
    for key in [
        "stage",
        "number",
        "label",
        "established",
        "waiting",
        "command",
        "counts",
    ] {
        assert!(plan.get(key).is_some(), "the canvas reads `{key}`: {plan}");
    }
    // `number` is what the strip prints as `4/6`; parsing it back out of the Chinese label would
    // break the moment the label is reworded.
    assert_eq!(plan["number"], 4, "{plan}");
    assert_eq!(plan["stage"], "values", "{plan}");
}

#[test]
fn a_model_with_no_requirement_is_at_the_first_stage() {
    let plan = session("empty", NO_REQUIREMENT)
        .next(Lang::En)
        .expect("plans");
    assert_eq!(plan["stage"], "requirements", "{plan}");
    assert_eq!(plan["number"], 1, "{plan}");
    let waiting = plan["waiting"].as_array().expect("a list");
    assert!(
        !waiting.is_empty(),
        "a stage that holds the model must say why: {plan}"
    );
}

#[test]
fn every_stage_has_a_number_and_a_key() {
    // The strip prints `n/6` and looks up the key in its own dictionary, so both have to exist for
    // every stage — including the one past the last gate.
    let stages = [
        (Stage::Requirements, 1, "requirements"),
        (Stage::Questions, 2, "questions"),
        (Stage::Allocation, 3, "allocation"),
        (Stage::Values, 4, "values"),
        (Stage::Interfaces, 5, "interfaces"),
        (Stage::Verification, 6, "verification"),
        (Stage::Ready, 7, "ready"),
    ];
    for (stage, number, key) in stages {
        assert_eq!(stage.number(), number, "{key}");
        assert_eq!(stage.key(), key);
    }
}

#[test]
fn the_targets_are_names_the_canvas_can_key_on() {
    // A chip in the stage strip jumps to a box, and the canvas keys a box by its qualified name.
    // A bare name here would draw a chip that goes nowhere, which is worse than no chip.
    let plan = session("targets", WITH_REQUIREMENT)
        .next(Lang::En)
        .expect("plans");
    let targets = plan["targets"].as_array().expect("a list");
    assert!(!targets.is_empty(), "stage 4 is held by a value: {plan}");
    for target in targets {
        let name = target.as_str().expect("a string");
        assert!(
            name.contains("::"),
            "`{name}` is not a qualified name: {plan}"
        );
        assert_eq!(name, "P::Thing::x", "{plan}");
    }
}

#[test]
fn the_inspector_gets_every_blocking_element_not_just_the_first_five() {
    // The strip shows five and the inspector asks about one. A map capped at the strip's five would
    // answer "not in the way" for the twenty-sixth unaccounted value — a confident wrong answer.
    let plan = session("blocking", WITH_REQUIREMENT)
        .next(Lang::En)
        .expect("plans");
    let blocking = plan["blocking"].as_object().expect("a map");
    assert_eq!(
        blocking.get("P::Thing::x").and_then(|v| v.as_str()),
        Some("unaccounted"),
        "{plan}"
    );
    // Every chip in the strip has to be in the map, or clicking it would show an element the
    // inspector calls innocent.
    for target in plan["targets"].as_array().expect("a list") {
        let key = target.as_str().expect("a string");
        assert!(
            blocking.contains_key(key),
            "`{key}` is in the strip but not in the map: {plan}"
        );
    }
}

#[test]
fn the_same_model_answers_the_same_way_twice() {
    // The canvas refetches this on every real load. A plan that changed between two identical calls
    // would make the strip flicker between stages with nothing edited.
    let s = session("stable", WITH_REQUIREMENT);
    let first = s.next(Lang::En).expect("plans");
    let second = s.next(Lang::En).expect("plans");
    assert_eq!(first, second);
}
