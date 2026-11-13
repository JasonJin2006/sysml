//! `feature-modifiers` — the modifier flags a feature states, and what they exclude.
//!
//! Nine rules were assigned to this group (`data/spec_rules.json`; line numbers below are into
//! `data/spec_rules.rs`, which carries the same `name` / `metaclass` / `ocl` / `says`):
//!
//!   * `validateFeatureEndIsConstant` (383) — `isEnd and isVariable implies isConstant`;
//!   * `validateFeatureIsVariable` (389) — `isVariable implies owningType <> null and
//!     owningType.specializes('Occurrences::Occurrence')`;
//!   * `validateFeatureConstantIsVariable` (419) — `isConstant implies isVariable`;
//!   * `validateFeaturePortionNotVariable` (425) — `isPortion implies not isVariable`;
//!   * `validateFeatureEndNotDerivedAbstractCompositeOrPortion` (461) — `isEnd implies not
//!     (isDerived or isAbstract or isComposite or isPortion)`;
//!   * `validateOccurrenceUsageIndividualDefinition` (893) —
//!     `occurrenceDefinition->selectByKind(OccurrenceDefinition)->select(isIndividual).size() <= 1`;
//!   * `validateOccurrenceUsageIndividualUsage` (899) — `isIndividual implies
//!     individualDefinition <> null`;
//!   * `validateOccurrenceUsageIsPortion` (905) — `portionKind <> null implies isPortion`;
//!   * `validateOccurrenceUsagePortionKind` (911) — `portionKind <> null implies owningType <> null
//!     and (owningType.oclIsKindOf(OccurrenceDefinition) or
//!     owningType.oclIsKindOf(OccurrenceUsage))`.
//!
//! The group reports under `SEM030`. Three of the nine are decided here; the other six are named at
//! the end with the field that blocks each.
//!
//! # `validateFeatureEndNotDerivedAbstractCompositeOrPortion`
//!
//! An `end` is what an association relates, not something its association is made of, so it may
//! carry none of the four modifiers. Every side of the OCL is on the tree: [`Element::is_end`]
//! (`sysml-model/src/lib.rs:860`) and four flags in [`Modifiers`] — `is_derived` (`:881`),
//! `is_abstract` (`:878`), `is_composite` (`:887`), `portion` (`:889`).
//!
//! Two readings of the projection decide the rule's reach:
//!
//!   * `is_composite` records only that the word `composite` was written
//!     (`sysml-syntax/src/lower.rs:351-353`). The metamodel's default is false
//!     (`generated.rs:2234`) and the reference implementation forces it false on every feature that
//!     takes the end role (`research/upstream/sem/mod/sysmlv2-model-0.1.2/src/build.rs:2303-2310`),
//!     so a plain `end e : W` is not composite and must not be reported. Reading the written word is
//!     the only reading that keeps that true.
//!   * `portion` carries `portion`, `snapshot` or `timeslice` (`lower.rs:354-357`), and all three
//!     satisfy `isPortion` under the reference reading: `portion` through the keyword table
//!     (`build.rs:1703`), `snapshot`/`timeslice` through `PortionUsage`'s `{ isPortion = true }`
//!     (`build.rs:238-245`). Any of the three on an end is therefore the `isPortion` disjunct.
//!
//! The four modifiers are mutually exclusive with `end` in the grammar itself
//! (`SysML.xtext:568-570`: `UnextendedUsagePrefix` is `EndUsagePrefix | BasicUsagePrefix`, and the
//! basic prefix is the one carrying `derived`, `abstract`, `composite` and `portion`), so writing
//! both is a model error the parser accepts and this rule exists to catch.
//!
//! ## Known limit: the owned cross feature is not modelled
//!
//! §7.13.2 splits a connection end that declares an owned cross feature into two declarations, and
//! only one of them is the end. `lower.rs` does not model the owned cross feature — the module
//! `feature_multiplicity.rs:49-67` records the same defect from the multiplicity side, and
//! `tests/rules_feature_multiplicity.rs:376-415` pins the shape it leaves: `end theCauses [*]
//! occurrence theCause :> causes` becomes two flat siblings, the first carrying `is_end`.
//!
//! The prefix run ends at the name, so the second declaration is a separate member and keeps its
//! own modifiers. In `end cross [0..1] derived part hub : Hub;` the `derived` therefore lands on
//! `hub`, while `is_end` lands on `cross` — and this rule, which asks whether the element carrying
//! `is_end` wrote a forbidden modifier, stays silent.
//!
//! The defect is a missed violation, not a false one: the metamodel forbids `derived` on the end
//! and this tree puts the flag somewhere else. Guarding against it is not possible here — the tree
//! exposes no `ownedCrossFeature` and no `crosses` target on an end — so the fix belongs in
//! `lower.rs`.
//!
//! # `validateOccurrenceUsageIndividualUsage`
//!
//! `individualDefinition` is derived: the first of `occurrenceDefinition` that is an
//! `OccurrenceDefinition` with `isIndividual = true` (`generated.rs:8765-8768`). The tree keeps
//! `isIndividual` (`sysml-model/src/lib.rs:880`) and the authored typing as [`Element::type_ref`]
//! (`:738`), so the consequent becomes "some type named here carries `isIndividual`".
//!
//! Three deliberate narrowings keep the rule from rejecting legal models:
//!
//!   * **The `selectByKind(OccurrenceDefinition)` filter is not applied.** `individual def Wheel` —
//!     the notation's ordinary way of declaring one — does not lower to a definition at all:
//!     `prefixes()` consumes `individual` (`sysml-parser/src/parser.rs:275-287`), `declaration()`
//!     then consumes the `def` (`parser.rs:846-851`), and no kind keyword is left, so the
//!     declaration falls through to the kind-less arm (`parser.rs:859-895`) and becomes
//!     `Kind::AttributeUsage` (`lower.rs:158-163`). Requiring the target's kind would reject the
//!     idiomatic `individual def Wheel; individual part w : Wheel;`, which is legal.
//!     `individual part def Wheel` does keep [`Kind::PartDef`], and is the shape the tests use where
//!     a real definition is wanted.
//!   * **The metaclass gate is `OccurrenceUsage`.** That is what keeps `individual` on a definition
//!     out of the rule: `individual part def Wheel` lowers to [`Kind::PartDef`] with
//!     `isIndividual = true`, and a `PartDefinition` is an `OccurrenceDefinition`, not an
//!     `OccurrenceUsage` (`generated.rs:1785`, the `ancestors` table; `PartUsage`'s own row names
//!     `ItemUsage` and `OccurrenceUsage`).
//!   * **Kinds that stand for both spellings are skipped** — see
//!     [`collapses_definition_and_usage`]. A modifier written on such an element cannot be
//!     attributed to a definition or a usage, and the two disagree about the rule.
//!
//! The consequence is that the rule fires only when the consequent is *certainly* null. Two cases
//! would be guesses, so both stay silent:
//!
//!   * A typing the resolver cannot reach. An unresolved reference is another rule's report, and
//!     assuming the target is not individual would add a defect the model may not have.
//!   * A clause that names more than one element. `individualDefinition` reads the whole
//!     `occurrenceDefinition` list, while [`Element::type_ref`] keeps the first typing target only
//!     (`lower.rs:326` calls `Head::target_of`, `sysml-parser/src/ast.rs:211-216`). For
//!     `individual part c : Plain, Ind;` the tree shows `Plain` and the metamodel would find `Ind`.
//!     [`Element::ref_spans`] (`sysml-model/src/lib.rs:827`, filled at `lower.rs:362-366`) holds
//!     every authored target, so the check treats it as an over-approximation of the typing list:
//!     if *any* reference the declaration writes names an individual element, the consequent may be
//!     non-null and the rule stays silent. `ref_spans` does not record which relation each target
//!     belongs to, so a reference that is not a typing can only make the check quieter, never
//!     wrong.
//!
//! A feature with no typing at all is judged only when it also claims no supertype. With a
//! supertype, `deriveFeatureType` (`generated.rs:8225-8227`) makes `type` include the types of the
//! features it subsets, so an individual definition can arrive through the inheritance instead.
//!
//! # `validateOccurrenceUsagePortionKind`
//!
//! `portionKind <> null` is true only for `snapshot` and `timeslice`. `portion` is the KerML
//! `isPortion` word (`KerML.xtext:518`) and sets no portion kind; `PortionKind` is
//! `snapshot | timeslice` (`SysML.xtext:860-862`) and is written only by a `PortionUsage`
//! (`SysML.xtext:854-861`). [`Modifiers::portion`] keeps the word, so the antecedent is
//! `portion == "snapshot" | "timeslice"` and a bare `portion part q` stays out of this rule.
//!
//! `owningType` is derived (`generated.rs:2227` — `owningType` is `subsets owningNamespace,
//! featuringType` in the library at
//! `core/stdlib/Kernel Libraries/Kernel Semantic Library/KerML.kerml:175`) and has no field. For a
//! member declaration the owning type is the element that owns it, which is [`Index::parent`]. A
//! usage declared at the root of the model has no owner at all, and the OCL's own
//! `owningType <> null` conjunct rejects that case.
//!
//! The rule needs no separate metaclass gate. `portionKind` is written only by the productions that
//! build an `OccurrenceUsage` — `OccurrenceUsagePrefix` (`SysML.xtext:836-841`), `PortionUsage`
//! (`:854-861`) and `ControlNodePrefix` (`:1655-1660`) — so `portionKind <> null` already implies
//! the metaclass, exactly as the OCL's own wording has it. Restricting the rule to the kinds a
//! metaclass test would accept is not only unnecessary but wrong in one direction: a bare
//! `snapshot s : O;` writes no kind keyword, falls through `declaration_kind()`
//! (`sysml-parser/src/parser.rs:859-895`) and lowers to [`Kind::AttributeUsage`]
//! (`lower.rs:158-163`), which is a `PortionUsage` in the metamodel but not an `OccurrenceUsage` in
//! the projection's mapping.
//!
//! No definition production admits a portion kind (`SysML.xtext:490-505` — `BasicDefinitionPrefix`
//! is `abstract | variation`, and `OccurrenceDefinitionPrefix` adds only `individual` and an empty
//! multiplicity), so a definition cannot state one. Two exclusions remain, both about elements whose
//! kind cannot say which spelling they came from:
//!
//!   * A kind that can only be a definition is skipped outright. Its metaclass does not declare
//!     `portionKind` at all (`generated.rs:2444`), so the rule does not apply to it.
//!   * A kind that stands for both spellings is judged only when it carries a type clause. The
//!     definition spelling turns `:` into `:>` and leaves no `type_ref`
//!     (`sysml-parser/src/parser.rs:1173-1177`), so a `type_ref` proves the usage spelling; without
//!     one, `item def I;` and `item i;` are the same element and the two readings disagree about
//!     whether `portionKind` even exists.
//!
//! [`is_occurrence_type`] can accept the disjunction's two sides together: `CaseDef` stands for
//! `CaseDefinition` and for `CaseUsage`, and the disjunction holds under either reading.
//!
//! ## The owning type is read from the owner's words as well as its kind
//!
//! [`is_occurrence_type`] reads a [`Kind`], and for the owner that is not enough by itself. A
//! prefix-only occurrence usage carries no kind keyword: `IndividualUsage` (`SysML.xtext:845-851`)
//! and `PortionUsage` (`:853-857`) both take a bare `Usage` after the prefix, and
//! `OccurrenceUsagePrefix` (`:833-841`) is a prefix on the same bare `Usage`. So
//! `snapshot context_t0 : C;` and `individual reference : R;` lower to [`Kind::AttributeUsage`]
//! (`lower.rs:158-163`) while being `OccurrenceUsage`s in the metamodel.
//!
//! Reading only the kind made the rule reject `validation/06-Individual and Snapshots/
//! 6-Individual and Snapshots.sysml:87-102`, where three `snapshot`s sit under a bare `individual`
//! and three more under a bare `snapshot` — six false positives in one file, caught by
//! `tools/coverage.py` dropping from 56/56 to 55/56. [`occurrence_type_flags`] therefore treats an
//! owner that wrote `individual`, `snapshot` or `timeslice` as an occurrence type whatever kind it
//! lowered to. The extra input only ever silences a report, and it silences exactly the elements
//! whose written prefix puts them in the occurrence hierarchy.
//!
//! # The six rules that are not decided here
//!
//! Four are blocked by one missing flag. `isVariable` is a stored `Feature` attribute
//! (`generated.rs:2245`) with no counterpart on [`Element`]: [`Modifiers`]
//! (`sysml-model/src/lib.rs:877-890`) carries nine flags and `isVariable` is not one of them. It is
//! not merely dropped by lowering — the parser never sets it: `prefixes()`
//! (`sysml-parser/src/parser.rs:251-321`) has no `var` arm, so `var x : Real` does not even arrive
//! as a typed feature (the word becomes the declaration's name and `x : Real` a second member).
//!
//!   * `validateFeatureEndIsConstant` — needs `isVariable` for the antecedent.
//!   * `validateFeatureIsVariable` — the antecedent is `isVariable`; the consequent also wants
//!     `owningType` and `specializes('Occurrences::Occurrence')`, and no `Occurrences` package is
//!     resolvable from a project unless the on-disk library is discovered: the bundled fallback
//!     holds `ScalarValues` and the ISQ packages only (`sysml-syntax/stdlib.sysml:1-60`).
//!   * `validateFeatureConstantIsVariable` — `isConstant` (`sysml-model/src/lib.rs:882`) is
//!     readable, `isVariable` is not. The reference implementation derives the implication
//!     (`build.rs:233-237`), so the rule is unobservable even where the flag exists.
//!   * `validateFeaturePortionNotVariable` — `isPortion` is readable through
//!     [`Modifiers::portion`], `isVariable` is not.
//!
//! The other two are blocked by the projection of a usage's type list:
//!
//!   * `validateOccurrenceUsageIsPortion` — `portionKind <> null` and `isPortion` are the *same*
//!     field here ([`Modifiers::portion`], `sysml-model/src/lib.rs:889`), and the reference
//!     implementation sets both together for `snapshot`/`timeslice` (`build.rs:238-245`). No model
//!     this tree can describe has a portion kind without `isPortion`, so the rule cannot fail, and
//!     a rule whose check cannot fail must not be listed as enforced (`conformance.rs:13-15`).
//!   * `validateOccurrenceUsageIndividualDefinition` — the OCL counts the whole
//!     `occurrenceDefinition` list. [`Element::type_ref`] keeps only the first typing target
//!     (`lower.rs:326`, `ast.rs:211-216`), so `ind : Ind, Ind2` arrives as `"Ind"` and the count can
//!     never exceed one. [`Element::ref_spans`] holds every target but records no relation
//!     operator, so subsetting, redefinition and reference targets cannot be told from typings
//!     without guessing — and a guess here reports a legal model.

