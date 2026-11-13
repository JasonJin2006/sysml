//! A `private` or `protected` member is not a name the rest of the model can use.
//!
//! `assign i.b := null;` with `protected attribute b;` inside `item i` is "Couldn't resolve
//! reference to Element 'b'" in the reference (`AssignmentActionUsage_invalid.sysml.xt:41`), and
//! the `.` walk is where that is decided: `i.b` steps into `i` and asks for `b`, which `i` keeps
//! to itself.

use sysml_resolve::{Index, Resolution};

fn index(src: &str) -> Index {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    Index::build(&loaded.roots).with_library(&loaded.library)
}

const MODEL: &str = r#"
package P {
	item i {
		protected attribute b;
		private attribute c;
		attribute a;
	}
	action def A {
		assign i.b := null;
	}
}
"#;

/// The `assign` statement, which is the element the `.` path is written in.
fn assign_scope(index: &Index) -> usize {
    (0..index.qualified.len())
        .find(|&id| index.kind[id] == sysml_model::Kind::AssignmentActionUsage)
        .expect("the assignment")
}

#[test]
fn a_protected_member_is_not_reachable_with_a_dot() {
    let index = index(MODEL);
    let from = assign_scope(&index);
    let path = sysml_model::NameRef::new("i.b", index.line[from]);
    assert!(
        matches!(index.resolve_path(from, &path), Resolution::Unresolved),
        "got {:?}",
        index.resolve_path(from, &path)
    );
}

#[test]
fn a_public_member_is_reachable_with_a_dot() {
    let index = index(MODEL);
    let from = assign_scope(&index);
    let path = sysml_model::NameRef::new("i.a", index.line[from]);
    assert!(
        matches!(index.resolve_path(from, &path), Resolution::Found(_)),
        "got {:?}",
        index.resolve_path(from, &path)
    );
}
