//! A filtered view: what the filter keeps, and what it keeps so the picture stays connected.
//!
//! The filter decides which elements a view is *about*. An enclosing package stays even when it
//! carries no annotation, because a node with no parent in the picture has nowhere to be drawn; a
//! sibling that carries no annotation is gone.

use std::collections::HashSet;

use sysml_model::Element;
use sysml_syntax::load;

const MODEL: &str = r#"package P {
    metadata def Safety { attribute level : Integer; }

    @Safety { level = 3; }
    part def Brake {
        part pad;
    }

    part def Bracket;
}
"#;

fn name_of(roots: &[Element], id: usize) -> Option<String> {
    fn walk(el: &Element, id: usize, out: &mut Option<String>) {
        if el.id == id {
            *out = Some(el.name.clone());
        }
        for child in &el.children {
            walk(child, id, out);
        }
    }
    let mut out = None;
    for root in roots {
        walk(root, id, &mut out);
    }
    out
}

fn id_of(roots: &[Element], name: &str) -> usize {
    fn walk(el: &Element, name: &str, out: &mut Option<usize>) {
        if el.name == name {
            *out = Some(el.id);
        }
        for child in &el.children {
            walk(child, name, out);
        }
    }
    let mut out = None;
    for root in roots {
        walk(root, name, &mut out);
    }
    out.unwrap_or_else(|| panic!("no element called {name}"))
}

#[test]
fn a_filter_keeps_the_element_it_named_and_the_path_to_it() {
    let loaded = load(MODEL).expect("model parses");
    let mut keep = HashSet::new();
    keep.insert(id_of(&loaded.roots, "Brake"));

    // The tree is where containment is decided, so the path is checked there: a kept element that
    // lost its package would have nowhere to be drawn in a view that draws packages.
    let pruned = sysml_view::keep_only(&loaded.roots, &keep);
    assert_eq!(pruned.len(), 1, "the package stays");
    let names: Vec<&str> = pruned[0].children.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Brake"), "{names:?}");
    assert!(
        !names.contains(&"Bracket"),
        "a sibling with no annotation is gone: {names:?}"
    );
    // The contents of Brake are not kept: the filter says which elements the view is *about*, not
    // what they contain.
    let brake = pruned[0]
        .children
        .iter()
        .find(|c| c.name == "Brake")
        .expect("Brake");
    assert!(
        !brake.children.iter().any(|c| c.name == "pad"),
        "{:?}",
        brake.children.len()
    );

    let graph = sysml_view::project_filtered(
        &loaded.roots,
        sysml_view::View::General,
        sysml_view::Mode::Extended,
        &keep,
    );
    // Pruning renumbers, because the resolver indexes by id and a sparse tree reads past the end.
    let drawn: Vec<&str> = graph.nodes.iter().map(|node| node.name.as_str()).collect();
    assert_eq!(drawn, ["Brake"]);
}

#[test]
fn an_empty_filter_keeps_nothing() {
    let loaded = load(MODEL).expect("model parses");
    let graph = sysml_view::project_filtered(
        &loaded.roots,
        sysml_view::View::General,
        sysml_view::Mode::Extended,
        &HashSet::new(),
    );
    assert!(graph.nodes.is_empty(), "got {} node(s)", graph.nodes.len());
}
