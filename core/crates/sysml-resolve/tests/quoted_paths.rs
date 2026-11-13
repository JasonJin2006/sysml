//! A quoted segment of a qualified path names the same thing as the unquoted spelling.
//!
//! `Element::name` is stored unquoted, so a reference that keeps its apostrophes matched nothing.
//! Single names were unquoted when the parser read them; the segments of a `A::B` path were not.

use sysml_resolve::Resolution;

fn resolves(src: &[&str], owner: &str, reference: &str) -> bool {
    let loaded = sysml_syntax::load_project(src, true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = index
        .name
        .iter()
        .position(|n| n == owner)
        .unwrap_or_else(|| panic!("{owner} not found"));
    matches!(index.resolve(id, reference), Resolution::Found(_))
}

#[test]
fn a_quoted_segment_resolves_like_its_unquoted_spelling() {
    let a = "package '3a-Behavior' {\n\taction 'provide power';\n}\n";
    let b = "package B {\n\taction pp;\n}\n";
    assert!(
        resolves(&[a, b], "pp", "3a-Behavior::provide power"),
        "the unquoted spelling is the baseline"
    );
    assert!(
        resolves(&[a, b], "pp", "'3a-Behavior'::'provide power'"),
        "quoting a segment must not change what it names"
    );
}

#[test]
fn mixing_quoted_and_unquoted_segments_works() {
    let a = "package '3a-Behavior' {\n\taction simple;\n}\n";
    let b = "package B {\n\taction pp;\n}\n";
    assert!(resolves(&[a, b], "pp", "'3a-Behavior'::simple"));
    assert!(resolves(&[a, b], "pp", "3a-Behavior::simple"));
}

#[test]
fn a_name_that_really_is_absent_still_fails() {
    let a = "package '3a-Behavior' {\n\taction real;\n}\n";
    let b = "package B {\n\taction pp;\n}\n";
    assert!(
        !resolves(&[a, b], "pp", "'3a-Behavior'::'not there'"),
        "unquoting must not make a missing member resolve"
    );
}
