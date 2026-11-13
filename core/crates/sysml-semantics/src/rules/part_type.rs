//! `part-type` — a part's type must be an occurrence definition.
//!
//! `validatePartUsagePartDefinition`: `partDefinition->notEmpty()` — at least one of the types a
//! part names must be a `PartDefinition`.
//!
//! ```text
//! part def Wheel;
//! part def Car {
//!     part wheels : Wheel;   // legal: a part definition
//!     part spare;            // legal: untyped, and the pilot implementation agrees
//!     part mass : Real;      // rejected: a datatype is not a part
//! }
//! ```
//!
//! # What the pilot implementation settled
//!
//! This rule was written once and withdrawn: it reported 98 violations across the official
//! validation models, all of them conforming, and it was assumed the OCL was stricter than the
//! notation. The OMG pilot implementation's own test models say otherwise —
//! `PartUsage_invalid.sysml.xt` marks `part p1: Real;`, `part p2: att;`, `part p3: act;` and
//! `part p4: AttDef;` as errors, and leaves `part aPart;` (untyped) alone in the same file.
//!
//! So the rule is about a part that *names* a type the part cannot be, not about one that names
//! none. Typing it in was the mistake; the OCL reading was right.
//!
//! # How it reads
//!
//! The type resolves, and its kind is asked. A part definition passes. An unresolved type is
//! skipped — a dangling name is a reference problem. The kinds accepted are the ones the
//! specification's occurrence hierarchy gives a part: a part, item or occurrence definition, or a
//! definition that specializes one.
//!
//! The report code this group reports under is `SEM079`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM079";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Whether a kind is an occurrence definition — something a part can be typed by.
///
/// `PartDefinition`, `ItemDefinition` and `OccurrenceDefinition` all sit under
/// `Occurrences::Occurrence` in the library; the tree keeps them apart because rules ask about
/// them separately, so they are listed here.
fn is_occurrence_definition(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::PartDef
            | Kind::ItemDef
            | Kind::OccurrenceDef
            | Kind::PortDef
            | Kind::ConnectionDef
            | Kind::InterfaceDef
            | Kind::AllocationDef
            | Kind::Flow
            | Kind::ActionDef
            | Kind::StateDef
            | Kind::ConstraintDef
            | Kind::CalcDef
            | Kind::CaseDef
            | Kind::AnalysisCaseDef
            | Kind::VerificationDef
            | Kind::UseCaseDef
            | Kind::RequirementDef
            | Kind::ConcernDef
            | Kind::ViewDef
            | Kind::ViewpointDef
            | Kind::MetadataDef
    )
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::PartUsage {
            return;
        }
        // Untyped is legal: the pilot implementation leaves `part aPart;` alone.
        for text in &el.type_refs {
            let head = text.split(',').next().unwrap_or(text).trim();
            let Resolution::Found(target) = index.resolve(el.id, head) else {
                continue;
            };
            if is_occurrence_definition(&index.kind[target]) {
                continue;
            }
            let target_kind = index.kind[target].keyword();
            let target_name = index
                .qualified
                .get(target)
                .cloned()
                .filter(|q| !q.is_empty())
                .unwrap_or_else(|| head.to_string());
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` is a part typed by `{target_name}`, a {target_kind}; a part must be typed \
                     by an occurrence definition (validatePartUsagePartDefinition)",
                    qualified(el, index)
                ),
            ));
        }
    });
    out
}
