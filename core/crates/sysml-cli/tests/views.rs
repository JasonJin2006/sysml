//! What the model itself declares as a picture.
//!
//! A `view` usage names a subset of the model, and the model is the authority on that subset — so
//! `render --expose` draws what the model asked for rather than what a flag guessed. `views` is
//! what makes those keys discoverable: a key is a qualified name, and nobody guesses those.

use std::process::Command;

fn sysml() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sysml"))
}

const MODEL: &str = r#"package P {
    private import ScalarValues::*;

    part def Battery { attribute energy : Real = 100; }
    part def Motor { attribute power : Real = 50; }
    part def Chassis;
    part def Drone {
        part battery : Battery;
        part motor : Motor;
        part frame : Chassis;
    }

    requirement def Endurance { require constraint { 1 < 2 } }

    view powerView : GeneralView {
        expose Battery;
        expose Motor;
        expose Drone;
    }

    view structureView : GeneralView {
        expose Drone;
        expose Chassis;
    }
}
"#;

/// A directory for one test, under the workspace's own `target/tmp`.
///
/// Not the system temporary directory: this machine refuses `create_dir_all` beneath `%TEMP%`
/// (an access error even for the owner), and every test that writes a file failed for it. The
/// path is derived from the crate's manifest so a checkout anywhere works.
fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("tmp")
        .join(format!("sysml-views-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// The model in a directory of its own, so two tests cannot share a file.
fn model(tag: &str) -> std::path::PathBuf {
    let dir = scratch(tag);
    let path = dir.join("m.sysml");
    std::fs::write(&path, MODEL).expect("write model");
    path
}

fn run(path: &std::path::Path, args: &[&str]) -> (String, i32) {
    let output = sysml()
        .arg(args[0])
        .arg(path)
        .args(&args[1..])
        .output()
        .expect("the binary runs");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (text, output.status.code().unwrap_or(-1))
}

/// The labels a rendered picture mentions, which is what "exposed" means in practice.
fn labels(svg: &str) -> Vec<String> {
    // Every label is a `<text ...>name</text>` run; the drawing has no other bare text.
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find('>') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('<') else { break };
        let word = after[..end].trim();
        if !word.is_empty()
            && word
                .chars()
                .all(|c| c.is_alphanumeric() || c == ' ' || c == '_')
        {
            out.push(word.to_string());
        }
        rest = &after[end..];
    }
    out
}

#[test]
fn views_lists_the_keys_the_model_declares() {
    let path = model("list");
    let (text, code) = run(&path, &["views"]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("P::powerView"), "{text}");
    assert!(text.contains("P::structureView"), "{text}");
    assert!(
        text.contains("--view gv"),
        "each row says how to draw it: {text}"
    );
}

#[test]
fn views_answers_json_when_asked() {
    let path = model("json");
    let (text, code) = run(&path, &["views", "--json"]);
    assert_eq!(code, 0, "{text}");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("json");
    let items = parsed.as_array().expect("an array");
    assert_eq!(items.len(), 2, "{text}");
    assert!(items.iter().all(|v| v["projection"] == "gv"));
}

#[test]
fn a_model_with_no_view_says_so_rather_than_printing_nothing() {
    let dir = scratch("plain");
    let path = dir.join("m.sysml");
    std::fs::write(&path, "package P { part def A; }\n").expect("write");
    let (text, code) = run(&path, &["views"]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("declares no view"), "{text}");
    // And it says how to see the model anyway, because "no view" is not "nothing to draw".
    assert!(text.contains("--view gv"), "{text}");
}

#[test]
fn expose_draws_only_what_the_view_names() {
    let path = model("power");
    let (svg, code) = run(&path, &["render", "--expose", "P::powerView"]);
    assert_eq!(code, 0, "{svg}");
    let mentioned = labels(&svg);
    assert!(mentioned.iter().any(|l| l == "Battery"), "{mentioned:?}");
    assert!(mentioned.iter().any(|l| l == "Motor"), "{mentioned:?}");
    // `Chassis` is not exposed by this view, so it must not be drawn.
    assert!(
        !mentioned.iter().any(|l| l == "Chassis"),
        "an unexposed element leaked into the picture: {mentioned:?}"
    );
}

#[test]
fn two_views_of_one_model_draw_different_pictures() {
    let path = model("two");
    let (power, _) = run(&path, &["render", "--expose", "P::powerView"]);
    let (structure, _) = run(&path, &["render", "--expose", "P::structureView"]);
    assert_ne!(power, structure, "one model, two views, two pictures");
    let power_labels = labels(&power);
    let structure_labels = labels(&structure);
    assert!(power_labels.iter().any(|l| l == "Motor"));
    assert!(!structure_labels.iter().any(|l| l == "Motor"));
    assert!(structure_labels.iter().any(|l| l == "Chassis"));
}

#[test]
fn an_unknown_view_is_a_usage_error_that_names_the_fix() {
    let path = model("bad");
    let (text, code) = run(&path, &["render", "--expose", "P::nope"]);
    assert_eq!(code, 2, "a wrong key is the call being wrong: {text}");
    assert!(text.contains("no view named"), "{text}");
    assert!(
        text.contains("sysml views"),
        "it says how to find one: {text}"
    );
}
