//! `property-type` — a metamodel property's declared type and multiplicity, checked against the
//! values it has.
//!
//! A few of the reference implementation's errors are not constraints but property types:
//!
//! ```text
//! abstract connection def B :> Links::BinaryLink;   // rejected: BinaryLink's ends are Features
//! interface def I { end part notPort : Fuel; }      // rejected: a part is not a PortUsage
//! connection { end ::> b0; }                        // rejected: one related element, not two
//! ```
//!
//! `ConnectionDefinition::connectionEnd` is declared `Usage[*]`, `FlowDefinition::flowEnd`
//! `Usage[*]`, `InterfaceDefinition::interfaceEnd` `PortUsage[*]`
//! (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:2094`, `:2293`, `:2341`), and
//! `Relationship::relatedElement` is `Element[2..*]` (`:2495`). The table lives in
//! [`crate::conformance::PROPERTY_CHECKS`] rather than in [`crate::conformance::RULES`], because
//! none of the four is one of the specification's 180 constraints — the reference states them by the
//! property's own declaration, and its messages say so in words: "A connection definition end must
//! be a usage.", "Must have at least two related elements."
//!
//! # Which ends the property has
//!
//! The ends the element writes plus the ones it inherits through its supertype closure. The
//! library is **not** skipped here, unlike every other rule about ends: `Links::BinaryLink`
//! declares `end feature source` and `end feature target`, and those two KerML features are
//! exactly what a connection definition inheriting it ends up with. Skipping the library would
//! leave the rule with nothing to reject in the model that states it
//! (`ConnectionUsage_End_Invalid.sysml.xt:33`).
//!
//! # An interface usage is held to the same type
//!
//! The metamodel states `interfaceEnd : PortUsage[*]` on `InterfaceDefinition` only — an
//! `InterfaceUsage` inherits `connectionEnd : Usage[*]` from `ConnectionUsage`
//! (`generated.rs:1687`, `:2341-2345`) — and the reference reports the stricter type for both:
//! `interface bad { end part ::> tankAssy.fuel; }` inside a part is an error in
//! `InterfaceUsage_Invalid.sysml.xt:79`, under the message "An interface end must be a port."
//!
//! One entry covers both here because the rule asks the metaclass hierarchy, and `InterfaceUsage`
//! inherits from `ConnectionUsage`, which reaches the same check. The projection keeps the two
//! apart (`Kind::InterfaceDef` and `Kind::InterfaceUsage`), so the entry is keyed on the
//! `InterfaceDefinition` property and the usage reaches it through inheritance rather than through
//! a second entry.
//!
//! # The count, and what is not abstract
//!
//! `PropertyConstraint::AtLeast` asks how many values the property has rather than what they are.
//! An abstract connector may be incomplete: the pilot writes `connection { end ::> b0; }` as an
//! error and `abstract connection { end ::> b0; }` beside it as nothing
//! (`Relationship_invalid_relatedElement0.sysml.xt:32-35`).
//!
//! The effective end set removes inherited identities replaced by local redefinitions. This also
//! applies to differently named and anonymous positional ends: retaining the library ends would
//! turn a valid two-end binary interface into a four-end interface.
//!
//! # Effective ends are the authoritative count
//!
//! A shorthand connector or interface usage — `connect a to b;`, `interface i : I connect x to
//! y;` — lowers to two anonymous child ends whose `references` name the authored endpoints
//! (`sysml-syntax/src/lower.rs`, `project_connector_ends`). Those children *are* the
//! relationship's ends, so when the effective end set is non-empty it is the whole count.
//! Adding `from` / `to` beside them states each authored endpoint twice: a valid two-ended
//! `interface joined : Base connect left to right;` would count four, and a one-ended form would
//! satisfy a two-ended check. `from` / `to` are therefore read only for a form that projects no
//! end children at all — a `flow a to b`, whose endpoints are authored text rather than features.
//!
//! The distinct-id requirement is not decoration: the two projected children must stay separate
//! identities so positional redefinition can replace one inherited end without coalescing the
//! other, and so the count can never fall back to deduplicating by endpoint name.
//!
//! The report code this group reports under is `SEM096`.

