//! `referential` — whether a usage is referential, and what a feature owned by an attribute may
//! be.
//!
//! Five rules, all of them reading the same derived property. The group reports under `SEM031`;
//! `validateReferenceUsageIsReference` is the sixth and is `not_applicable` in the ledger, for the
//! reason recorded at the end of this note.
//!
//!   * `validateUsageIsReferential` (935) — Usage,
//!     `direction <> null or isEnd or featuringType->isEmpty() implies isReference`,
//!     "A Usage that is directed, an end feature or has no featuringTypes must be referential."
//!   * `validatePortUsageIsReference` (1025) — PortUsage,
//!     `owningType = null or not owningType.oclIsKindOf(PortDefinition) and not
//!     owningType.oclIsKindOf(PortUsage) implies isReference`.
//!   * `validateAttributeUsageIsReference` (977) — AttributeUsage, `isReference`.
//!   * `validateAttributeUsageFeatures` (983) — AttributeUsage, `feature->forAll(not isComposite)`.
//!   * `validateAttributeDefinitionFeatures` (971) — AttributeDefinition,
//!     `feature->forAll(not isComposite)`.
//!
//! # What the rules read
//!
//! `isReference` is derived, not stored: `deriveUsageIsReference` is
//! `isReference = not isComposite` (`generated.rs:8957-8961`). Substituting that derivation turns
//! every `isReference` in the group into `not isComposite`, so the group reads one property —
//! [`Element::is_composite`] — and never the written word.
//!
//! That property is the *effective* `isComposite`, filled by the lowering
//! (`fill_feature_defaults`, `sysml-syntax/src/lower.rs`) from the specification's own derivation:
//! `ref` and an explicit `composite` decide it outright; a `ReferenceUsage`, an end, a directed
//! feature, an attribute usage and a port outside a port are non-composite; a type's other
//! features are composite; a feature owned by a package has no owning type and is not composite.
//! The arms and the rule behind each one are documented at that function.
//!
//! Reading the written word instead — what this module did before that field existed — decided
//! nothing that matters: `SysML.xtext:556-566` gives a SysML usage no `composite` word, so
//! `part b2 : B;` inside `port def pd1` came out non-composite and
//! `validatePortDefinitionOwnedUsagesNotComposite` could not reject the model whose violation the
//! pilot suite states (`PortUsage_Invalid.sysml.xt:35`).
//!
//! # `validateUsageIsReferential`
//!
//! The consequent is `isReference`, hence `not isComposite`; a violation is a composite usage with
//! at least one disjunct of the antecedent true.
//!
//! The antecedent's third disjunct is the one the tree could not decide before.
//! `Feature::featuringType` is derived (`deriveFeatureFeaturingType`, `generated.rs:8213-8217`)
//! from the `TypeFeaturing`s that feature the feature, and `Feature::owningType subsets
//! featuringType` (`KerML.kerml:166`) puts the owning type in that set. So a feature owned by a
//! type is always featured by it, `featuringType->isEmpty()` holds exactly when the owner is not a
//! type, and [`Index::featuring_types_of`](sysml_resolve::Index::featuring_types_of) answers it.
//! `featured by` (`KerML.xtext:568-571`) is not parsed yet and would only add to the set.
//!
//! With the default derivation in place, the antecedent can be true only where a declaration
//! overrides the default:
//!
//!   * a `composite` written on a directed feature or on an end,
//!   * a `composite` written on a usage whose owner is not a type — at package level, the shape
//!     that has no featuring type at all.
//!
//! Both are contradictions the model states, and both are now reported. An end reported here is
//! also reported by `validateFeatureEndNotDerivedAbstractCompositeOrPortion` (SEM030): the two
//! specification rules say the same thing about the same word, and the end case is recorded rather
//! than hidden — the same overlap SEM021/SEM033 already carries.
//!
//! # `validatePortUsageIsReference`
//!
//! OCL precedence makes the antecedent `(owningType = null) or ((not PortDefinition) and (not
//! PortUsage))`. The two owners that falsify it are [`Kind::PortDef`] and [`Kind::PortUsage`];
//! every other owner — none, a package, a part — leaves it true, because `owningType` is null when
//! the owner is not a `Type` and a non-port `Type` fails both conjuncts.
//!
//! The default derivation already makes such a port non-composite, so the rule fires on the one
//! thing that overrides the default: a `composite port` under a part, an action or a package.
//! That is the KerML spelling (`KerML.xtext:518`), and it states the contradiction the rule
//! forbids.
//!
//! Two guards are required. The lowering relabels every `end` declaration to [`Kind::PortUsage`]
//! (`sysml-syntax/src/lower.rs:281-283`), and the metamodel makes an end non-composite ahead of its
//! port arms (`build.rs:2243-2249` vs `:2278-2283`) — but a declaration that writes `composite end`
//! states the value outright, and `validateFeatureEndNotDerivedAbstractCompositeOrPortion` is the
//! rule that reports it, so this module leaves an end alone rather than reporting it twice. A port
//! copied onto a part usage from its definition is not an authored declaration
//! (`inherit_ports`, `sysml-syntax/src/lower.rs`), and judging it would report the definition's
//! own word once per usage.
//!
//! # `validateAttributeUsageIsReference`, `validateAttributeUsageFeatures`,
//! `validateAttributeDefinitionFeatures`
//!
//! The first is the single word `isReference`: an attribute usage that states `composite`, which
//! the default derivation refuses to supply. The two `...Features` rules read
//! `feature->forAll(not isComposite)`, where `feature` is derived as
//! `featureMembership.ownedMemberFeature` (`generated.rs:8161-8164`) — the element's children
//! restricted to what a `FeatureMembership` can own, which is [`Kind::is_feature`]. With the
//! default in place these two now reject `attribute def A { part p : P; }`: a part is composite,
//! and an attribute has no parts.
//!
//! # Gaps this module records rather than guesses
//!
//! Inheritance. `featureMembership` is
//! `ownedFeatureMembership->union(inheritedMembership->selectByKind(FeatureMembership))`
//! (`generated.rs:8147-8150`), but the tree copies inherited members for ports on part usages only
//! (`inherit_ports`, gated on [`Kind::PartUsage`]). An attribute definition that inherits a
//! composite feature from a supertype is therefore not reported.
//!
//! Two metaclasses that specialize the ruled ones are not covered: `EnumerationDefinition
//! specializes AttributeDefinition` (`generated.rs:1658`) lowers to [`Kind::EnumDef`], and
//! `EnumerationUsage specializes AttributeUsage` (`:1659`) to [`Kind::EnumValue`]. Both are
//! recorded as misses rather than treated as attributes, since neither is one.
//!
//! `Kind::AttributeUsage` also stands for the KerML classifiers `feature` / `step` / `expr` /
//! `bool` / `connector` (`sysml-syntax/src/lower.rs:258-261`) and for the kind-less reference
//! `mass : MassValue`. A KerML `composite feature f : T` is a `Feature` and not an
//! `AttributeUsage`, yet it lowers to an element indistinguishable from `composite attribute a :
//! T`; no field separates them. A project model is untriggered (no example or benchmark writes
//! `composite`), but a `.kerml` file handed to the CLI *as the model* would be reported for a legal
//! `composite feature`. The fix belongs in the projection, which would have to keep the keyword it
//! lowered from; `tests/rules_referential.rs` pins the current shape so the day the field arrives
//! the collision is visible.
//!
//! # `validateReferenceUsageIsReference` — not applicable
//!
//! `isReference` on a `ReferenceUsage`. The metaclass redefines the property with
//! `derived: true, default: Some(true)` (`generated.rs:2491-2493`), and the only notation that
//! could state the contrary is `composite ref x : T`, where `ref` wins
//! (`build.rs:2243-2248`). No model states a violation, and the rule is recorded as
//! `not_applicable` in `conformance::NOT_APPLICABLE` with that citation.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::{walk, Violation};

