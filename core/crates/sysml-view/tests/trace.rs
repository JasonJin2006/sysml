//! The requirement traceability view, held to the question it exists to answer.
//!
//! Every test here is one half of "is this requirement implemented, and by what": the view is
//! only useful if a satisfied requirement shows its satisfier and an unsatisfied one shows that
//! nothing does. A view that draws both the same way is worse than the general view, which at
//! least does not claim to answer the question.

use sysml_syntax::load;

const SATISFIED: &str = r#"package P {
    part def Drone { attribute m : Real = 5; }
    requirement def Limit { subject d : Drone; require constraint { d.m <= 6 } }
    part drone : Drone;
    satisfy Limit by drone;
}
"#;

const ORPHANED: &str = r#"package P {
    part def Drone { attribute m : Real = 5; }
    requirement def Met { subject d : Drone; require constraint { d.m <= 6 } }
    requirement def Orphan { subject d : Drone; require constraint { d.m >= 99 } }
    part drone : Drone;
    satisfy Met by drone;
}
"#;

#[test]
fn a_satisfied_requirement_names_its_satisfier() {
    let loaded = load(SATISFIED).expect("parses");
    let traced = sysml_view::trace::traceability(&loaded.roots);
    let limit = traced
        .traces
        .iter()
        .find(|t| traced.graph.nodes[t.node].label() == "Limit")
        .expect("Limit is a requirement in the view");
    assert!(!limit.is_unmet(), "it is satisfied");
    assert_eq!(limit.satisfied_by.len(), 1, "by exactly one part");
    let satisfier = &traced.graph.nodes[limit.satisfied_by[0]];
    assert_eq!(satisfier.label(), "drone", "the part the satisfy names");
    // And the edge is drawn, so the picture shows what the trace records.
    assert_eq!(traced.graph.edges.len(), 1);
}

/// The case the view exists for. A requirement with a subject but no `satisfy` is not
/// implemented — the subject says what it is about, not that anything meets it.
#[test]
fn a_requirement_nothing_satisfies_is_marked() {
    let loaded = load(ORPHANED).expect("parses");
    let traced = sysml_view::trace::traceability(&loaded.roots);
    let orphan = traced
        .traces
        .iter()
        .find(|t| traced.graph.nodes[t.node].label() == "Orphan")
        .expect("Orphan is in the view");
    assert!(orphan.is_unmet(), "nothing satisfies it");
    assert!(
        orphan.subject.is_some(),
        "it still records what it is about; that is not the same as being met"
    );

    let met = traced
        .traces
        .iter()
        .find(|t| traced.graph.nodes[t.node].label() == "Met")
        .expect("Met is in the view");
    assert!(!met.is_unmet(), "Met is satisfied");
}

/// Two implementers hanging off one row must not land on the same coordinate.
///
/// A requirement's subject and the part that satisfies it are different elements, and both are
/// pushed into the right column. Before the collision check they were drawn at the same y, which
/// rendered as one box with doubled text — the picture claimed one implementer where there were
/// two.
#[test]
fn implementers_on_one_row_do_not_overlap() {
    let loaded = load(ORPHANED).expect("parses");
    let traced = sysml_view::trace::traceability(&loaded.roots);
    let geom = sysml_view::traceability_geometry(&traced.graph);
    let mut ys: Vec<f32> = geom
        .boxes
        .iter()
        .filter(|(i, ..)| !traced.graph.nodes[*i].stereotype.starts_with("requirement"))
        .map(|(_, _, y, _, _)| *y)
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for pair in ys.windows(2) {
        assert!(
            (pair[1] - pair[0]).abs() > 1.0,
            "two implementer boxes share a y: {ys:?}"
        );
    }
}

/// A requirement with no subject and no satisfy is still a requirement, and still gets a row.
#[test]
fn a_bare_requirement_still_appears() {
    let loaded = load(
        r#"package P {
    requirement def Bare;
}
"#,
    )
    .expect("parses");
    let traced = sysml_view::trace::traceability(&loaded.roots);
    assert_eq!(traced.traces.len(), 1);
    assert!(traced.traces[0].is_unmet());
    assert_eq!(
        traced.graph.nodes.len(),
        1,
        "one requirement, no implementers"
    );
}

/// The constraint text is quoted on the requirement's own row, so a reviewer can see what has to
/// hold without opening the source.
#[test]
fn the_constraint_text_travels_with_the_requirement() {
    let loaded = load(SATISFIED).expect("parses");
    let traced = sysml_view::trace::traceability(&loaded.roots);
    let limit = traced
        .traces
        .iter()
        .find(|t| traced.graph.nodes[t.node].label() == "Limit")
        .expect("Limit is in the view");
    let text = limit.predicate.as_deref().unwrap_or("");
    assert!(
        text.contains("d.m") && text.contains('6'),
        "the predicate is quoted: {text}"
    );
}

/// The view must not invent a link from a name inside a constraint.
///
/// `require constraint { d.m <= 6 }` mentions `Drone` through `d`, and inferring that the
/// requirement is "implemented by Drone" would be a guess. The trace keeps the subject, which is
/// what the model states, and draws no edge.
#[test]
fn a_subject_is_recorded_but_draws_no_edge() {
    let loaded = load(ORPHANED).expect("parses");
    let traced = sysml_view::trace::traceability(&loaded.roots);
    let orphan = traced
        .traces
        .iter()
        .find(|t| traced.graph.nodes[t.node].label() == "Orphan")
        .expect("Orphan is in the view");
    assert!(orphan.subject.is_some(), "the subject is known");
    assert!(
        orphan.satisfied_by.is_empty(),
        "but nothing states it is satisfied"
    );
    // Only `Met by drone` produces an edge.
    assert_eq!(traced.graph.edges.len(), 1);
}

#[test]
fn an_annotation_is_an_edge_to_the_type_that_carries_it() {
    // `@Safety { level = 3 }` on a part is a relationship, and a traceability view that left it out
    // answered "what satisfies what" while saying nothing about the parts themselves.
    const ANNOTATED: &str = r#"package P {
        metadata def Safety { attribute level : Integer; }
        @Safety { level = 3; }
        part def Brake;
        @Safety { level = 1; }
        part def Cup;
    }
    "#;
    let loaded = load(ANNOTATED).expect("model parses");
    let graph = sysml_view::project(
        &loaded.roots,
        sysml_view::View::Traceability,
        sysml_view::Mode::Extended,
    );
    let annotation_edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| edge.arrow == sysml_view::Arrow::Annotate)
        .collect();
    assert_eq!(annotation_edges.len(), 2, "one edge per annotation");
    // Both parts are annotated by the same definition, so the two edges meet at one node.
    assert_eq!(annotation_edges[0].to, annotation_edges[1].to);
    let labels: Vec<String> = annotation_edges
        .iter()
        .map(|edge| edge.label.clone().unwrap_or_default())
        .collect();
    assert!(labels.iter().any(|l| l.contains("level = 3")), "{labels:?}");
    assert!(labels.iter().any(|l| l.contains("level = 1")), "{labels:?}");
}