use std::collections::HashMap;

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::{walk, Violation};

/// The `SEM` code this group reports under.
const CODE: &str = "SEM030";

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    // `isIndividual` per element, so the flag of a typing target can be read: [`Index`] carries
    // kinds, names and parents but not element data.
    let individual = individual_flags(roots);
    let occurrence = occurrence_type_flags(roots);
    let mut out = Vec::new();
    walk(roots, |el| {
        end_modifier_issue(el, index, &mut out);
        individual_usage_issue(el, index, &individual, &mut out);
        portion_kind_issue(el, index, &occurrence, &mut out);
    });
    out
}

// -- validateFeatureEndNotDerivedAbstractCompositeOrPortion ------------------

/// `isEnd implies not (isDerived or isAbstract or isComposite or isPortion)`.
fn end_modifier_issue(el: &Element, index: &Index, out: &mut Vec<Violation>) {
    if !el.is_end {
        return;
    }
    let written = forbidden_end_modifiers(el);
    if written.is_empty() {
        return;
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified_name(el, index),
        format!(
            "an `end` feature must be neither derived, abstract, composite nor a portion, but it is \
             written {}",
            join_words(&written)
        ),
    ));
}

/// The forbidden modifiers an `end` wrote, in the order the OCL names them.
///
/// `snapshot` and `timeslice` count as `isPortion` alongside `portion`; the module note says why.
fn forbidden_end_modifiers(el: &Element) -> Vec<&'static str> {
    let mut written = Vec::new();
    if el.modifiers.is_derived {
        written.push("`derived`");
    }
    if el.modifiers.is_abstract {
        written.push("`abstract`");
    }
    if el.modifiers.is_composite {
        written.push("`composite`");
    }
    match el.modifiers.portion.as_deref() {
        Some("portion") => written.push("`portion`"),
        Some("snapshot") => written.push("`snapshot`"),
        Some("timeslice") => written.push("`timeslice`"),
        _ => {}
    }
    written
}

