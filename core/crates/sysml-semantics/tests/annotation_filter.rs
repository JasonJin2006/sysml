//! `@Safety{level >= 2}` as a filter: which elements a narrowed view shows.
//!
//! The filter reads the annotation written on each element, not a global setting, so two parts
//! annotated with different levels are decided separately. That is the whole point of a metadata
//! annotation: the statement lives on the element it is about.

use sysml_model::{Element, Expr};
use sysml_semantics::annotated_by;

const MODEL: &str = r#"
package P {
    private import ScalarValues::*;

    metadata def Safety {
        attribute level : Integer;
    }

    @Safety { level = 3; }
    part def Brake;

    @Safety { level = 1; }
    part def Cup;

    part def Bracket;
}
"#;

/// An expression written in the filter, parsed by the model parser the same way the CLI does it.
fn constraint(text: &str) -> Expr {
    let src = format!("package __filter {{\n    attribute value = {text};\n}}\n");
    let loaded = sysml_syntax::load_project(&[&src], false).expect("expression parses");
    loaded.roots[0].children[0]
        .value_expr
        .clone()
        .expect("a value expression")
}

fn names_matching(text: Option<&str>) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[MODEL], true).expect("model parses");
    let expr = text.map(constraint);
    let kept = annotated_by(&loaded.roots, &loaded.library, "Safety", expr.as_ref());
    let mut out = Vec::new();
    fn walk(
        el: &Element,
        kept: &std::collections::HashSet<usize>,
        out: &mut Vec<String>,
        inside_annotation: bool,
    ) {
        // Annotations travel with the element they annotate, so the annotation's own id and its
        // arguments are in the set too; only the elements the filter is *about* are reported here.
        if kept.contains(&el.id) && !inside_annotation {
            out.push(el.name.clone());
        }
        for child in &el.children {
            let annotation = child.kind == sysml_model::Kind::MetadataUsage;
            walk(child, kept, out, inside_annotation || annotation);
        }
    }
    for root in &loaded.roots {
        walk(root, &kept, &mut out, false);
    }
    out
}

#[test]
fn an_annotation_name_selects_every_element_that_carries_it() {
    // The definition comes with them: it is the node they all point at.
    assert_eq!(names_matching(None), ["Safety", "Brake", "Cup"]);
}

#[test]
fn a_constraint_narrows_the_selection_to_the_arguments_it_reads() {
    // `level` is read from each annotation, not from the element: Brake's is 3 and Cup's is 1.
    assert_eq!(names_matching(Some("level >= 2")), ["Safety", "Brake"]);
    assert_eq!(names_matching(Some("level < 2")), ["Safety", "Cup"]);
}

#[test]
fn a_constraint_nobody_satisfies_selects_nothing() {
    assert!(names_matching(Some("level > 10")).is_empty());
}

#[test]
fn an_annotation_nobody_carries_selects_nothing() {
    let loaded = sysml_syntax::load_project(&[MODEL], true).expect("model parses");
    let kept = annotated_by(&loaded.roots, &loaded.library, "Availability", None);
    assert!(kept.is_empty(), "got {} element(s)", kept.len());
}

#[test]
fn a_qualified_name_matches_on_its_last_segment() {
    // A model writes `@Safety`; a filter may write `@FMEA::Safety`. They name one annotation.
    let loaded = sysml_syntax::load_project(&[MODEL], true).expect("model parses");
    let short = annotated_by(&loaded.roots, &loaded.library, "Safety", None);
    let long = annotated_by(&loaded.roots, &loaded.library, "FMEA::Safety", None);
    assert_eq!(short, long);
}
