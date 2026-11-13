//! What a model says about a standard's clauses, against the clauses themselves.

use sysml_ledger::clauses::{self, TraceReport};

fn ledger() -> sysml_ledger::clauses::ClauseLedger {
    clauses::read(
        r#"{"id":"4.1","title":"General concepts","kind":"normative"}
{"id":"5.3.2.2.2","title":"Registration","kind":"normative"}
{"id":"3.2","title":"Abbreviations","kind":"descriptive"}
"#,
    )
    .expect("ledger parses")
}

fn trace(src: &str) -> TraceReport {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    clauses::trace(&loaded.roots, &ledger())
}

#[test]
fn a_citation_of_a_clause_the_ledger_has_is_not_dangling() {
    let report = trace(
        "package P {\n\trequirement R {\n\t\tdoc /* TS 23.501 5.3.2.2.2: registration */\n\
         \t\trequire constraint { 1 > 0 }\n\t}\n}\n",
    );
    assert!(report.dangling.is_empty(), "{:?}", report.dangling);
    assert_eq!(report.cited.len(), 1);
    assert_eq!(report.cited[0].clause, "5.3.2.2.2");
}

#[test]
fn a_citation_of_a_clause_the_ledger_lacks_is_dangling() {
    let report = trace(
        "package P {\n\trequirement R {\n\t\tdoc /* TS 23.501 99.99.99: invented */\n\
         \t\trequire constraint { 1 > 0 }\n\t}\n}\n",
    );
    assert_eq!(report.dangling.len(), 1, "{:?}", report.dangling);
    assert_eq!(report.dangling[0].clause, "99.99.99");
}

#[test]
fn the_standard_number_is_not_read_as_a_clause() {
    // `TS 23.501 5.3.2.2.2` names the document and then the clause. `23.501` has the shape of a
    // clause number, and reading it as one made every citation dangling.
    let report = trace(
        "package P {\n\trequirement R {\n\t\tdoc /* TS 23.501 5.3.2.2.2: registration */\n\
         \t\trequire constraint { 1 > 0 }\n\t}\n}\n",
    );
    assert!(
        !report.cited.iter().any(|c| c.clause == "23.501"),
        "{:?}",
        report.cited
    );
}

#[test]
fn a_top_level_clause_is_read() {
    // `4.1` has one dot. Requiring two dropped every top-level citation, and the clause then read
    // as uncovered when it was covered.
    let report = trace(
        "package P {\n\trequirement R {\n\t\tdoc /* TS 23.501 4.1: concepts */\n\
         \t\trequire constraint { 1 > 0 }\n\t}\n}\n",
    );
    assert_eq!(report.cited.len(), 1, "{:?}", report.cited);
    assert!(
        !report.uncovered.iter().any(|c| c.id == "4.1"),
        "the cited clause is covered: {:?}",
        report.uncovered
    );
}

#[test]
fn citing_a_descriptive_clause_is_reported_separately() {
    let report = trace(
        "package P {\n\trequirement R {\n\t\tdoc /* TS 23.501 3.2: abbreviations */\n\
         \t\trequire constraint { 1 > 0 }\n\t}\n}\n",
    );
    assert_eq!(report.not_normative.len(), 1, "{:?}", report.not_normative);
    assert_eq!(report.not_normative[0].clause, "3.2");
}

#[test]
fn an_uncited_normative_clause_is_reported_as_uncovered() {
    let report = trace(
        "package P {\n\trequirement R {\n\t\tdoc /* TS 23.501 4.1: concepts */\n\
         \t\trequire constraint { 1 > 0 }\n\t}\n}\n",
    );
    assert_eq!(report.normative_total, 2);
    assert_eq!(report.uncovered.len(), 1, "{:?}", report.uncovered);
    assert_eq!(report.uncovered[0].id, "5.3.2.2.2");
}

#[test]
fn a_requirement_with_no_doc_cites_nothing() {
    let report =
        trace("package P {\n\trequirement R {\n\t\trequire constraint { 1 > 0 }\n\t}\n}\n");
    assert!(report.cited.is_empty(), "{:?}", report.cited);
}

#[test]
fn only_requirements_are_read_for_citations() {
    // A `part def` may carry a `doc` that happens to name a clause. It is not a requirement, so
    // it makes no claim about the standard.
    let report = trace(
        "package P {\n\tpart def D {\n\t\tdoc /* TS 23.501 5.3.2.2.2: registration */\n\t}\n}\n",
    );
    assert!(report.cited.is_empty(), "{:?}", report.cited);
}

#[test]
fn a_ledger_line_without_an_id_is_an_error() {
    let err = clauses::read("{\"title\":\"no id\"}\n").unwrap_err();
    assert!(err.contains("no `id`"), "{err}");
}