// -- validateOccurrenceUsageIndividualUsage ---------------------------------

/// `isIndividual implies individualDefinition <> null`.
fn individual_usage_issue(
    el: &Element,
    index: &Index,
    individual: &HashMap<usize, bool>,
    out: &mut Vec<Violation>,
) {
    if !el.modifiers.is_individual {
        return;
    }
    if !el.kind.is_metaclass("OccurrenceUsage") {
        // Not an `OccurrenceUsage`. This is what keeps `individual part def Wheel`, which lowers to
        // `Kind::PartDef` with `isIndividual = true`, out of the rule: a definition is an
        // `OccurrenceDefinition`, not an `OccurrenceUsage`.
        return;
    }
    // A supertype makes `type` include the types of the features it subsets
    // (`deriveFeatureType`, generated.rs:8225-8227), so an individual definition can arrive through
    // the inheritance and this rule cannot be decided.
    if !el.supertype_list().is_empty() {
        return;
    }
    let Some(stated) = &el.type_ref else {
        if collapses_definition_and_usage(&el.kind) {
            // `item def I` and `item i : I` are the same element, and a definition legitimately has
            // no type of its own. The "no type at all" branch cannot be decided for these kinds.
            // The branch below can: a definition writes `:` as `:>` (`sysml-parser/src/parser.rs:
            // 1173-1177`), so a `type_ref` means the usage spelling was written.
            return;
        }
        out.push(Violation::new(
            CODE,
            el,
            qualified_name(el, index),
            "an `individual` occurrence usage must have an individual definition among its types, \
             but it declares no type"
                .to_string(),
        ));
        return;
    };
    let head = first_target(stated);
    let Some(target) = index.resolve(el.id, head).found() else {
        // The type does not resolve, so the reference pass is where that is reported. Assuming it
        // is not individual would claim a defect the model may not have.
        return;
    };
    if individual.get(&target).copied().unwrap_or(false) {
        return;
    }
    // The type is not individual, but the declaration may name an individual one elsewhere in the
    // same clause: `individual part c : Plain, Ind;` keeps only `Plain` in `type_ref`
    // (`lower.rs:326`, `ast.rs:211-216`). `ref_spans` covers the whole clause, so any target it
    // names that is individual leaves the consequent possibly non-null.
    for (text, _) in &el.ref_spans {
        let other = first_target(text);
        if other == head || other.is_empty() {
            continue;
        }
        let Some(other_id) = index.resolve(el.id, other).found() else {
            continue;
        };
        if individual.get(&other_id).copied().unwrap_or(false) {
            return;
        }
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified_name(el, index),
        format!(
            "an `individual` occurrence usage must have an individual definition among its types, \
             but `{}` is not an individual definition",
            index
                .qualified
                .get(target)
                .map(String::as_str)
                .unwrap_or(head)
        ),
    ));
}

