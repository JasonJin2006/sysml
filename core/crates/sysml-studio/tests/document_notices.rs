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
fn node_folding_preserves_siblings_counts_and_typing() {
    let path = fixture("node-fold-siblings");
    std::fs::write(
        &path,
        "package P { part def A { part child { part grandchild; } } part def B; part b : B; }",
    )
    .expect("write model");
    let mut session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    session.folded.insert("P::A".to_string(), true);
    let doc = session.document().expect("folded document");
    let owner = doc.nodes.iter().find(|n| n.key == "P::A").unwrap();
    assert_eq!((owner.descendant_count, owner.hidden_count), (2, 2));
    assert!(owner
        .rows
        .iter()
        .any(|row| row == "… 2 nested elements collapsed"));
    assert!(!doc.nodes.iter().any(|n| n.key.starts_with("P::A::")));
    assert!(doc.nodes.iter().any(|n| n.key == "P::B"));
    assert!(doc.nodes.iter().any(|n| n.key == "P::b"));
    assert!(doc.edges.iter().any(|edge| {
        edge.arrow == "typing"
            && edge.from_key == "P::b"
            && edge.to_key == "P::B"
            && doc.nodes[edge.from].key == edge.from_key
            && doc.nodes[edge.to].key == edge.to_key
    }));
}

#[test]
fn scoped_expansion_overrides_depth_and_remembers_nested_folds() {
    let path = fixture("node-fold-scoped");
    std::fs::write(
        &path,
        "package P { part def A { part child { part grandchild { part leaf; } } part sibling { part hidden; } } }",
    )
    .expect("write model");
    let mut session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    session.scope = Some("P::A".to_string());
    session.depth = Some(1);
    session.folded.insert("P::A::child".to_string(), false);
    session
        .folded
        .insert("P::A::child::grandchild".to_string(), true);
    let doc = session.document().expect("expanded past depth");
    let child = doc.nodes.iter().find(|n| n.key == "P::A::child").unwrap();
    assert_eq!((child.descendant_count, child.hidden_count), (2, 0));
    let nested = doc
        .nodes
        .iter()
        .find(|n| n.key == "P::A::child::grandchild")
        .unwrap();
    assert_eq!((nested.descendant_count, nested.hidden_count), (1, 1));
    assert!(!doc.nodes.iter().any(|n| n.key.ends_with("::hidden")));
    session.folded.insert("P::A".to_string(), true);
    let doc = session.document().expect("parent folded");
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(doc.nodes[0].hidden_count, 5);
    session.folded.insert("P::A".to_string(), false);
    let doc = session.document().expect("parent expanded");
    assert!(doc.nodes.iter().any(|n| n.key.ends_with("::hidden")));
    assert!(!doc.nodes.iter().any(|n| n.key.ends_with("::leaf")));
    assert!(session.folded["P::A::child::grandchild"]);
}

#[test]
fn folding_is_limited_to_supported_builtin_views() {
    let path = fixture("node-fold-views");
    let mut session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    for view in [View::General, View::StateTransition, View::ActionFlow] {
        session.view = view;
        session.folded.insert("P::Thing".to_string(), false);
        assert!(session.document().is_ok());
    }
    for view in [
        View::Interconnection,
        View::Traceability,
        View::Packages,
        View::Sequence,
    ] {
        session.view = view;
        assert!(session.document().unwrap_err().contains("Node folding"));
    }
    session.view = View::General;
    session.model_view = Some("P::declared".to_string());
    assert!(session.document().unwrap_err().contains("Node folding"));
}

#[test]
fn state_and_action_folding_preserve_real_nested_siblings() {
    for (tag, view, source) in [
        (
            "state",
            View::StateTransition,
            "package P { state def M { state outer { state inner { state leaf; } } state sibling; } }",
        ),
        (
            "action",
            View::ActionFlow,
            "package P { action def M { action outer { action inner { action leaf; } } action sibling; } }",
        ),
    ] {
        let path = fixture(tag);
        std::fs::write(&path, source).expect("nested behavior model");
        let mut session = Session::open(vec![path], view, Mode::Extended).expect("session");
        session.scope = Some("P::M".to_string());
        session.depth = Some(1);
        session.folded.insert("P::M::outer".to_string(), false);
        let doc = session.document().expect("expanded behavior");
        assert!(doc.nodes.iter().any(|n| n.key == "P::M::outer::inner::leaf"));
        session.folded.insert("P::M::outer".to_string(), true);
        let doc = session.document().expect("folded behavior");
        let outer = doc.nodes.iter().find(|n| n.key == "P::M::outer").unwrap();
        assert_eq!((outer.descendant_count, outer.hidden_count), (2, 2));
        assert!(doc.nodes.iter().any(|n| n.key == "P::M::sibling"));
        assert!(!doc.nodes.iter().any(|n| n.key.starts_with("P::M::outer::")));
    }
}

#[test]
fn explicit_expansion_cannot_restore_filtered_elements() {
    let path = fixture("fold-filter");
    std::fs::write(
        &path,
        "package P { metadata def Safety; part def A { @Safety part kept; part excluded; } }",
    )
    .expect("filtered model");
    let mut session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    session.scope = Some("P::A".to_string());
    session.folded.insert("P::A".to_string(), false);
    session.folded.insert("P::A::excluded".to_string(), false);
    let doc = session
        .document_with(Some("@Safety"))
        .expect("filtered document");
    assert!(doc.nodes.iter().any(|n| n.key == "P::A::kept"));
    assert!(!doc.nodes.iter().any(|n| n.key == "P::A::excluded"));
    let owner = doc.nodes.iter().find(|n| n.key == "P::A").unwrap();
    // The retained part and its metadata application are both model elements.
    assert_eq!((owner.descendant_count, owner.hidden_count), (2, 0));
}

#[test]
fn expose_clauses_do_not_duplicate_view_node_identities_when_selection_changes() {
    let path = fixture("view-node-identities");
    std::fs::write(
        &path,
        "package P { part def A { part child; } part def B; view architecture : GeneralView { expose A; } }",
    )
    .expect("model with declared view");
    let mut session = Session::open(vec![path], View::General, Mode::Extended).expect("session");
    for (scope, only) in [
        (None, None),
        (Some("P::A"), None),
        (None, Some("P::architecture")),
        (None, None),
    ] {
        session.scope = scope.map(str::to_string);
        session.model_view = only.map(str::to_string);
        let doc = session.document().expect("selected document");
        let identities: std::collections::BTreeSet<_> = doc.nodes.iter().map(|n| &n.key).collect();
        assert_eq!(
            identities.len(),
            doc.nodes.len(),
            "duplicate canvas identities"
        );
        assert!(!doc.nodes.iter().any(|n| n.stereotype == "expose"));
        if scope.is_some() || only.is_some() {
            assert!(doc
                .nodes
                .iter()
                .all(|n| n.key == "P::A" || n.key.starts_with("P::A::")));
        } else {
            assert!(doc.nodes.iter().any(|n| n.key == "P::architecture"));
            assert!(doc.nodes.iter().any(|n| n.key == "P::B"));
        }
    }
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
