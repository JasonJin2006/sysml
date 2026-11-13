//! `sysml units` — the quantity of each value, and which units are not checked.
//!
//! The unit table lives inside the evaluator, so "what kind of quantity is this value" was a
//! question only the evaluator could answer, and only internally. A report needs it without
//! re-deriving the table, and a unit outside the table has to be distinguishable from a value
//! that states no unit at all: the first means comparisons involving it stay undecided, the
//! second is a bare number that compares with anything.

use std::process::Command;

const MODEL: &str = r#"package P {
	private import ScalarValues::*;
	part def S {
		attribute mass : Real = 1200 [kg];
		attribute speed : Real = 30 [m/s];
		attribute count : Real = 5;
		attribute odd : Real = 3 [furlong];
	}
}
"#;

struct Run {
    dir: std::path::PathBuf,
    model: std::path::PathBuf,
}

impl Run {
    fn new(tag: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("sysml-units-{tag}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let model = dir.join("m.sysml");
        std::fs::write(&model, MODEL).expect("write model");
        Run { dir, model }
    }

    fn json(&self) -> serde_json::Value {
        let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
            .arg("units")
            .arg(&self.model)
            .arg("--json")
            .output()
            .expect("the binary runs");
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
    }

    fn text(&self) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
            .arg("units")
            .arg(&self.model)
            .output()
            .expect("the binary runs");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn row<'a>(doc: &'a serde_json::Value, element: &str) -> &'a serde_json::Value {
    doc["values"]
        .as_array()
        .expect("a values array")
        .iter()
        .find(|row| row["element"] == element)
        .unwrap_or_else(|| panic!("no row for {element}"))
}

#[test]
fn a_value_reports_what_kind_of_quantity_it_is() {
    let run = Run::new("dimension");
    let doc = run.json();
    assert_eq!(row(&doc, "P::S::mass")["dimension"], "mass");
    assert_eq!(row(&doc, "P::S::speed")["dimension"], "length/time");
    assert_eq!(row(&doc, "P::S::mass")["unit"], "kg");
    assert_eq!(row(&doc, "P::S::mass")["checked"], true);
}

/// A value with no unit is not "unchecked": it is a bare number, and bare numbers compare.
#[test]
fn a_value_with_no_unit_is_neither_dimensioned_nor_unchecked() {
    let run = Run::new("bare");
    let doc = run.json();
    let count = row(&doc, "P::S::count");
    assert_eq!(count["unit"], "", "no unit written");
    assert_eq!(count["dimension"], serde_json::Value::Null);
}

/// A unit outside the table is reported as unchecked, not as dimensionless.
///
/// The two read the same in a schema that only carries a dimension string, and they mean opposite
/// things: an unchecked unit leaves every comparison involving it undecided, while a bare number
/// is comparable with anything.
#[test]
fn a_unit_outside_the_table_is_marked_unchecked() {
    let run = Run::new("unknown");
    let doc = run.json();
    let odd = row(&doc, "P::S::odd");
    assert_eq!(odd["unit"], "furlong", "the authored spelling is kept");
    assert_eq!(odd["dimension"], serde_json::Value::Null);
    assert_eq!(odd["checked"], false);
    assert_eq!(doc["unchecked"], 1, "and it is counted");
    assert!(
        run.text().contains("not in the table"),
        "the printed form says so rather than leaving a blank: {}",
        run.text()
    );
}
