//! The commands that write the ledger beside the model, held to their refusals.
//!
//! `check` and `status` only read; a bug in them wastes a run. `decide`, `approve`, `retract` and
//! the rest write a file a person will later read as their own decisions, so the cases that
//! matter are the ones where they must refuse.

use std::path::PathBuf;
use std::process::Command;

const MODEL: &str = r#"package P {
    private import ScalarValues::*;
    part def Battery { attribute mass : Real = 2.1; }
    part def Drone { part battery : Battery; attribute power : Real = 900; }
    requirement power {
        subject d : Drone;
        require constraint { d.power >= 1 }
    }
}
"#;

struct Model {
    dir: PathBuf,
    path: PathBuf,
}

impl Model {
    fn new(tag: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "sysml-writes-{tag}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.sysml");
        std::fs::write(&path, MODEL).expect("write model");
        Model { dir, path }
    }

    fn run(&self, args: &[&str]) -> (String, i32) {
        let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
            .arg(args[0])
            .arg(&self.path)
            .args(&args[1..])
            .output()
            .expect("the binary runs");
        // Both streams: a refusal is written to stderr, and a test that read only stdout would
        // find an empty string and conclude the command said nothing.
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        (text, output.status.code().unwrap_or(-1))
    }

    fn ledger(&self) -> String {
        let path = self.dir.join("m.ledger.json");
        std::fs::read_to_string(path).unwrap_or_default()
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_decision_records_itself_in_the_ledger() {
    let model = Model::new("decide");
    let (_out, code) = model.run(&[
        "decide",
        "--element",
        "P::Drone::power",
        "--value",
        "900",
        "--origin",
        "assumed",
        "--falsifier",
        "if the datasheet says otherwise",
        "--why",
        "datasheet",
    ]);
    assert_eq!(code, 0, "the write succeeds");
    let ledger = model.ledger();
    assert!(ledger.contains("P::Drone::power"), "got {ledger}");
    assert!(ledger.contains("datasheet"), "got {ledger}");
}

#[test]
fn an_unknown_origin_is_a_usage_error() {
    let model = Model::new("origin");
    let (_out, code) = model.run(&[
        "decide",
        "--element",
        "P::Drone::power",
        "--value",
        "1",
        "--origin",
        "invented",
    ]);
    assert_eq!(code, 2, "a bad flag value is a usage error");
}

#[test]
fn an_assumption_with_no_falsifier_is_refused() {
    // The rule the canvas has too, and the one thing an assumption must carry: what would show it
    // wrong. A command line that accepted it would be the quietest way to write an unfalsifiable
    // guess.
    let model = Model::new("falsifier");
    let (out, code) = model.run(&[
        "decide",
        "--element",
        "P::Drone::power",
        "--value",
        "900",
        "--origin",
        "assumed",
        "--why",
        "looks right",
    ]);
    assert_ne!(code, 0, "{out}");
    assert!(
        model.ledger().is_empty() || !model.ledger().contains("P::Drone::power"),
        "nothing is written: {}",
        model.ledger()
    );
}

#[test]
fn an_approval_must_name_a_person() {
    let model = Model::new("approve");
    let (out, code) = model.run(&["approve", "--id", "D001", "--by", "agent"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.to_lowercase().contains("human"), "{out}");
}

#[test]
fn retracting_an_id_that_is_not_there_is_an_error() {
    let model = Model::new("retract");
    let (out, code) = model.run(&["retract", "--id", "D999"]);
    assert_ne!(code, 0, "{out}");
}

#[test]
fn a_decision_about_an_element_the_model_does_not_have_is_refused() {
    let model = Model::new("no-element");
    let (out, code) = model.run(&[
        "decide",
        "--element",
        "P::Drone::nowhere",
        "--value",
        "1",
        "--origin",
        "spec",
        "--why",
        "datasheet",
    ]);
    assert_ne!(code, 0, "{out}");
}

#[test]
fn diff_takes_the_library_option_the_other_commands_take() {
    // A project split across files is the normal case. `diff` loaded one file per side and
    // accepted no `--library`, so a reference to a sibling file resolved to nothing and a change
    // there never reached the requirement it touches.
    let model = Model::new("diff-library");
    let lib = model.dir.join("lib.sysml");
    std::fs::write(
        &lib,
        "package Lib {\n    part def Motor { attribute power : Real; }\n}\n",
    )
    .expect("write library");
    let after = model.dir.join("after.sysml");
    std::fs::write(
        &after,
        "package P {\n    private import Lib::*;\n    part def D { part m : Motor; }\n\
         requirement R { subject d : D; require constraint { d.m.power >= 2.0 } }\n}\n",
    )
    .expect("write after");
    let before = model.path.clone();
    std::fs::write(
        &before,
        "package P {\n    private import Lib::*;\n    part def D { part m : Motor; }\n\
         requirement R { subject d : D; require constraint { d.m.power >= 1.0 } }\n}\n",
    )
    .expect("write before");

    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .arg("diff")
        .arg(&before)
        .arg(&after)
        .args(["--library", lib.to_str().expect("path")])
        .output()
        .expect("runs");
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(text.contains("P::R"), "the requirement is named: {text}");
}