/// `isIndividual` per element id, for the project's elements only.
///
/// A typing that resolves into the bundled library has no entry, and the rule stays silent about it
/// rather than reading the absence as `false`.
fn individual_flags(roots: &[Element]) -> HashMap<usize, bool> {
    let mut map = HashMap::new();
    fn go(el: &Element, map: &mut HashMap<usize, bool>) {
        // `individual def X` lowers to Kind::IndividualDef — the keyword is the kind, not a
        // prefix, so the modifier flag stays false. Both spellings mean the same thing.
        let by_kind = matches!(el.kind, sysml_model::Kind::IndividualDef);
        map.insert(el.id, el.modifiers.is_individual || by_kind);
        for child in &el.children {
            go(child, map);
        }
    }
    for root in roots {
        go(root, &mut map);
    }
    map
}

/// Whether an element is an `OccurrenceUsage` or an `OccurrenceDefinition` because of the words it
/// wrote, rather than because of the kind the projection assigned it.
///
/// The kind alone is not enough. An `individual` or a `snapshot` / `timeslice` usage may omit the
/// kind keyword — `SysML.xtext:833-841` (`OccurrenceUsagePrefix`), `:845-851` (`IndividualUsage`)
/// and `:853-857` (`PortionUsage`) all take a bare `Usage` after the prefix — so `snapshot
/// context_t0 : C;` parses with no kind keyword, falls through `declaration_kind()`
/// (`sysml-parser/src/parser.rs:859-895`) and lowers to [`Kind::AttributeUsage`]
/// (`sysml-syntax/src/lower.rs:158-163`). In the metamodel it is a `PortionUsage`, and a
/// `PortionUsage` is an `OccurrenceUsage` (`SysML.xtext:853-857`); a bare `individual reference : R;`
/// is an `IndividualUsage`, likewise an `OccurrenceUsage` (`:845-851`).
///
/// So a `Kind::AttributeUsage` owner carrying `portion` / `snapshot` / `timeslice` or `individual`
/// is an occurrence type, and the portion-kind rule must not report the usage it owns. The flag
/// makes the check quieter, never wrong: the words are the ones that put the element in the
/// occurrence hierarchy in the first place.
fn occurrence_type_flags(roots: &[Element]) -> HashMap<usize, bool> {
    let mut map = HashMap::new();
    fn go(el: &Element, map: &mut HashMap<usize, bool>) {
        let written_occurrence_prefix = el.modifiers.is_individual
            || matches!(
                el.modifiers.portion.as_deref(),
                Some("snapshot" | "timeslice")
            );
        map.insert(el.id, written_occurrence_prefix);
        for child in &el.children {
            go(child, map);
        }
    }
    for root in roots {
        go(root, &mut map);
    }
    map
}

