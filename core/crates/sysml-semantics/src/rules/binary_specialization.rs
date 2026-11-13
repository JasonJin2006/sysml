//! `binary-specialization` — a link with more than two ends is not a binary link.
//!
//! `validateAssociationBinarySpecialization` and `validateConnectorBinarySpecialization`:
//! `...Ends->size() > 2 implies not specializesFromLibrary('Links::BinaryLink')`.
//!
//! `BinaryLink` is the library's two-ended link — the thing every ordinary connection is. A
//! relation with three or more ends specializes something else; claiming `BinaryLink` for it says
//! it has two ends and then gives it three.
//!
//! ```text
//! assoc Towing :> BinaryLink { end a; end b; }        // legal: two ends
//! assoc Ternary :> BinaryLink { end a; end b; end c; } // rejected: three ends, binary base
//! ```
//!
//! # How it reads
//!
//! The library is loaded, so `Links::BinaryLink` resolves, and `Index::is_subtype` walks the
//! relation's supertype closure. Ends are the identities [`Index::effective_ends_of`] reports: an
//! inherited end is an end of the subtype, and an inherited end a local redefinition replaces is
//! not. Counting only the authored children undercounts whenever a relation's ends are not all
//! written in its own body: `connection def B :> Links::BinaryLink;` writes none of its own yet has
//! the two `BinaryLink` provides, and a subtype that inherits one end while writing two has three
//! where the children say two.
//!
//! Three spellings are covered by one check: `Kind::AssocDef` is `assoc`, `Kind::ConnectionDef` is
//! `connection def`, and `Kind::AssocStructureDef` is `assoc struct` — the one identity that is both
//! an `Association` and a `Structure`. `validateAssociationBinarySpecialization` and its connector
//! twin are stated of `Association`, so the combined kind is subject too: `assoc struct T :>
//! BinaryLink { end a; end b; end c; }` claims a binary base and gives it three ends exactly as the
//! `assoc` spelling does.
//!
//! The report code this group reports under is `SEM078`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM078";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether a kind is a relation this rule is about.
fn is_link(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::AssocDef | Kind::ConnectionDef | Kind::AssocStructureDef
    )
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let Some(binary_link) = index
        .name
        .iter()
        .position(|n| n == "BinaryLink")
        .filter(|&id| index.qualified[id].ends_with("BinaryLink"))
    else {
        // No library, no `BinaryLink` to compare against.
        return Vec::new();
    };
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !is_link(&el.kind) {
            return;
        }
        // Effective ends, not authored children. The two usually agree here — `assoc T :>
        // BinaryLink { end a; end b; end c; }` writes three, and the first two redefine
        // `BinaryLink`'s `source` and `target` by position, so three are in force either way — but
        // they diverge in both directions: a subtype that inherits one end and writes two has
        // three where the children count says two, and a relation that writes no end of its own
        // (`connection def B :> Links::BinaryLink;`) inherits two where the children count says
        // zero.
        let ends = index.effective_ends_of(el.id).len();
        if ends <= 2 {
            return;
        }
        if !index.is_subtype(el.id, binary_link) {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` has {ends} ends but specializes `Links::BinaryLink`; a binary link has two \
                 (validateAssociationBinarySpecialization / \
                 validateConnectorBinarySpecialization)",
                qualified(el, index)
            ),
        ));
    });
    out
}
