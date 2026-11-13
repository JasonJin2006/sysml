//! `library-package` — a user's library is not the standard one.
//!
//! ```text
//! standard library package Mine { }   // warned: the standard libraries are the specification's
//! library package Mine { }            // legal: a library the model itself provides
//! ```
//!
//! The reference states it as a warning at the word `standard`:
//! `LibraryPackage_invalid_notStandard.kerml.xt:15`. Nothing in the specification's 180 says it —
//! it is the Xtext validator's own advice — so this module is not counted in `conformance`, like
//! `typed_by_definition`.
//!
//! # How it reads
//!
//! `standard` is not part of the kind: `library package` and `standard library package` both lower
//! to [`Kind::Package`] (`lower.rs:225`), and the parser used to drop the word entirely. It now records it in
//! `Element::is_standard`, which is the whole of what this rule reads.
//!
//! The standard library itself is written `standard library package` in 95 files, and every one of
//! them is loaded as a library root rather than a model — `check` walks `roots` alone, so the rule
//! never sees them. A user model that marks its own library standard is what it is for.
//!
//! The report code this group reports under is `SEM097`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM097";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::Package || !el.is_standard {
            return;
        }
        out.push(Violation::warning(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is written `standard library package`; the standard libraries are the \
                 specification's, and a model's own library is not one of them",
                qualified(el, index)
            ),
        ));
    });
    out
}
