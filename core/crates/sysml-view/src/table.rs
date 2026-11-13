//! A model as a table, and its requirements as a matrix.
//!
//! The names are the deliverable: Syside calls this a Grid view and builds it from its own
//! `TableView`/`MatrixView` library. Those types are not in the SysML specification or in the
//! standard library — `Views::View` is there, `TableView` is not — so this is not a standard view
//! in the sense the others are. It is the same idea applied to the tree this kernel already has:
//! a flat table of elements, and a matrix of which element satisfies which requirement.
//!
//! Both are plain data, not a `Graph`. A table has no edges, and forcing one through the node and
//! edge model would have the canvas lay out rows as boxes.

use sysml_model::Element;

use crate::trace;

/// A table: column headers and rows of cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// What the table is, in one line — `elements`, `requirements by satisfier`.
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

/// Every element, one row each: qualified name, kind, type, value.
///
/// A value is what the model writes after `=`; a type is what it writes after `:`. Both are kept
/// as written rather than resolved, because a table is read against the source.
pub fn elements(roots: &[Element]) -> Table {
    let mut rows = Vec::new();
    fn walk(el: &Element, path: &mut Vec<String>, rows: &mut Vec<Vec<String>>) {
        let anonymous = el.name == "<anonymous>";
        if !anonymous {
            path.push(el.name.clone());
        }
        rows.push(vec![
            path.join("::"),
            el.kind.keyword_as(el.is_definition).to_string(),
            el.type_ref.clone().unwrap_or_default(),
            el.value_text.clone().unwrap_or_default(),
        ]);
        for child in &el.children {
            walk(child, path, rows);
        }
        if !anonymous {
            path.pop();
        }
    }
    for root in roots {
        walk(root, &mut Vec::new(), &mut rows);
    }
    Table {
        title: "elements".to_string(),
        columns: ["qualified", "kind", "type", "value"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        rows,
    }
}

/// Requirements as rows and the elements that satisfy them as columns.
///
/// The cells are `x` where the model states a `satisfy`, empty where it does not. A requirement
/// nothing satisfies is a row of empty cells, which is what the reader is looking for.
pub fn requirement_matrix(roots: &[Element]) -> Table {
    let traced = trace::traceability(roots);
    let graph = &traced.graph;
    let satisfiers: Vec<usize> = {
        let mut out: Vec<usize> = Vec::new();
        for t in &traced.traces {
            for &node in &t.satisfied_by {
                if !out.contains(&node) {
                    out.push(node);
                }
            }
        }
        out
    };
    let mut columns = vec!["requirement".to_string()];
    columns.extend(satisfiers.iter().map(|&n| graph.nodes[n].name.clone()));

    let mut rows = Vec::new();
    for t in &traced.traces {
        let mut row = vec![graph.nodes[t.node].name.clone()];
        for &node in &satisfiers {
            row.push(if t.satisfied_by.contains(&node) {
                "x".to_string()
            } else {
                String::new()
            });
        }
        rows.push(row);
    }
    Table {
        title: "requirements by satisfier".to_string(),
        columns,
        rows,
    }
}

/// A table as CSV, quoting cells that hold a comma, a quote or a newline.
pub fn to_csv(table: &Table) -> String {
    fn cell(text: &str) -> String {
        if text.contains(',') || text.contains('"') || text.contains('\n') {
            format!("\"{}\"", text.replace('"', "\"\""))
        } else {
            text.to_string()
        }
    }
    let mut out = String::new();
    out.push_str(
        &table
            .columns
            .iter()
            .map(|c| cell(c))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in &table.rows {
        out.push_str(&row.iter().map(|c| cell(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}
