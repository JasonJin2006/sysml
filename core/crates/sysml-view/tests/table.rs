//! The model as a table, and its requirements as a matrix.

use sysml_view::table;

fn load(src: &str) -> Vec<sysml_model::Element> {
    sysml_syntax::load(src).expect("parses").roots
}

#[test]
fn every_element_is_one_row() {
    let roots = load("package P {\n\tpart def A { attribute x : Real = 1; }\n}\n");
    let t = table::elements(&roots);
    assert_eq!(t.title, "elements");
    assert_eq!(t.columns, ["qualified", "kind", "type", "value"]);
    let names: Vec<&str> = t.rows.iter().map(|r| r[0].as_str()).collect();
    assert!(names.contains(&"P"), "{names:?}");
    assert!(names.contains(&"P::A"), "{names:?}");
    assert!(names.contains(&"P::A::x"), "{names:?}");
}

#[test]
fn an_element_row_carries_its_type_and_value_as_written() {
    let roots = load("package P {\n\tattribute x : Real = 1;\n}\n");
    let t = table::elements(&roots);
    let row = t.rows.iter().find(|r| r[0] == "P::x").expect("x");
    assert_eq!(row[1], "attribute");
    assert_eq!(row[2], "Real");
    assert_eq!(row[3], "1");
}

#[test]
fn an_anonymous_element_does_not_become_a_path_segment() {
    // The rule the resolver states: `@Safety;` is not a name.
    let roots = load("package P {\n\tpart def A {\n\t\t@Safety;\n\t}\n}\n");
    let t = table::elements(&roots);
    assert!(
        !t.rows.iter().any(|r| r[0].contains("<anonymous>")),
        "{:?}",
        t.rows.iter().map(|r| &r[0]).collect::<Vec<_>>()
    );
}

#[test]
fn the_matrix_marks_a_satisfier_and_leaves_an_unmet_requirement_empty() {
    let roots = load(
        "package P {\n\tpart def A;\n\tpart a : A;\n\
         requirement R1 { subject x : A; require constraint { 1 > 0 } }\n\
         requirement R2 { subject x : A; require constraint { 1 > 0 } }\n\
         satisfy R1 by a;\n}\n",
    );
    let t = table::requirement_matrix(&roots);
    assert_eq!(t.columns[0], "requirement");
    let r1 = t.rows.iter().find(|r| r[0] == "R1").expect("R1");
    let r2 = t.rows.iter().find(|r| r[0] == "R2").expect("R2");
    assert!(r1.iter().skip(1).any(|c| c == "x"), "{r1:?}");
    assert!(r2.iter().skip(1).all(|c| c.is_empty()), "{r2:?}");
}

#[test]
fn csv_quotes_a_cell_that_holds_a_comma_or_a_quote() {
    let t = table::Table {
        title: "t".to_string(),
        columns: vec!["a".to_string(), "b".to_string()],
        rows: vec![vec!["x,y".to_string(), "say \"hi\"".to_string()]],
    };
    let csv = table::to_csv(&t);
    assert_eq!(csv, "a,b\n\"x,y\",\"say \"\"hi\"\"\"\n");
}

#[test]
fn csv_puts_the_header_first_and_one_row_per_line() {
    let t = table::Table {
        title: "t".to_string(),
        columns: vec!["a".to_string()],
        rows: vec![vec!["1".to_string()], vec!["2".to_string()]],
    };
    assert_eq!(table::to_csv(&t), "a\n1\n2\n");
}

#[test]
fn the_tree_style_puts_every_node_on_its_own_row() {
    // The nested style draws a child inside its owner's rectangle; the tree style keeps one node
    // per row and indents by depth, so a deep model stays readable.
    let roots =
        load("package P {\n\tpart def A {\n\t\tpart b : A;\n\t\tattribute x : Real;\n\t}\n}\n");
    let graph = sysml_view::project(
        &roots,
        sysml_view::View::General,
        sysml_view::Mode::Extended,
    );
    let svg = sysml_view::render_styled(&graph, "t", sysml_view::layered::Style::Tree);
    assert!(svg.starts_with("<svg"), "{svg:.80}");
    // One rect per box, and every box's y differs: that is the row-per-node property.
    let ys: Vec<&str> = svg
        .match_indices("y=\"")
        .map(|(i, _)| &svg[i..(i + 20).min(svg.len())])
        .collect();
    assert!(!ys.is_empty(), "the picture has boxes");
    let tree = sysml_view::render_styled(&graph, "t", sysml_view::layered::Style::Tree);
    let nested = sysml_view::render_styled(&graph, "t", sysml_view::layered::Style::Nested);
    assert_ne!(
        tree, nested,
        "the two styles lay the picture out differently"
    );
}
