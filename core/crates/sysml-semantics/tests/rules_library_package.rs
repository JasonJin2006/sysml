//! `library-package` — a user's library is not the standard one.

use sysml_semantics::{check, Severity};

fn sem097(src: &str) -> Vec<(String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.code == "SEM097")
        .map(|i| (i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_user_library_marked_standard_is_warned_about() {
    // `LibraryPackage_invalid_notStandard.kerml.xt:15`: the warning lands on the declaration, and
    // the word `standard` is the whole of the evidence.
    let src = "standard library package Mine {\n\tfeature x;\n}\n";
    let found = sem097(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].1.contains("standard library package"),
        "got {found:?}"
    );
}

#[test]
fn a_plain_library_package_is_accepted() {
    let src = "library package Mine {\n\tfeature x;\n}\n";
    assert!(sem097(src).is_empty(), "got {:?}", sem097(src));
}

#[test]
fn the_warning_is_a_warning() {
    // The reference states it as one, and a model that marks its own library standard is
    // questionable rather than wrong.
    let loaded = sysml_syntax::load_project(&["standard library package M { feature x; }"], true)
        .expect("model parses");
    let report = check(&loaded.roots, &loaded.library);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.code == "SEM097" && i.severity == Severity::Warning),
        "got {:?}",
        report
            .issues
            .iter()
            .map(|i| (i.code, i.severity))
            .collect::<Vec<_>>()
    );
}
