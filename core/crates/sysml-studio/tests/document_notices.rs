use std::path::PathBuf;
use sysml_studio::document::Session;
use sysml_view::{Mode, View};

fn fixture(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("sysml-notices-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&path).expect("test directory");
    let model = path.join("m.sysml");
    std::fs::write(&model, "package P {\n private import ScalarValues::*;\n part def A { attribute x : Real = 1; }\n}\n").expect("test model");
    model
}

#[test]
fn a_scoped_diagram_truncates_descendants_and_reports_the_collapsed_count() {
    let path = fixture("scope-depth");
    std::fs::write(
        &path,
        "package P { part def A { part child { part grandchild; } } }",
    )
    .expect("write nested model");
    let mut session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    session.scope = Some("P::A".to_string());
    session.depth = Some(1);
    session.style = Some(sysml_view::layered::Style::Tree);
    let doc = session.document().expect("scoped document");
    assert!(doc.scoped);
    assert_eq!(doc.depth, Some(1));
    assert_eq!(doc.layout, "tree");
    assert!(
        doc.nodes.iter().any(|n| n.key == "P::A::child"),
        "nodes: {:?}",
        doc.nodes.iter().map(|n| &n.key).collect::<Vec<_>>()
    );
    assert!(!doc.nodes.iter().any(|n| n.key == "P::A::child::grandchild"));
    let owner = doc
        .nodes
        .iter()
        .find(|n| n.key == "P::A::child")
        .expect("collapsed owner");
    assert!(owner
        .rows
        .iter()
        .any(|row| row == "… 1 nested elements collapsed"));
}

#[test]
fn a_depth_without_a_scope_limits_the_whole_model() {
    let path = fixture("whole-depth");
    std::fs::write(
        &path,
        "package P { part def A { part child { part grandchild; } } }",
    )
    .expect("write nested model");
    let mut session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    session.depth = Some(2);
    let doc = session.document().expect("depth-limited document");
    assert!(!doc.scoped);
    // The depth counts from the outermost drawn element, which without a scope is the package.
    assert!(!doc.nodes.iter().any(|n| n.key == "P::A::child::grandchild"));
    let owner = doc
        .nodes
        .iter()
        .find(|n| n.key == "P::A::child")
        .expect("collapsed owner");
    assert!(owner
        .rows
        .iter()
        .any(|row| row == "… 1 nested elements collapsed"));
}

#[test]
fn depth_pruning_keeps_later_sibling_type_references_valid() {
    let path = fixture("depth-sibling-ids");
    std::fs::write(
        &path,
        "package P { part def A { part child { part grandchild; } } part def B; part b : B; }",
    )
    .expect("write sibling model");
    let mut session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    for depth in [1, 2, 3] {
        session.depth = Some(depth);
        let doc = session.document().expect("depth-limited document");
        assert!(doc.nodes.iter().any(|node| node.key == "P::b"));
        assert!(doc.edges.iter().any(|edge| edge.arrow == "typing"));
    }
}

#[test]
fn an_unknown_element_scope_is_an_actionable_error() {
    let path = fixture("missing-scope");
    let mut session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    session.scope = Some("P::deleted".to_string());
    let message = session.document().err().expect("missing element refused");
    assert!(message.contains("P::deleted") && message.contains("Refresh the diagram"));
}

#[test]
fn an_unstarted_ledger_is_one_information_notice_with_accurate_counts() {
    let path = fixture("unstarted");
    let session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    let doc = session.document().expect("document");
    let notices: Vec<_> = doc
        .issues
        .iter()
        .filter(|i| i.code.starts_with("PROV"))
        .collect();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].code, "PROV000");
    assert_eq!(notices[0].severity, "info");
    assert!(notices[0]
        .message
        .contains("1 value(s) and 1 part definition(s)"));
    assert_eq!(doc.summary.errors, 0);
}

#[test]
fn a_removed_declared_view_is_an_actionable_error_not_an_empty_picture() {
    let path = fixture("removed-view");
    let mut session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    session.model_view = Some("P::removed".to_string());
    let message = session.document().err().expect("missing view refused");
    assert!(message.contains("P::removed") && message.contains("Choose another diagram"));
}
