//! `sysml.project.json`: what a project is, and what naming it in a file buys.
//!
//! The manifest exists to answer three questions a command line cannot: which files are the model,
//! which library it was written against, and where its ledger lives. Each test below is one of those
//! answers, plus the ordering rule that makes two machines load one project the same way.

use std::fs;
use std::path::{Path, PathBuf};

use sysml_syntax::project::{
    constraint_expr, discover, expand, parse_filter, Project, Stdlib, MANIFEST,
};

/// A directory of our own, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let path =
            std::env::temp_dir().join(format!("sysml-project-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch directory");
        Scratch(path)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory");
        }
        fs::write(&path, text).expect("write");
        path
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_directory_expands_to_its_sources_in_a_stable_order() {
    // `read_dir` returns entries in filesystem order, so an unsorted expansion would load the same
    // project differently on another machine — and ids are assigned in load order.
    let scratch = Scratch::new("order");
    scratch.write("src/b.sysml", "package B {}");
    scratch.write("src/a.sysml", "package A {}");
    scratch.write("src/deeper/c.sysml", "package C {}");
    scratch.write("src/notes.md", "not a model");

    let files = expand(&scratch.0.join("src")).expect("expands");
    let names: Vec<String> = files
        .iter()
        .map(|p| {
            p.strip_prefix(&scratch.0)
                .unwrap()
                .display()
                .to_string()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(names, ["src/a.sysml", "src/b.sysml", "src/deeper/c.sysml"]);
}

#[test]
fn a_file_named_twice_is_one_file() {
    // A manifest that lists a directory and one of its members names that member twice. Lowering it
    // twice would report every name in it twice.
    let scratch = Scratch::new("dedupe");
    scratch.write("src/a.sysml", "package A {}");
    scratch.write(
        "sysml.project.json",
        r#"{"sources": ["src", "src/a.sysml"]}"#,
    );

    let project = Project::read(&scratch.0.join(MANIFEST)).expect("reads");
    assert_eq!(project.sources.len(), 1, "got {:?}", project.sources);
}

#[test]
fn a_library_file_is_reachable_but_not_one_of_the_sources() {
    let scratch = Scratch::new("library");
    scratch.write("src/a.sysml", "package A {}");
    scratch.write("corpus/b.sysml", "package Corpus {}");
    scratch.write(
        "sysml.project.json",
        r#"{"sources": ["src"], "library": ["corpus"]}"#,
    );

    let project = Project::read(&scratch.0.join(MANIFEST)).expect("reads");
    assert_eq!(project.sources.len(), 1);
    assert_eq!(project.library.len(), 1);
    assert!(project.library[0].ends_with("b.sysml"));
}

#[test]
fn an_empty_source_list_is_named_as_the_mistake() {
    let scratch = Scratch::new("empty");
    scratch.write("sysml.project.json", r#"{"sources": []}"#);
    let error = Project::read(&scratch.0.join(MANIFEST)).expect_err("must not read");
    assert!(error.contains("`sources` is empty"), "{error}");
}

#[test]
fn a_missing_source_names_the_path_it_looked_for() {
    let scratch = Scratch::new("missing");
    scratch.write("sysml.project.json", r#"{"sources": ["nope"]}"#);
    let error = Project::read(&scratch.0.join(MANIFEST)).expect_err("must not read");
    assert!(error.contains("nope"), "{error}");
}

#[test]
fn the_ledger_defaults_to_the_project_name() {
    // One ledger for the project, not one per file: a model split across ten files has one set of
    // decisions behind it.
    let scratch = Scratch::new("ledger");
    scratch.write("src/a.sysml", "package A {}");
    scratch.write(
        "sysml.project.json",
        r#"{"name": "vehicle", "sources": ["src"]}"#,
    );
    let project = Project::read(&scratch.0.join(MANIFEST)).expect("reads");
    assert!(
        project.ledger.ends_with("vehicle.ledger.json"),
        "{:?}",
        project.ledger
    );
}

#[test]
fn a_ledger_named_in_the_manifest_is_used_as_written() {
    let scratch = Scratch::new("ledger-named");
    scratch.write("src/a.sysml", "package A {}");
    scratch.write(
        "sysml.project.json",
        r#"{"name": "vehicle", "sources": ["src"], "ledger": "decisions.json"}"#,
    );
    let project = Project::read(&scratch.0.join(MANIFEST)).expect("reads");
    assert!(
        project.ledger.ends_with("decisions.json"),
        "{:?}",
        project.ledger
    );
}

#[test]
fn the_library_defaults_to_discovery_and_can_be_pinned() {
    let scratch = Scratch::new("stdlib");
    scratch.write("src/a.sysml", "package A {}");
    scratch.write("stdlib/Kernel.sysml", "package Kernel {}");

    scratch.write("sysml.project.json", r#"{"sources": ["src"]}"#);
    let project = Project::read(&scratch.0.join(MANIFEST)).expect("reads");
    assert_eq!(project.stdlib, Stdlib::Discovered);

    scratch.write(
        "sysml.project.json",
        r#"{"sources": ["src"], "stdlib": "stdlib"}"#,
    );
    let project = Project::read(&scratch.0.join(MANIFEST)).expect("reads");
    assert!(matches!(project.stdlib, Stdlib::Pinned(_)));

    scratch.write(
        "sysml.project.json",
        r#"{"sources": ["src"], "stdlib": "bundled"}"#,
    );
    let project = Project::read(&scratch.0.join(MANIFEST)).expect("reads");
    assert_eq!(project.stdlib, Stdlib::Bundled);

    // A pin that names no directory is a mistake to say out loud: silently falling back to
    // discovery would check the model against a library nobody chose.
    scratch.write(
        "sysml.project.json",
        r#"{"sources": ["src"], "stdlib": "no-such-library"}"#,
    );
    let error = Project::read(&scratch.0.join(MANIFEST)).expect_err("must not read");
    assert!(error.contains("no directory"), "{error}");
}

#[test]
fn the_manifest_is_found_from_a_subdirectory() {
    // A command run from `src/` is describing the same project. Refusing to find the manifest there
    // would make it a thing you have to stand in front of.
    let scratch = Scratch::new("discover");
    scratch.write("src/a.sysml", "package A {}");
    let manifest = scratch.write("sysml.project.json", r#"{"sources": ["src"]}"#);

    let found = discover(&scratch.0.join("src")).expect("found from below");
    assert_eq!(
        fs::canonicalize(found).unwrap(),
        fs::canonicalize(&manifest).unwrap()
    );
}

#[test]
fn a_project_with_a_pinned_library_loads_its_sources() {
    // The end-to-end shape: two files, one of which imports the other's package. Loading them as
    // one project is the whole point of naming a directory instead of listing files.
    let scratch = Scratch::new("load");
    scratch.write("src/a.sysml", "package A {\n    part def Wheel;\n}\n");
    scratch.write(
        "src/b.sysml",
        "package B {\n    private import A::*;\n    part front : Wheel;\n}\n",
    );
    scratch.write(
        "sysml.project.json",
        r#"{"name": "demo", "sources": ["src"], "stdlib": "bundled"}"#,
    );

    let project = Project::read(&scratch.0.join(MANIFEST)).expect("reads");
    let loaded = project.load().expect("loads");
    assert_eq!(loaded.roots.len(), 2, "one root per file");
}

// -- annotation filters ------------------------------------------------------

#[test]
fn a_filter_names_an_annotation_and_may_narrow_it() {
    let plain = parse_filter("@Safety").expect("parses");
    assert_eq!(plain.name, "Safety");
    assert_eq!(plain.constraint, None);

    let narrowed = parse_filter("@Safety{level >= 2}").expect("parses");
    assert_eq!(narrowed.name, "Safety");
    assert_eq!(narrowed.constraint.as_deref(), Some("level >= 2"));
}

#[test]
fn the_spellings_a_person_writes_name_the_same_annotation() {
    // `@//safety/` is how a path is written elsewhere; a filter reads the last segment either way.
    for text in ["@Safety", "@FMEA::Safety", "@//safety/"] {
        let parsed = parse_filter(text).expect("parses");
        assert_eq!(parsed.name.to_lowercase(), "safety", "{text}");
    }
}

#[test]
fn a_filter_without_an_at_sign_is_refused_with_the_form() {
    let error = parse_filter("Safety").expect_err("must refuse");
    assert!(
        error.contains("@Safety"),
        "the message shows what to type: {error}"
    );
}

#[test]
fn a_constraint_is_parsed_by_the_model_parser() {
    // The filter's expression and the model's expressions have to agree, or a filter matches nothing
    // on a model the checker reads fine.
    let expr = constraint_expr("level >= 2").expect("parses");
    assert!(matches!(expr, sysml_model::Expr::Binary { .. }), "{expr:?}");
    // An operator with nothing after it is not an expression, and the message has to say so rather
    // than hand the evaluator a node that means nothing.
    let error = constraint_expr("level >=").expect_err("must refuse");
    assert!(error.contains("level >="), "{error}");
}