// -- validateOccurrenceUsagePortionKind ------------------------------------

/// `portionKind <> null implies owningType <> null and (owningType is an OccurrenceDefinition or an
/// OccurrenceUsage)`.
fn portion_kind_issue(
    el: &Element,
    index: &Index,
    occurrence: &HashMap<usize, bool>,
    out: &mut Vec<Violation>,
) {
    // `portionKind` is written only by `snapshot` and `timeslice`; `portion` is `isPortion`.
    let Some(word) = el.modifiers.portion.as_deref() else {
        return;
    };
    if word != "snapshot" && word != "timeslice" {
        return;
    }
    if is_definition_only(&el.kind) {
        // `portionKind` is declared on `OccurrenceUsage` only (`generated.rs:2444`), so for a
        // definition the antecedent's property does not exist and the rule does not apply. A word
        // the parser read onto a definition came from a prefix run the grammar does not allow.
        return;
    }
    if collapses_definition_and_usage(&el.kind) && el.type_ref.is_none() {
        // The kind stands for both spellings, and with no type clause the two are the same element:
        // `item def I;` (a definition, which has no `portionKind`) and `item i;` (a `PortionUsage`)
        // leave identical trees, because the definition spelling turns `:` into `:>` and writes no
        // type at all (`sysml-parser/src/parser.rs:1173-1177`). Only the usage spelling is judged,
        // and only when a type clause proves it.
        return;
    }
    let Some(owner) = index.parent.get(el.id).copied().flatten() else {
        out.push(Violation::new(
            CODE,
            el,
            qualified_name(el, index),
            format!(
                "a `{word}` usage states a portion kind, so it must be owned by an occurrence \
                 definition or occurrence usage, but nothing owns it"
            ),
        ));
        return;
    };
    if !is_type_like(&index.kind[owner]) {
        // A package is a `Namespace`, not a `Type`, so a feature it owns has a null `owningType`
        // (`KerML.kerml:175`). The OCL's `owningType <> null` conjunct fails.
        let owner_name = index
            .qualified
            .get(owner)
            .map(String::as_str)
            .unwrap_or_default();
        out.push(Violation::new(
            CODE,
            el,
            qualified_name(el, index),
            format!(
                "a `{word}` usage states a portion kind, so it must be owned by an occurrence \
                 definition or occurrence usage, but `{owner_name}` is a {} and owns no type",
                index.kind[owner]
            ),
        ));
        return;
    }
    if is_occurrence_type(&index.kind[owner]) || occurrence.get(&owner).copied().unwrap_or(false) {
        return;
    }
    out.push(Violation::new(
        CODE,
        el,
        qualified_name(el, index),
        format!(
            "a `{word}` usage states a portion kind, so it must be owned by an occurrence \
             definition or occurrence usage, but its owner `{}` is a {}",
            index
                .qualified
                .get(owner)
                .map(String::as_str)
                .unwrap_or_default(),
            index.kind[owner]
        ),
    ));
}

