//! `sysml format`: re-indent a file, or say whether it needs it.
//!
//! The command has two jobs and one rule: it never loses anything the author wrote. A formatter
//! that dropped a comment would be caught here rather than in somebody's diff.

use std::process::Command;

fn sysml() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sysml"))
}

/// A directory for one test, under the workspace's own `target/tmp`.
fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("tmp")
        .join(format!("sysml-format-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
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

#[test]
fn a_misindented_file_is_rewritten() {
    let dir = scratch("rewrite");
    let path = dir.join("m.sysml");
    std::fs::write(
        &path,
        "package P {\npart def A {\nattribute x : Real;\n}\n}\n",
    )
    .expect("write");
    let (text, code) = run(&path, &["format"]);
    assert_eq!(code, 0, "{text}");
    let after = std::fs::read_to_string(&path).expect("read");
    assert_eq!(
        after,
        "package P {\n    part def A {\n        attribute x : Real;\n    }\n}\n"
    );
}

#[test]
fn check_reports_without_writing_and_fails() {
    // A hook that used this and passed on an unformatted file would be a hook that does nothing.
    let dir = scratch("check");
    let path = dir.join("m.sysml");
    let before = "package P {\npart def A;\n}\n";
    std::fs::write(&path, before).expect("write");
    let (text, code) = run(&path, &["format", "--check"]);
    assert_eq!(
        code, 4,
        "a file that needs formatting is not a model defect: {text}"
    );
    assert!(text.contains("not formatted"), "{text}");
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        before,
        "nothing was written"
    );
}

#[test]
fn check_passes_on_a_formatted_file() {
    let dir = scratch("clean");
    let path = dir.join("m.sysml");
    let formatted = "package P {\n    part def A;\n}\n";
    std::fs::write(&path, formatted).expect("write");
    let (text, code) = run(&path, &["format", "--check"]);
    assert_eq!(code, 0, "{text}");
    assert!(!text.contains("not formatted"), "{text}");
}

#[test]
fn a_comment_is_not_lost() {
    let dir = scratch("comment");
    let path = dir.join("m.sysml");
    std::fs::write(&path, "package P {\n// why this exists\npart def A;\n}\n").expect("write");
    let (_, code) = run(&path, &["format"]);
    assert_eq!(code, 0);
    let after = std::fs::read_to_string(&path).expect("read");
    assert!(after.contains("// why this exists"), "{after}");
    assert!(after.contains("    // why this exists"), "{after}");
}

#[test]
fn formatting_twice_changes_nothing_the_second_time() {
    let dir = scratch("idempotent");
    let path = dir.join("m.sysml");
    std::fs::write(&path, "package P {\npart def A {\nattribute x;\n}\n}\n").expect("write");
    run(&path, &["format"]);
    let once = std::fs::read_to_string(&path).expect("read");
    let (text, code) = run(&path, &["format", "--check"]);
    assert_eq!(code, 0, "the second run has nothing to do: {text}");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), once);
}

#[test]
fn a_missing_file_is_a_usage_error() {
    let dir = scratch("missing");
    let path = dir.join("nope.sysml");
    let (text, code) = run(&path, &["format"]);
    assert_eq!(code, 2, "{text}");
}
