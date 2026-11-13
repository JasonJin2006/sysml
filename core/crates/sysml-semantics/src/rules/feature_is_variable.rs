//! `feature-is-variable` — `var` needs an owner that can change.
//!
//! `validateFeatureIsVariable`: `isVariable implies owningType <> null and
//! owningType.specializes('Occurrences::Occurrence')`. A variable is a feature whose value changes
//! over time, and time is what an occurrence has. A variable in a part definition — a structure —
//! names something that would have to change in a thing that does not have a lifetime.
//!
//! ```text
//! occurrence def Engine {
//!     var part rpm : Real;        // legal: an engine runs
//! }
//! part def Drawing {
//!     var part scale : Real;      // rejected: a drawing does not run
//! }
//! ```
//!
//! # How it reads
//!
//! The library is loaded, so `Occurrences::Occurrence` resolves — it is a `class` in the kernel
//! semantic library, id 10457 in a 106-root index. The owner's supertype closure is walked with
//! `Index::is_subtype`, which is the `specializes` the OCL asks for.
//!
//! An owner that does not resolve, or a library that is not loaded, leaves the rule silent: with
//! `SYSML_LIBRARY=off` there is no `Occurrence` to compare against, and reporting every `var` then
//! would be an artefact of the test harness rather than of the model.
//!
//! The report code this group reports under is `SEM077`.

use sysml_model::Element;
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM077";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The kinds SysML makes occurrences without writing a supertype clause.
///
/// Each of them specializes `Occurrences::Occurrence` in the SysML layer of the library, so the
/// question "does this owner have a lifetime" is answered by the kind alone.
pub(crate) fn owns_a_lifetime(kind: &sysml_model::Kind) -> bool {
    use sysml_model::Kind;
    matches!(
        kind,
        Kind::OccurrenceDef
            | Kind::PartDef
            | Kind::PartUsage
            | Kind::ItemDef
            | Kind::ItemUsage
            | Kind::ActionDef
            | Kind::ActionUsage
            | Kind::StateDef
            | Kind::StateUsage
            // A KerML `class` may be specialized into an occurrence — `Object` is a `class` in
            // the kernel library — so `var feature x;` inside one is not an error. The reference
            // reports it only for a `datatype`, which cannot be an occurrence
            // (`Feature_variable_invalid.kerml.xt:29`, `:31`).
            | Kind::ClassDef
            | Kind::PortDef
            | Kind::PortUsage
            | Kind::ConnectionDef
            | Kind::InterfaceDef
            | Kind::AllocationDef
            | Kind::Flow
            | Kind::CaseDef
            | Kind::AnalysisCaseDef
            | Kind::VerificationDef
            | Kind::VerificationUsage
            | Kind::UseCaseDef
            | Kind::UseCaseUsage
            | Kind::ConstraintDef
            | Kind::ConstraintUsage
            | Kind::CalcDef
            | Kind::CalcUsage
            | Kind::Perform
            | Kind::ExhibitState
    )
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !el.modifiers.is_variable {
            return;
        }
        // A result slot is a variable by construction — the function assigns to it — but the
        // model never wrote `var`. This rule is about the word.
        if el.clause.as_deref() == Some("return") {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        // The library element the rule names. Absent when the harness runs with
        // `SYSML_LIBRARY=off`; the rule then says nothing rather than guessing.
        let Some(occurrence) = index
            .name
            .iter()
            .position(|n| n == "Occurrence")
            .filter(|&id| index.qualified[id].ends_with("Occurrence"))
        else {
            return;
        };
        // Two ways to be an occurrence. The explicit one is an inherited `:>` chain that reaches
        // `Occurrences::Occurrence`. The implicit one is the kind: `part`, `action`, `state`,
        // `item` and `occurrence` are all occurrences in SysML — the SysML layer specializes each
        // of them from the KerML occurrence, and the model writes no clause for it.
        //
        // Without the second half this rule reported `var` inside every part and case; with it,
        // what remains is a variable in a namespace or a data type, which is the case the rule is
        // about.
        if index.is_subtype(owner, occurrence) || owns_a_lifetime(&index.kind[owner]) {
            return;
        }
        let owner_kind = index.kind[owner].keyword();
        let owner_name = index
            .qualified
            .get(owner)
            .cloned()
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| index.name[owner].clone());
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is a variable inside `{owner_name}`, a {owner_kind} that is not an \
                 occurrence; a value that changes over time needs something with a lifetime \
                 (validateFeatureIsVariable)",
                qualified(el, index)
            ),
        ));
    });
    out.extend(el_needs_variable(roots, index));
    out.extend(overrides_a_default(roots, index));
    out
}

