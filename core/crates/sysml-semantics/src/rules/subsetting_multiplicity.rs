//! `subsetting-multiplicity` — a subsetting may not widen the bound it narrows, and a redefinition
//! may not lower the floor.
//!
//! `Subsetting::subsettingFeature` denotes a subset of what `subsettedFeature` denotes, so its
//! *upper* bound cannot be larger. The pilot implementation states the same warning for subsettings
//! and redefinitions alike: "Subsetting/redefining feature should not have larger multiplicity
//! upper bound".
//!
//! A redefinition also may not shrink the *lower* bound, and the pilot words that one for
//! redefinitions only: "Redefining feature should not have smaller multiplicity lower bound". The
//! wording is the evidence. `attribute a3[*] :> a2` has lower bound 0 against the target's 1, and
//! the pilot states no warning for it; `part redefines cyl[1..6]` against `[4..6]` states one.
//!
//! ```text
//! attribute a1;
//! part def P {
//!     attribute a2 :> a1;        // fine: the default is [1..1] on both sides
//!     attribute a3[*] :> a2;     // warned: [0..*] is wider than [1..1]
//! }
//! ```
//!
//! Each violated end is its own report: `part mycyl[1..8] redefines cyl` against `[4..6]` breaks
//! both, and the pilot states two warnings on that line.
//!
//! # The default is `[1..1]`, for both sides
//!
//! A feature that writes no bound has one: the pilot's own model says so in a comment — "explicit
//! subsettings do not exclude the default [1..1]" — and asserts the warning for
//! `attribute a5[*] :> a4` where neither `a4` nor `a5`'s target writes a bound. Reading an absent
//! bound as unbounded would report nothing at all, which is what this check did until it read that
//! model.
//!
//! # What is not judged
//!
//! A bound this layer cannot read as numbers (`[n]` where `n` is a name, an expression) is skipped:
//! comparing it would mean guessing, and a warning about a legal model is worse than a missed one.
//! A target that does not resolve is skipped for the same reason — that is a reference problem and
//! the reference pass reports it.
//!
//! The report code this group reports under is `SEM092`.

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

const CODE: &str = "SEM092";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The two bounds a feature states, in the answers a comparison needs.
#[derive(Clone, Copy)]
struct Bounds {
    /// The lower bound. Always a number: `[*]` is `[0..*]`.
    lower: u64,
    /// The upper bound, `None` for unbounded.
    upper: Option<u64>,
}

enum Written {
    Known(Bounds),
    /// The feature writes no bound and has no default one, so there is nothing to compare.
    Absent,
    /// A bound this layer cannot read (`[n]`, an expression).
    Unreadable,
}

/// Whether an absent bound means `[1..1]` for a feature of this kind.
///
/// It does for what a type is *made of* — an attribute, part, item, port, occurrence. It does not
/// for an action or a state usage, and the pilot model says so in its own comment: "These kinds of
/// usages do not have default multiplicity", marking `action n2[*] :> n1` and `state s2[*] :> s1`
/// as clean. A behaviour is not a value with a count.
fn default_is_one(kind: &Kind) -> bool {
    !kind.is_metaclass("ActionUsage") && !kind.is_metaclass("StateUsage")
}

fn bounds(text: Option<&str>, kind: &Kind) -> Written {
    let Some(text) = text else {
        return if default_is_one(kind) {
            Written::Known(Bounds {
                lower: 1,
                upper: Some(1),
            })
        } else {
            Written::Absent
        };
    };
    let parsed = text
        .trim()
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .map(str::trim);
    let Some(inner) = parsed else {
        return Written::Unreadable;
    };
    // `[*]` is `[0..*]` (`KerML.xtext:305-320`), and a single number bounds both ends to itself.
    if inner == "*" {
        return Written::Known(Bounds {
            lower: 0,
            upper: None,
        });
    }
    let (lower, upper) = match inner.split_once("..") {
        Some((lower, upper)) => (lower.trim(), upper.trim()),
        None => (inner, inner),
    };
    let Ok(lower) = lower.parse::<u64>() else {
        return Written::Unreadable;
    };
    let upper = if upper == "*" {
        None
    } else {
        match upper.parse::<u64>() {
            Ok(value) => Some(value),
            Err(_) => return Written::Unreadable,
        }
    };
    Written::Known(Bounds { lower, upper })
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        // Only a feature has a bound, and only one that subsets something has a supertype to widen.
        if !el.kind.is_feature() {
            return;
        }
        let Written::Known(written) = bounds(el.multiplicity.as_deref(), &el.kind) else {
            return;
        };
        for clause in el.supertype_list() {
            let head = clause.split(',').next().unwrap_or(clause).trim();
            if head.is_empty() {
                continue;
            }
            let owner = index.parent.get(el.id).copied().flatten();
            let target = match index.resolve(el.id, head) {
                Resolution::Found(id) if id != el.id => Some(id),
                _ => owner.and_then(|owner| index.inherited_member_of(owner, head)),
            };
            let Some(target) = target else {
                continue;
            };
            if !index.kind[target].is_feature() {
                continue;
            }
            let Written::Known(inherited) =
                bounds(index.multiplicity_of(target), &index.kind[target])
            else {
                continue;
            };
            let target_name = index
                .qualified
                .get(target)
                .map(String::as_str)
                .unwrap_or(head);
            let written_text = el.multiplicity.as_deref().unwrap_or("[1..1]");
            let inherited_text = index.multiplicity_of(target).unwrap_or("[1..1]");
            // The upper bound may not widen: the subsetting denotes a subset of what the target
            // denotes, so it cannot hold more.
            let upper_wider = match (written.upper, inherited.upper) {
                (None, Some(_)) => true,
                (Some(a), Some(b)) => a > b,
                _ => false,
            };
            if upper_wider {
                out.push(Violation::warning(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` is bounded `{written_text}` but subsets `{target_name}`, which is \
                         bounded `{inherited_text}`; a subsetting may not have a larger upper \
                         bound (subsetting multiplicity)",
                        qualified(el, index)
                    ),
                ));
            }
            // The lower bound may not shrink, and only a redefinition says so: the pilot's message
            // is "Redefining feature should not have smaller multiplicity lower bound", and it
            // asserts nothing of the sort for `attribute a3[*] :> a2`, whose lower bound is 0
            // (`DefaultMultiplicity_invalid.sysml.xt:57`).
            let is_redefinition = el
                .redefines
                .iter()
                .any(|text| text.split(',').next().unwrap_or(text).trim() == head);
            if is_redefinition && written.lower < inherited.lower {
                out.push(Violation::warning(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` is bounded `{written_text}` but redefines `{target_name}`, which is \
                         bounded `{inherited_text}`; a redefinition may not have a smaller lower \
                         bound (subsetting multiplicity)",
                        qualified(el, index)
                    ),
                ));
            }
        }
    });
    out
}
