//! `variable-modifiers` — a feature's `var`, `constant`, `portion` and `end` words are a set of
//! mutually constraining flags, and three of the four rules below are contradictions between them.
//!
//! The rules, from the specification's table:
//!
//!   * `validateFeatureEndIsConstant` — `isEnd and isVariable implies isConstant`. An association
//!     end that can change is not an end: the end of a link is fixed when the link is made.
//!   * `validateFeatureConstantIsVariable` — `isConstant implies isVariable`. A constant that is
//!     not a variable is not a thing that could have varied; the notation admits the pair and the
//!     rule says the pair is what `constant` alone means.
//!   * `validateFeaturePortionNotVariable` — `isPortion implies not isVariable`. A portion is a
//!     slice of an occurrence fixed by its portion kind; a variable portion would slice something
//!     else on every access.
//!   * `validateOccurrenceUsageIndividualUsage`-adjacent field checks are in the group modules;
//!     this module keeps only what these four flags decide.
//!
//! `validateFeatureIsVariable` is **not** here. Its consequent needs
//! `owningType.specializes('Occurrences::Occurrence')`: the owner must resolve, and the bundled
//! fallback library (`sysml-syntax/stdlib.sysml`) holds only `ScalarValues` and the ISQ packages,
//! so a model that imports nothing resolves the name to nothing and the consequent cannot be
//! answered. Deciding it against an unresolvable owner would report every variable feature in a
//! model that does not import `Occurrences` — including correct ones.
//!
//! The report code this group reports under is `SEM034`.
//!
//! # Why `isVariable` is read rather than derived
//!
//! The reference builder derives `isVariable` from `isConstant` and from `isEnd`
//! (`research/upstream/mod/sysmlv2-model-0.1.2/src/build.rs:233-237`, `:2243-2249`) because the
//! abstract syntax holds the implied flag. This tree holds what the source *wrote*
//! (`Modifiers::is_variable`, `sysml-model/src/lib.rs`), so `constant x : Real;` arrives with
//! `is_constant` true and `is_variable` false. That is the pair the notation allows and the pair
//! `validateFeatureConstantIsVariable` is about — deriving it first would make the rule
//! unfalsifiable, which is the same reasoning `rules/variation_rules.rs` records for
//! `validateUsageVariationIsAbstract`.

use sysml_model::{Element, Group, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM034";

/// Whether a kind can write these modifiers at all.
///
/// Structural, interface, data and behaviour elements are features and can carry `var` /
/// `constant` / `end`; a namespace or a requirement cannot, and a membership (`Frame`, `Subject`)
/// is not a feature at all — an `end` on one would be a parse accident, not a model to judge.
fn is_feature(kind: &Kind) -> bool {
    matches!(
        kind.group(),
        Group::Structure | Group::Interface | Group::Data | Group::Behavior
    )
}

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The word a mismatch should name, for the message.
fn written(el: &Element) -> String {
    let m = &el.modifiers;
    let mut words: Vec<&str> = Vec::new();
    if el.is_end {
        words.push("end");
    }
    if m.is_derived {
        words.push("derived");
    }
    if m.is_abstract {
        words.push("abstract");
    }
    if m.is_variable {
        words.push("var");
    }
    if m.is_constant {
        words.push("constant");
    }
    if m.is_composite {
        words.push("composite");
    }
    if let Some(portion) = &m.portion {
        words.push(portion);
    }
    words.join(" ")
}

/// `validateFeatureEndIsConstant` — an `end` that is a variable must also be constant.
fn end_is_constant(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if !(el.is_end && el.modifiers.is_variable && !el.modifiers.is_constant) {
        return;
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified(el, index),
        format!(
            "`{}` is written `{}`: an `end` that is a variable must also be constant",
            qualified(el, index),
            written(el)
        ),
    ));
}

/// `validateFeatureConstantIsVariable` — `constant` implies `var`.
fn constant_is_variable(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if !(el.modifiers.is_constant && !el.modifiers.is_variable) {
        return;
    }
    // KerML's `const` and `var` are alternatives in the grammar (`KerML.xtext:519`), and the
    // reference states no error for `const feature f;` alone
    // (`Subsetting_constant_invalid.kerml.xt:24`). The rule is the SysML `constant` modifier
    // losing against the default `variable` of a usage.
    if el.kind == sysml_model::Kind::KermlFeature {
        return;
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified(el, index),
        format!(
            "`{}` is written `{}`: `constant` states a value that does not change, so it must \
             also be a variable (`var`)",
            qualified(el, index),
            written(el)
        ),
    ));
}

/// `validateFeaturePortionNotVariable` — a portion is not a variable.
fn portion_not_variable(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if !(el.modifiers.portion.is_some() && el.modifiers.is_variable) {
        return;
    }
    let portion = el.modifiers.portion.as_deref().unwrap_or("portion");
    out.push(Violation::new(
        CODE,
        el,
        qualified(el, index),
        format!(
            "`{}` is written `{}`: a `{portion}` is not a variable",
            qualified(el, index),
            written(el)
        ),
    ));
}

/// `validateRedefinitionEndConformance` — `redefinedFeature.isEnd implies redefiningFeature.isEnd`.
///
/// An `end` marks a feature as one end of an association. A feature that redefines an end and is
/// not itself an end would detach the association from that role, so the notation does not allow
/// it. Both halves are on the tree: `Element::is_end` and `Element::redefines`.
fn redefinition_end_conformance(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if el.is_end {
        return;
    }
    for text in &el.redefines {
        let head = text.split(',').next().unwrap_or(text).trim();
        let Some(target) = index.resolve(el.id, head).found() else {
            continue;
        };
        if !index.is_end_of(target) {
            continue;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` redefines the `end` feature `{}` but is not written `end`",
                qualified(el, index),
                index.qualified[target]
            ),
        ));
    }
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !is_feature(&el.kind) {
            return;
        }
        end_is_constant(el, index, &mut out);
        constant_is_variable(el, index, &mut out);
        portion_not_variable(el, index, &mut out);
        redefinition_end_conformance(el, index, &mut out);
    });
    out
}