/// `validateFeatureValueOverriding` — `featureWithValue.redefinition.redefinedFeature->
/// closure(redefinition.redefinedFeature).valuation->forAll(isDefault)`.
///
/// A feature that states a value, and every feature it redefines directly or indirectly, must have
/// only *default* values among them: a redefinition may restate a default, but it may not stand on a
/// value that was stated outright.
///
/// # What `isDefault` actually is
///
/// This rule was previously implemented against the wrong flag, and the error ran in both directions.
/// It read `Index::is_initial_of` — the `:=` marker — so it rejected a redefinition standing on a
/// `:=` value and accepted one standing on `= 42`. The grammar
/// (`KerML.xtext:744-750`) settles what `isDefault` means:
///
/// ```text
/// FeatureValue :
///     ( '='                                   // neither initial nor default
///     | isInitial ?= ':='                     // initial, not default
///     | isDefault ?= 'default' ( '=' | isInitial ?= ':=' )?   // default
///     )
///     ownedRelatedElement += OwnedExpression
/// ;
/// ```
///
/// `isDefault` is true **only when the word `default` was written**. A plain `= 42` is not a default,
/// so a redefinition standing on it violates the OCL's `forAll(isDefault)`. Measured before the fix:
/// `part def A { attribute a = 42; }` + `part def B :> A { attribute a :>> a = 7; }` reported nothing,
/// and the `:=` case reported an error it should not have.
///
/// The flag now comes from [`Element::is_default`] (`parser.rs`, `lower.rs`), which records the authored
/// word and nothing else.
///
/// # What still bounds this
///
/// `is_initial` values also violate `forAll(isDefault)`, so they are reported too — but they may trip
/// `validateFeatureValueIsInitial` first, and both diagnostics are correct about different rules.
///
/// The redefinition closure is walked with [`Index::inherited_member_of`], which is the scope a `:>>`
/// names, and the values are read from the index rather than the tree because only ids are available
/// once the walk leaves the element that wrote the clause.
fn overrides_a_default(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !index.has_value_of(el.id) || el.redefines.is_empty() {
            return;
        }
        let Some(owner) = index.parent.get(el.id).copied().flatten() else {
            return;
        };
        let mut frontier: Vec<usize> = Vec::new();
        for text in &el.redefines {
            let head = text.split(',').next().unwrap_or(text).trim();
            if let Some(target) = index.inherited_member_of(owner, head) {
                frontier.push(target);
            }
        }
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut offender: Option<usize> = None;
        while let Some(current) = frontier.pop() {
            if !seen.insert(current) {
                continue;
            }
            // `valuation->forAll(isDefault)`: a value that was written plainly is not a default, and
            // neither is one written with `:=`. Either way the redefinition may not restate over it.
            if index.has_value_of(current) && !index.is_default_of(current) {
                offender = Some(current);
                break;
            }
            // `closure(redefinition.redefinedFeature)`: the redefinitions of what this feature
            // redefines. The owner of a redefined feature is the one its own `:>>` resolved in.
            let scope = index
                .parent
                .get(current)
                .copied()
                .flatten()
                .unwrap_or(current);
            for text in index.redefines_of(current) {
                let head = text.split(',').next().unwrap_or(text).trim();
                if let Some(next) = index.inherited_member_of(scope, head) {
                    frontier.push(next);
                }
            }
        }
        let Some(offender) = offender else {
            return;
        };
        let offender_value = if index.is_initial_of(offender) {
            "an initial (`:=`) value"
        } else {
            "a value that is not a default"
        };
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` states a value over `{}`, which has {offender_value}; a redefinition may only \
                 restate a `default` value (validateFeatureValueOverriding)",
                qualified(el, index),
                index.qualified[offender]
            ),
        ));
    });
    out
}

/// `validateFeatureValueIsInitial` — `isInitial implies featureWithValue.isVariable`.
///
/// The rule is stated of the `FeatureValue` the declaration's `= value` writes; the projection keeps
/// the value on the feature and marks the `:=` spelling with [`Element::is_initial`]
/// (`parser.rs:1237`, `:2732`). `:=` says "assign this now", and only a variable can be assigned, so
/// an initial value on anything that is not `var` states a contradiction.
fn el_needs_variable(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if !el.is_initial || el.modifiers.is_variable {
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified(el, index),
            format!(
                "`{}` is given an initial value with `:=` but is not `var`; only a variable can be \
                 assigned (validateFeatureValueIsInitial)",
                qualified(el, index)
            ),
        ));
    });
    out
}
