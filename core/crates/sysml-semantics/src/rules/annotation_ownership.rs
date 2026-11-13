//! `annotation-ownership` — an annotation that its annotated element owns must own an annotating
//! element.
//!
//! `validateAnnotationAnnotatedElementOwnership`: `(owningAnnotatedElement <> null) =
//! (ownedAnnotatingElement <> null)`. `owningAnnotatedElement` is the owner of the annotation when
//! that owner is also what the annotation says it is about (`subsets annotatedElement,
//! owningRelatedElement`, `KerML.kerml:6623`). An annotation written `comment c about c` is owned by
//! `c` and is about `c`, and owns no annotating element, so it is reported.
//!
//! ```text
//! package P {
//!     comment c about c;     // rejected: the comment is about itself and owns nothing
//!     comment d about P::d;  // rejected too: the qualified name names the same element
//!     feature x;
//!     comment e about x;     // accepted: about another element
//! }
//! ```
//!
//! The kernel does not model the annotation relationship as an element of its own. A named comment
//! with an `about` clause is the element, and its targets are its `to` text (`lower.rs`), so the
//! annotation is owned by the element it is attached to. The reference reports the violation on the
//! annotation with the message "Must own its annotating element" (`KerMLValidator.xtend:411-416`).
//!
//! The report code is `SEM121`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM121";

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !matches!(index.kind[el.id], Kind::Comment | Kind::MetadataUsage) {
            return;
        }
        let Some(targets) = index.end_text(el.id, false) else {
            return;
        };
        // The targets are written in the scope that holds the comment, so a bare name resolves to
        // the comment itself when it names it.
        let scope = index.clause_scope_of(el.id);
        for target in targets.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            if let Resolution::Found(annotated) = index.resolve(scope, target) {
                if annotated == el.id {
                    out.push(Violation::new(
                        CODE,
                        el,
                        index.qualified[el.id].clone(),
                        "annotates itself, and an annotation owned by its annotated element must own an annotating element",
                    ));
                }
            }
        }
    });
    out
}
