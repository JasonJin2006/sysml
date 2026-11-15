//! What an analysis or verification case says about itself.
//!
//! A case is a claim about a thing, not a structure holding one. Its `subject` names what is being
//! analysed and its `objective` names what the analysis is for; both are members in the grammar, so
//! a plain walk draws them as two boxes *below* the case, which reads as "a case that contains a
//! vehicle". The notation puts them in compartments of the case, and so does this.

use sysml_view::graph;

fn rows(src: &str, box_name: &str) -> Vec<String> {
    let loaded = sysml_syntax::load(src).expect("parses");
    let g = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    g.nodes
        .iter()
        .find(|n| n.label() == box_name)
        .unwrap_or_else(|| panic!("no box called {box_name}"))
        .attributes
        .clone()
}

fn has_box(src: &str, name: &str) -> bool {
    let loaded = sysml_syntax::load(src).expect("parses");
    graph::general(&loaded.roots, sysml_view::Mode::Extended)
        .nodes
        .iter()
        .any(|n| n.label() == name)
}

#[test]
fn a_subject_is_a_compartment_row_not_a_box() {
    let src = "package P { part def V; analysis def A { subject vehicle : V; } }";
    let rows = rows(src, "A");
    assert!(
        rows.iter().any(|r| r == "subject vehicle : V"),
        "the subject is named in the case's own rows: {rows:?}"
    );
    assert!(
        !has_box(src, "vehicle"),
        "the subject must not also be drawn as a separate box"
    );
}

#[test]
fn an_objective_names_what_it_is_about() {
    let src = "package P { requirement def R; analysis def A { objective obj : R; } }";
    let rows = rows(src, "A");
    assert!(
        rows.iter().any(|r| r == "objective obj : R"),
        "the objective states its target: {rows:?}"
    );
}

#[test]
fn an_anonymous_objective_is_not_named_twice() {
    // The loader gives a nameless objective the keyword as its name, so a row built from that
    // name reads `objective objective`.
    let src = "package P { analysis def A { objective { } } }";
    let rows = rows(src, "A");
    assert!(
        rows.iter().any(|r| r == "objective"),
        "one word, once: {rows:?}"
    );
    assert!(!rows.iter().any(|r| r == "objective objective"), "{rows:?}");
}

#[test]
fn a_redefined_subject_is_named_by_what_it_redefines() {
    // `subject :>> vehicle = V()` writes no name of its own; the redefined feature is the only
    // name the row can carry, and without it the row reads `subject  : vehicle`.
    let src = "package P {
        part def V;
        analysis def A { subject vehicle : V; }
        analysis inst : A { subject :>> vehicle = V(); }
    }";
    let rows = rows(src, "inst");
    assert!(
        rows.iter().any(|r| r.starts_with("subject vehicle")),
        "the redefined name is kept: {rows:?}"
    );
    assert!(
        !rows.iter().any(|r| r.starts_with("subject  ")),
        "no row has an empty name: {rows:?}"
    );
}

#[test]
fn an_objective_quotes_the_sentence_it_states() {
    // The prose is what the objective *is*; without it the row can only say the case has one.
    let src = "package P {
        requirement def R;
        analysis def A {
            objective obj : R {
                doc /* Determine whether the vehicle meets its range target. */
            }
        }
    }";
    let rows = rows(src, "A");
    let row = rows
        .iter()
        .find(|r| r.starts_with("objective"))
        .unwrap_or_else(|| panic!("no objective row: {rows:?}"));
    assert!(
        row.contains("Determine whether the vehicle meets its range target"),
        "the objective's own sentence is quoted: {row}"
    );
}

#[test]
fn a_long_objective_is_cut_to_one_row() {
    // A compartment row is one line. A sentence that runs past the row width would widen the box
    // and push every neighbour aside for text the reader can find in the file.
    let src = "package P {
        analysis def A {
            objective obj {
                doc /* This is a sentence that goes on and on and on well past the width of a compartment row and then some more. */
            }
        }
    }";
    let rows = rows(src, "A");
    let row = rows
        .iter()
        .find(|r| r.starts_with("objective"))
        .expect("a row");
    assert!(row.chars().count() < 120, "the row stays one line: {row}");
    assert!(row.ends_with('…'), "a cut is shown as one: {row}");
}

#[test]
fn an_ordinary_part_is_still_a_box() {
    // The compartment rule is about a case's subject and objective, not about members in general:
    // a part of a definition is still drawn as a box, or every diagram becomes a list of rows.
    let src = "package P { part def A { part b : A; } }";
    assert!(has_box(src, "b"), "a plain member is still a box");
    assert!(rows(src, "A").is_empty(), "and it is not a row");
}
