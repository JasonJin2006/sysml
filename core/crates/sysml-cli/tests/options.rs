//! Command-line option spellings.
//!
//! `--key value` and `--key=value` are the same option to everyone who types them. The parser used to
//! treat the whole `--library=stdlib` token as the key, compare it against the accepted list, and
//! reject it as an option named literally `--library=stdlib` — while `--library stdlib` worked. A user
//! who wrote the `=` form was told the option did not exist, which is both wrong and misleading about
//! what the tool can do.
//!
//! These tests pin both spellings, and pin that an unknown option is still rejected: the fix must not
//! turn "reject unknown options" into "accept anything with an `=` in it".

use std::process::Command;

/// A model and a directory to use as a library, so `--library` has something real to point at.
fn fixture(tag: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "sysml-cli-opts-{tag}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("lib")).expect("temp dirs");
    let model = dir.join("m.sysml");
    std::fs::write(&model, "package P {\n    part def A;\n}\n").expect("write model");
    std::fs::write(
        dir.join("lib").join("l.sysml"),
        "package L {\n    part def B;\n}\n",
    )
    .expect("write library");
    let library = dir.join("lib");
    (dir, model, library)
}

fn run(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .args(args)
        .output()
        .expect("the binary runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn an_option_value_may_be_written_with_an_equals_sign() {
    let (dir, model, library) = fixture("equals");
    let library = library.to_string_lossy().into_owned();
    let model = model.to_string_lossy().into_owned();

    let with_equals = format!("--library={library}");
    let (code, stdout, stderr) = run(&["check", &model, &with_equals, "--json"]);
    assert_ne!(
        code, 2,
        "`--library=<path>` must be accepted, got exit {code}: {stderr}"
    );
    assert!(
        !stderr.contains("unknown option"),
        "`--library=<path>` was rejected as unknown: {stderr}"
    );
    assert!(
        stdout.trim_start().starts_with('{'),
        "expected a JSON document, got: {stdout}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn both_spellings_of_an_option_agree() {
    let (dir, model, library) = fixture("agree");
    let library = library.to_string_lossy().into_owned();
    let model = model.to_string_lossy().into_owned();

    let (code_a, out_a, _) = run(&["check", &model, "--library", &library, "--json"]);
    let with_equals = format!("--library={library}");
    let (code_b, out_b, _) = run(&["check", &model, &with_equals, "--json"]);

    assert_eq!(
        code_a, code_b,
        "the two spellings must give the same verdict"
    );
    assert_eq!(out_a, out_b, "the two spellings must give the same output");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_option_is_still_rejected_with_an_equals_sign() {
    let (dir, model, _library) = fixture("unknown");
    let model = model.to_string_lossy().into_owned();

    let (code, _stdout, stderr) = run(&["check", &model, "--zzzNotAnOption=1"]);
    assert_eq!(
        code, 2,
        "an unknown option must be a usage error, got {code}"
    );
    assert!(
        stderr.contains("--zzzNotAnOption"),
        "the error must name the option, got: {stderr}"
    );
    assert!(
        !stderr.contains("--zzzNotAnOption=1"),
        "the error must name the option, not the option plus its value: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_switch_written_with_an_equals_sign_is_still_a_switch() {
    // `--json` takes no value. Writing `--json=yes` is not meaningful, but it must not consume the
    // next argument as its value — that would silently swallow a path.
    let (dir, model, _library) = fixture("switch");
    let model = model.to_string_lossy().into_owned();

    let (code, stdout, _stderr) = run(&["check", &model, "--json=yes"]);
    assert_ne!(code, 2, "`--json=yes` must not be a usage error");
    assert!(
        stdout.trim_start().starts_with('{'),
        "the switch must still produce JSON, got: {stdout}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