/// The `SEM` code this group reports under.
const CODE: &str = "SEM031";

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    walk(roots, |el| {
        usage_is_referential_issue(el, index, &mut out);
        port_usage_issue(el, index, &mut out);
        attribute_usage_issue(el, index, &mut out);
        attribute_features_issue(el, index, &mut out);
    });
    out
}

// -- validateUsageIsReferential ---------------------------------------------

/// `direction <> null or isEnd or featuringType->isEmpty() implies isReference`.
fn usage_is_referential_issue(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if !el.kind.is_feature() || el.inherited {
        return;
    }
    // The consequent: `isReference` is `not isComposite`.
    if !el.is_composite {
        return;
    }
    let no_featuring = index.featuring_types_of(el.id).is_empty();
    if el.direction.is_none() && !el.is_end && !no_featuring {
        return;
    }
    let why = if el.direction.is_some() {
        "it is directed".to_string()
    } else if el.is_end {
        "it is an end".to_string()
    } else {
        "nothing features it".to_string()
    };
    let owner = index.parent.get(el.id).copied().flatten();
    let where_owned = match owner {
        Some(owner) => format!(
            "its owner `{}` is a {}, which is not a type",
            index
                .qualified
                .get(owner)
                .map(String::as_str)
                .unwrap_or_default(),
            index.kind[owner]
        ),
        None => "nothing owns it".to_string(),
    };
    out.push(Violation::new(
        CODE,
        el,
        qualified_name(el, index),
        format!(
            "a usage that is directed, an end feature or has no featuring type must be \
             referential, but this usage states `composite` and {why} ({where_owned})"
        ),
    ));
}