// -- metaclass coverage ----------------------------------------------------

/// The disjunction the `owningType` test reads:
/// `owningType.oclIsKindOf(OccurrenceDefinition) or owningType.oclIsKindOf(OccurrenceUsage)`.
///
/// Asked of the metaclass hierarchy rather than of a list of kinds. The list this replaces had 69
/// lines and cited `generated.rs:1785` — the ancestor table — as its source; that table is now
/// compiled in, so the answer is read rather than restated, and it is right for the kinds the list
/// got wrong: `flow def` is a `Behavior` and an `Association`, not an occurrence type, and
/// `rendering def` is a `Structure`.
fn is_occurrence_type(kind: &Kind) -> bool {
    kind.is_metaclass("OccurrenceDefinition") || kind.is_metaclass("OccurrenceUsage")
}

/// Kinds that stand for both a definition and a usage spelling, so a modifier written on one of
/// them cannot be attributed to either.
///
/// Each is produced by one arm of `element_kind` that ignores `is_definition`
/// (`sysml-syntax/src/lower.rs:147-266`): `item def I` and `item i : I` are both [`Kind::ItemDef`]
/// (`:172`), `occurrence def O` and `occurrence o : O` are both [`Kind::OccurrenceDef`] (`:253`),
/// and the same holds for `case`, `analysis case`, `use case`, `concern`, `view`, `viewpoint`,
/// `rendering`, `allocation`, `interface` and `flow`.
///
/// Only the `individual` rule consults this. The portion-kind rule does not need it, because no
/// definition production admits a portion kind.
fn collapses_definition_and_usage(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::CaseDef
            | Kind::AnalysisCaseDef
            | Kind::UseCaseDef
            | Kind::ConcernDef
            | Kind::ViewDef
            | Kind::ViewpointDef
            | Kind::RenderingDef
            | Kind::AllocationDef
            | Kind::InterfaceDef
            | Kind::ItemDef
            | Kind::OccurrenceDef
            | Kind::Flow
    )
}

