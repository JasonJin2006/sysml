//! The shape of every `--json` answer, pinned.
//!
//! An agent reads these documents instead of the printed text, so a renamed or dropped field is
//! a breaking change even though nothing in the crate fails to compile. Each test asserts the
//! key set, not the values: the values are the model's business, the shape is the contract.
//!
//! These run the binary rather than calling into it, because the contract is the command line —
//! which flags exist, what they print, and what the process answers. A unit test on the
//! serialiser would keep passing after the flag stopped being wired up.

use std::process::Command;

fn sysml() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sysml"))
}

/// A model with one typed attribute, so every command has something to report.
///
/// `Integer` is imported rather than written bare: an unresolved name is a semantic error, and a
/// semantic error outranks "the ledger is empty", so the tests below about ledger state need a
/// model that is semantically clean to reach it.
const MODEL: &str = "package P {\n\tprivate import ScalarValues::*;\n\tpart def A {\n\t\tattribute x : Integer = 5;\n\t}\n}\n";

struct Built {
    dir: std::path::PathBuf,
    model: std::path::PathBuf,
}

impl Built {
    fn new(tag: &str) -> Self {
        // The tag alone is not unique: `every_command_that_prints_a_document_accepts_the_flag`
        // builds one per command name, and the per-command tests use the same names. Two
        // directories sharing a path meant one test's `remove_dir_all` deleted the model the
        // other had just written, and the loser saw an empty stdout. The counter makes each
        // instance distinct within the process, and the process id across concurrent runs.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("sysml-json-{tag}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let model = dir.join("m.sysml");
        std::fs::write(&model, MODEL).expect("write model");
        Built { dir, model }
    }

    fn run(&self, args: &[&str]) -> (String, i32) {
        let output = sysml()
            .arg(args[0])
            .arg(&self.model)
            .args(&args[1..])
            .output()
            .expect("the binary runs");
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            output.status.code().unwrap_or(-1),
        )
    }
}

impl Drop for Built {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn parse(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
}

fn keys(value: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

#[test]
fn check_reports_the_keys_a_caller_reads() {
    let built = Built::new("check");
    let (out, code) = built.run(&["check", "--json"]);
    let doc = parse(&out);
    assert_eq!(
        keys(&doc),
        [
            "blocked",
            "errorCount",
            "issues",
            "ledgerStarted",
            "notModelled",
            "ok",
            "provenanceErrors",
            "provenanceIssues",
            "semanticErrors",
            "status",
            "unaccountedPartCount",
            "unaccountedParts",
            "verdicts",
            "warningCount",
        ]
    );
    // A model with a value and no ledger is unfinished, and that has to be visible in two
    // places at once: the document and the exit code. A caller reading only `ok` would treat
    // "nobody has recorded where this came from" as "this model is broken".
    assert_eq!(doc["status"], "unfinished", "{doc}");
    assert_eq!(doc["ok"], false);
    assert_eq!(
        code, 3,
        "unfinished is not the same failure as a violated rule"
    );
}

#[test]
fn status_reports_the_same_four_facts() {
    let built = Built::new("status");
    let (out, code) = built.run(&["status", "--json"]);
    let doc = parse(&out);
    assert_eq!(
        keys(&doc),
        [
            "blocked",
            "issues",
            "ledgerStarted",
            "provenanceErrors",
            "semanticErrors",
            "status",
            "summary",
            "unaccountedParts",
        ]
    );
    assert_eq!(doc["status"], "unfinished", "{doc}");
    assert_eq!(doc["semanticErrors"], 0);
    assert_eq!(code, 3);
}

#[test]
fn outline_answers_a_tree_with_names_and_lines() {
    let built = Built::new("outline");
    let (out, code) = built.run(&["outline", "--json"]);
    let doc = parse(&out);
    assert_eq!(code, 0, "reading a model is not a verdict");
    let roots = doc.as_array().expect("a list of roots");
    assert_eq!(roots.len(), 1, "one package");
    assert_eq!(roots[0]["name"], "P");
    assert_eq!(roots[0]["qualified"], "P");
    assert_eq!(roots[0]["kind"], "package");
    // The package's first child is the import; find the part by name rather than by position,
    // so an import added to the fixture does not silently point this test at the wrong element.
    let children = roots[0]["children"].as_array().expect("children");
    let child = children
        .iter()
        .find(|c| c["name"] == "A")
        .unwrap_or_else(|| panic!("no part A in {children:?}"));
    assert_eq!(child["qualified"], "P::A");
    // The attribute is the only element that carries a written type, and the type is the
    // authored text: this command does not resolve, so it must not pretend to.
    let attribute = &child["children"][0];
    assert_eq!(attribute["name"], "x");
    assert_eq!(attribute["type"], "Integer");
}

#[test]
fn resolve_separates_what_bound_from_what_did_not() {
    let built = Built::new("resolve");
    let (out, code) = built.run(&["resolve", "--json"]);
    let doc = parse(&out);
    assert_eq!(
        keys(&doc),
        [
            "ambiguous",
            "imports",
            "notAccessible",
            "resolved",
            "status",
            "symbols",
            "unresolved"
        ]
    );
    // The type is imported, so everything binds and the command answers `pass`.
    assert_eq!(doc["status"], "pass", "{doc}");
    assert_eq!(code, 0);
    assert!(
        !doc["resolved"].as_array().expect("a list").is_empty(),
        "the bound reference must be listed: {doc}"
    );
    // Arrays are always present, so a consumer never tells "absent" apart from "none".
    assert!(doc["ambiguous"].as_array().expect("a list").is_empty());
    assert!(doc["unresolved"].as_array().expect("a list").is_empty());
}

#[test]
fn graph_answers_nodes_and_edges() {
    let built = Built::new("graph");
    let (out, code) = built.run(&["graph", "--json"]);
    let doc = parse(&out);
    assert_eq!(keys(&doc), ["edges", "nodes"]);
    assert_eq!(code, 0);
    let nodes = doc["nodes"].as_array().expect("a list");
    assert_eq!(nodes.len(), 2, "the part def and its attribute: {doc}");
    assert_eq!(nodes[0]["alias"], "E1");
    assert_eq!(nodes[0]["kind"], "part def");
    assert_eq!(nodes[0]["isDefinition"], true);
}

#[test]
fn every_command_that_prints_a_document_accepts_the_flag() {
    // The five commands that had no JSON used to accept `--json` and ignore it, which is worse
    // than refusing: a caller got a human table and no signal that the contract had not applied.
    for command in ["outline", "resolve", "graph", "status", "check"] {
        let built = Built::new(command);
        let (out, _) = built.run(&[command, "--json"]);
        parse(&out);
    }
}
