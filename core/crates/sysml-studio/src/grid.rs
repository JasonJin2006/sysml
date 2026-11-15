use serde::Serialize;
use sysml_model::Element;
use sysml_resolve::Index;
use sysml_view::table::{self, Table};

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Elements,
    Matrix,
}

impl Kind {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "elements" => Ok(Self::Elements),
            "matrix" => Ok(Self::Matrix),
            _ => Err("Unknown grid kind. Use `elements` or `matrix`.".to_string()),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Grid {
    pub title: &'static str,
    pub description: &'static str,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_keys: Vec<String>,
    pub csv: String,
}

pub(crate) fn build(roots: &[Element], kind: Kind) -> Grid {
    let (title, description, table, row_keys) = match kind {
        Kind::Elements => {
            let table = table::elements(roots);
            let keys = table.rows.iter().map(|row| row[0].clone()).collect();
            (
                "Model elements",
                "Read-only element table. Types and values are shown as written in the source model.",
                table,
                keys,
            )
        }
        Kind::Matrix => {
            let table = table::requirement_matrix(roots);
            let traced = sysml_view::trace::traceability(roots);
            let index = Index::build(roots);
            let keys = traced
                .traces
                .iter()
                .map(|trace| {
                    let element = traced.graph.nodes[trace.node]
                        .element
                        .expect("a requirement node has a source element");
                    index.qualified[element].clone()
                })
                .collect();
            (
                "Requirement satisfaction matrix",
                "Read-only satisfaction matrix. Columns include only elements named by existing satisfaction relationships, not all possible implementers. An empty cell means no satisfaction relationship is stated; it is not a failed verification result.",
                table,
                keys,
            )
        }
    };
    let csv = table::to_csv(&table);
    let Table { columns, rows, .. } = table;
    Grid {
        title,
        description,
        columns,
        rows,
        row_keys,
        csv,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_preserves_unmet_requirements_and_qualified_source_keys() {
        let loaded = sysml_syntax::load(include_str!(
            "../../../examples/showcase/traceability.sysml"
        ))
        .expect("showcase model parses");
        let grid = build(&loaded.roots, Kind::Matrix);
        assert_eq!(grid.rows.len(), 6);
        assert_eq!(grid.row_keys.len(), grid.rows.len());
        assert!(grid
            .row_keys
            .iter()
            .all(|key| key.starts_with("MaritimeAssurance::")));
        let unmet = grid
            .row_keys
            .iter()
            .position(|key| key == "MaritimeAssurance::IceOperations")
            .expect("unmet requirement remains a row");
        assert!(grid.rows[unmet].iter().skip(1).all(String::is_empty));
        assert_eq!(grid.rows[unmet][0], "IceOperations");
        assert!(grid.description.starts_with("Read-only"));
        assert!(grid.description.contains("only elements named by existing"));
        assert!(grid.description.contains("not a failed verification"));
        assert_eq!(
            grid.csv,
            table::to_csv(&table::requirement_matrix(&loaded.roots))
        );
    }

    #[test]
    fn equal_requirement_labels_in_different_namespaces_keep_distinct_keys() {
        let loaded =
            sysml_syntax::load("package A { requirement def R; } package B { requirement def R; }")
                .expect("parses");
        let grid = build(&loaded.roots, Kind::Matrix);
        assert_eq!(grid.row_keys, ["A::R", "B::R"]);
        assert_eq!(grid.rows.len(), 2);
        assert!(grid.rows.iter().all(|row| row == &["R"]));
    }

    #[test]
    fn element_rows_keep_the_same_keys_and_csv_quoting_as_the_cli() {
        let loaded = sysml_syntax::load(
            "package P { part def A { attribute label = \"survey, \\\"north\\\"\"; } }",
        )
        .expect("parses");
        let grid = build(&loaded.roots, Kind::Elements);
        assert_eq!(grid.title, "Model elements");
        assert_eq!(grid.rows.len(), grid.row_keys.len());
        for (row, key) in grid.rows.iter().zip(&grid.row_keys) {
            assert_eq!(&row[0], key);
        }
        assert_eq!(grid.csv, table::to_csv(&table::elements(&loaded.roots)));
        assert!(grid.csv.contains("survey,"));
        assert!(grid.csv.contains("\"\""), "CSV quotes are escaped");
        assert!(grid.description.starts_with("Read-only"));
    }

    #[test]
    fn unknown_grid_kind_is_rejected_without_guessing_a_projection() {
        assert!(Kind::parse("elements").is_ok());
        assert!(Kind::parse("matrix").is_ok());
        assert!(Kind::parse("GridView").is_err());
        assert!(Kind::parse("").is_err());
    }
}