use sysml_model::Element;
use sysml_resolve::Index;

use crate::conformance::{PropertyConstraint, PROPERTY_CHECKS};

use super::Violation;

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Use the same end identities as positional redefinition and derived typing. A local end replaces
/// its inherited target even when the two declarations have different names or are anonymous.
fn ends_of(index: &Index, id: usize) -> Vec<usize> {
    index.effective_ends_of(id)
}

/// How many values the relationship's end property holds, for the `AtLeast` question.
///
/// The effective owns are authoritative. A shorthand connector lowers its two endpoints into two
/// anonymous end children (`project_connector_ends`), so counting `from` / `to` as well would
/// count the same authored endpoint twice. Only a form that projects no ends at all — a `flow a to
/// b`, whose ends are authored text and not features — falls back to the endpoint fields.
///
/// The count is not capped and not deduplicated by endpoint text: two projected ends that name the
/// same feature are still two related features, and the pilot's "at least two" is a count of
/// values, not of distinct names.
fn stated_ends(index: &Index, el: &Element) -> usize {
    let ends = ends_of(index, el.id);
    if !ends.is_empty() {
        return ends.len();
    }
    usize::from(el.from.as_deref().is_some_and(|f| !f.trim().is_empty()))
        + usize::from(el.to.as_deref().is_some_and(|t| !t.trim().is_empty()))
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    for check in PROPERTY_CHECKS {
        super::walk(roots, |el| {
            if !index.is_kind_of(el.id, check.owner)
                && !check
                    .also_owner
                    .is_some_and(|owner| index.is_kind_of(el.id, owner))
            {
                return;
            }
            // See `PropertyCheck::requires_no_own_ends`: a definition that writes ends of its own
            // is the shape the corpus leaves clean while the same definition without them is the
            // shape the pilot rejects.
            if check.requires_no_own_ends
                && index
                    .children
                    .get(el.id)
                    .is_some_and(|kids| kids.iter().any(|&c| index.is_end_of(c)))
            {
                return;
            }
            let declared = match check.constraint {
                PropertyConstraint::Each(declared) => declared,
                PropertyConstraint::AtLeast(min) => {
                    // An abstract connector is allowed to be incomplete.
                    if el.modifiers.is_abstract {
                        return;
                    }
                    // The projected/effective ends are the count; `from` / `to` is a fallback for
                    // forms that project none. See `stated_ends`.
                    let stated = stated_ends(index, el);
                    if stated >= min {
                        return;
                    }
                    // An anonymous connector takes its owner's qualified name, so quoting it reads
                    // as "the part `v` states 1 related element" when `v` is the enclosing usage
                    // and states no related element at all. The kind is the part worth printing;
                    // the `end` branch below does the same for the same reason.
                    let name = qualified(el, index);
                    let subject = if name.is_empty() || !index.has_authored_name(el.id) {
                        format!("a {}", el.kind.keyword())
                    } else {
                        format!("`{name}`")
                    };
                    out.push(Violation::new(
                        check.code,
                        el,
                        qualified(el, index),
                        format!("{subject} states {stated} {}; {}", check.noun, check.says),
                    ));
                    return;
                }
                PropertyConstraint::AtMost(max) => {
                    let ends = ends_of(index, el.id);
                    if ends.len() <= max {
                        return;
                    }
                    // The one past the limit is the one to report, on its own line when it is
                    // written here: `end part notPort : Fuel;` is the third end of an interface
                    // and the assertion points at it, not at the definition
                    // (`InterfaceUsage_Invalid.sysml.xt:53`).
                    for &end in ends.iter().skip(max) {
                        let written = index.parent.get(end).copied().flatten() == Some(el.id);
                        let element = if written {
                            el.children.iter().find(|c| c.id == end).unwrap_or(el)
                        } else {
                            el
                        };
                        out.push(Violation::new(
                            check.code,
                            element,
                            qualified(element, index),
                            format!(
                                "`{}` is a further {} of `{}`; {}",
                                qualified(element, index),
                                check.noun,
                                qualified(el, index),
                                check.says
                            ),
                        ));
                    }
                    return;
                }
            };
            // Keyed by the line the report will land on, because the pilot states one assertion
            // per line and a second report on a line that already has one counts as a false
            // positive: a connection definition inheriting `BinaryLink` has two wrong ends and
            // exactly one expected error.
            let mut by_line: std::collections::BTreeMap<u32, Vec<(usize, bool)>> =
                std::collections::BTreeMap::new();
            // A definition that writes no end of its own holds every end it inherits, and the
            // library's own redefinitions do not take one away from it: `Transfer` redefines
            // `BinaryLink::source`, and `Flows::Flow` redefines `Transfer`'s, so the *effective*
            // set of `abstract flow def F :> Links::BinaryLink;` keeps only the usage ends and
            // drops the two KerML features the definition also inherits — the very ends
            // `FlowDefinition::flowEnd` is declared `Usage` for (`FlowUsage_End_Invalid.sysml.xt:33`).
            // Each supertype's own effective ends put them back. Only the checks that judge
            // inherited ends widen this way: a definition that writes ends of its own is judged by
            // `ends_of` alone, so the identity replacement a local end makes still counts once.
            let mut judged = ends_of(index, el.id);
            if check.requires_no_own_ends {
                for general in index.supertypes_of_public(el.id) {
                    for end in ends_of(index, general) {
                        if !judged.contains(&end) {
                            judged.push(end);
                        }
                    }
                }
            }
            for end in judged {
                if index.is_kind_of(end, declared) {
                    continue;
                }
                // Written here, the report goes on the end's own line; inherited, it goes on the
                // declaration, because the end's line is in the library and not in this file.
                // `InterfaceUsage_Invalid.sysml.xt` states both shapes: `end part notPort : Fuel;`
                // is asserted on the `end` line, and `abstract connection def B :>
                // Links::BinaryLink;` on the declaration. Reporting the first shape on the
                // declaration is a miss at the assertion and a false positive where it was made.
                let written = index.parent.get(end).copied().flatten() == Some(el.id);
                let line = if written {
                    index.line[end]
                } else {
                    index.line[el.id]
                };
                by_line.entry(line).or_default().push((end, written));
            }
            for (line, ends) in by_line {
                let written = ends.iter().all(|&(_, written)| written);
                let kinds: Vec<String> = ends
                    .iter()
                    .map(|&(end, _)| {
                        let name = &index.qualified[end];
                        let kind = index.kind[end].keyword();
                        // An anonymous end takes its owner's qualified name, and quoting it reads
                        // as "X is an end of X"; the kind is the part worth printing.
                        if name.is_empty() || name == &qualified(el, index) {
                            format!("a {kind}")
                        } else {
                            format!("`{name}`, a {kind}")
                        }
                    })
                    .collect();
                let element = if written {
                    // The end owns the line; reporting at the declaration would name the wrong
                    // element.
                    el.children
                        .iter()
                        .find(|child| index.line[child.id] == line)
                        .unwrap_or(el)
                } else {
                    el
                };
                out.push(Violation::new(
                    check.code,
                    element,
                    qualified(element, index),
                    if written {
                        format!(
                            "{} {} of `{}` is {}; {}",
                            article(check.noun),
                            check.noun,
                            qualified(el, index),
                            kinds.join(", "),
                            check.says
                        )
                    } else {
                        format!(
                            "`{}` inherits {} {}; {}",
                            qualified(el, index),
                            if kinds.len() == 1 {
                                format!("{} {}", article(check.noun), check.noun)
                            } else {
                                format!("{} {}s", kinds.len(), check.noun)
                            },
                            kinds.join("; "),
                            check.says
                        )
                    },
                ));
            }
        });
    }
    out
}

/// `a` or `an` before the noun, so the message reads as English without a special case per entry.
fn article(noun: &str) -> &'static str {
    match noun.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}