// -- helpers ---------------------------------------------------------------

/// A reference text with any multiplicity bracket and conjugation tilde removed, so
/// `Wheel[2]` and `~FuelPort` are looked up as the names they are.
fn first_target(text: &str) -> &str {
    let trimmed = text.trim();
    let trimmed = trimmed.strip_prefix('~').unwrap_or(trimmed);
    match trimmed.find('[') {
        Some(at) => trimmed[..at].trim_end(),
        None => trimmed,
    }
}

/// Kinds that can only be a definition, never a usage.
///
/// Written out rather than taken from [`Kind::is_definition`] (`sysml-model/src/lib.rs:227-256`),
/// because that table has two entries a usage declaration produces: `ConstraintUsage` (`:237`) and
/// `Variant` (`:239`, which is the `variant part x` membership). It also names the collapsed pairs
/// (`ItemDef`, `OccurrenceDef`, `CaseDef`, `ViewDef`, …), which stand for a usage just as well.
fn is_definition_only(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::Package
            | Kind::PartDef
            | Kind::PortDef
            | Kind::AttributeDef
            | Kind::RequirementDef
            | Kind::VerificationDef
            | Kind::ActionDef
            | Kind::ConstraintDef
            | Kind::CalcDef
            | Kind::StateDef
            | Kind::EnumDef
            | Kind::IndividualDef
            | Kind::MetadataDef
            | Kind::KermlType
            | Kind::ConnectionDef
    )
}

/// Kinds that are a `Type` in the metamodel, which is what `owningType` requires.
///
/// `owningType` is `subsets owningNamespace, featuringType` and typed `Type[0..1]`
/// (`KerML.kerml:175`), so a feature owned by a `Package` or a `RootNamespace` has a null
/// `owningType` — a package is a `Namespace`, not a `Type` (`generated.rs:1785`). That distinction
/// is what makes the OCL's `owningType <> null` conjunct meaningful.
fn is_type_like(kind: &Kind) -> bool {
    // [`Kind::is_type`](sysml_model::Kind::is_type) asks the same question; this used to be a second
    // list of the namespace kinds, and the two drifted.
    kind.is_type()
}

/// Join modifiers as prose: `a`, `a and b`, `a, b, and c`.
fn join_words(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [only] => only.to_string(),
        [first, second] => format!("{first} and {second}"),
        [head, tail @ ..] => format!("{}, and {}", head, tail.join(", ")),
    }
}

/// The qualified name of an element, falling back to its own name for an element the index does not
/// know.
fn qualified_name<'a>(el: &'a Element, index: &'a Index) -> &'a str {
    index
        .qualified
        .get(el.id)
        .map(String::as_str)
        .unwrap_or(el.name.as_str())
}