// -- validatePortUsageIsReference -------------------------------------------

/// `owningType = null or not owningType.oclIsKindOf(PortDefinition) and not
/// owningType.oclIsKindOf(PortUsage) implies isReference`.
fn port_usage_issue(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if el.kind != Kind::PortUsage {
        return;
    }
    // A port copied onto a part usage from its definition is not an authored declaration
    // (`inherit_ports`); judging it would report the definition's word once per usage.
    if el.inherited {
        return;
    }
    // An `end` relabelled to `PortUsage` states `composite` against
    // `validateFeatureEndNotDerivedAbstractCompositeOrPortion`, which reports it. The module note
    // says why this rule leaves it alone.
    if el.is_end {
        return;
    }
    let owner = index.parent.get(el.id).copied().flatten();
    if let Some(owner) = owner {
        if index.kind[owner] == Kind::PortDef || index.kind[owner] == Kind::PortUsage {
            return;
        }
    }
    if !el.is_composite {
        return;
    }
    let owned_by = match owner {
        Some(owner) => format!(
            "its owner `{}` is a {}",
            index
                .qualified
                .get(owner)
                .map(String::as_str)
                .unwrap_or_default(),
            index.kind[owner]
        ),
        None => "nothing owns it".to_string(),
    };
    out.push(Violation::new(
        CODE,
        el,
        qualified_name(el, index),
        format!(
            "a port usage whose owning type is not a port definition or a port usage must be \
             referential, but {owned_by} and the usage states `composite`"
        ),
    ));
}

// -- validateAttributeUsageIsReference --------------------------------------

/// `isReference` on an `AttributeUsage`, i.e. `not isComposite`.
fn attribute_usage_issue(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if el.kind != Kind::AttributeUsage {
        return;
    }
    if !el.is_composite {
        return;
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified_name(el, index),
        "an attribute usage is always referential, but it states `composite`".to_string(),
    ));
}

// -- validateAttributeUsageFeatures / validateAttributeDefinitionFeatures ----

/// `feature->forAll(not isComposite)` on an `AttributeUsage` and on an `AttributeDefinition`.
fn attribute_features_issue(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if el.kind != Kind::AttributeUsage && el.kind != Kind::AttributeDef {
        return;
    }
    let owner = qualified_name(el, index);
    for child in &el.children {
        // `feature` is `featureMembership.ownedMemberFeature` (`generated.rs:8161-8164`).
        if !child.kind.is_feature() {
            continue;
        }
        // A port copied onto a part usage from its definition is not authored here either.
        if child.inherited {
            continue;
        }
        if !child.is_composite {
            continue;
        }
        out.push(Violation::new(
            CODE,
            child,
            qualified_name(child, index),
            format!(
                "`{owner}` owns the feature `{}` as composite, but every feature of an attribute \
                 must be non-composite",
                child.name
            ),
        ));
    }
}

/// The qualified name of an element, falling back to its own name for an element the index does
/// not know.
fn qualified_name<'a>(el: &'a Element, index: &'a Index) -> &'a str {
    index
        .qualified
        .get(el.id)
        .map(String::as_str)
        .unwrap_or(el.name.as_str())
}
