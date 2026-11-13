//! `variation-rules` — variation points: their abstractness, the members they may own, the
//! specialization they may not state, and whether an enumeration definition is one.
//!
//! Seven rules were assigned to this group (`data/spec_rules.rs`, the rule's own `name` /
//! `metaclass` / `ocl`):
//!
//!   * `validateUsageVariationIsAbstract` (`:917`) — `isVariation implies isAbstract`, asked of
//!     `Usage`;
//!   * `validateUsageVariationOwnedFeatureMembership` (`:923`) —
//!     `isVariation implies ownedFeatureMembership->isEmpty()`, asked of `Usage`;
//!   * `validateUsageVariationSpecialization` (`:929`) — a variation `Usage` may not specialize a
//!     variation `Definition` or `Usage`;
//!   * `validateDefinitionVariationIsAbstract` (`:959`),
//!     `validateDefinitionVariationOwnedFeatureMembership` (`:965`),
//!     `validateDefinitionVariationSpecialization` (`:953`) — the same three asked of
//!     `Definition`;
//!   * `validateEnumerationDefinitionIsVariation` (`:539`) — `isVariation`, asked of
//!     `EnumerationDefinition`.
//!
//! The report code this group reports under is `SEM032`.
//!
//! # Nothing in this group is decided here, and the reason is no longer a missing field
//!
//! `Element` now carries the flags these OCL bodies name. `Modifiers`
//! (`sysml-model/src/lib.rs:876-890`) holds `is_abstract` (`:878`) and `is_variation` (`:879`),
//! and the lowering copies them off the authored prefixes (`sysml-syntax/src/lower.rs:343-358`,
//! `is_abstract` at `:344`, `is_variation` at `:345`). So the six rules that read those two flags
//! are no longer blocked by the projection, and the older note in `rules/variation_modifiers.rs`
//! (`:22-44`, which says neither field exists) is out of date: commit `0f22f92` added them. What
//! blocks each rule now is stated below, per rule, and it is a different fact in each case.
//!
//! # `validate*VariationIsAbstract`: the notation cannot state the violation, and the rule is
//! vacuous once the model is built as the specification says
//!
//! `isVariation implies isAbstract` is asked of the *abstract syntax*. The specification's own
//! notation clause settles what that abstract syntax holds for a variation: "A definition or usage
//! is specified as a variation by placing the keyword `variation` before its kind keyword. A
//! variation is always abstract, so the `abstract` keyword is not used on a variation."
//! (`research/corpus/sysml2-spec.pdf`, PDF page 74 / printed page 43, §7.6.7.)
//!
//! The grammar agrees: both prefix productions make the two words alternatives, never a pair —
//! `BasicDefinitionPrefix` is `isAbstract ?= 'abstract' | isVariation ?= 'variation'`
//! (`research/upstream/SysML.xtext:491`) and `RefPrefix` is
//! `( isAbstract ?= 'abstract' | isVariation ?= 'variation')?` (`:559`). The notation table lists
//! them as separate rows (`«abstract part» abstract part part1 :` and
//! `«variation part» variation part part1 :`, spec PDF page 66).
//!
//! So a conforming abstract-syntax model of `variation part p` has `isVariation = true` and
//! `isAbstract = true`, and the reference builder sets exactly that, from the stated flag to the
//! implied one (`research/upstream/mod/sysmlv2-model-0.1.2/src/build.rs:233-237`). Our element
//! tree does not: `lower.rs:344` copies the *authored* `is_abstract`, which is `false` for every
//! legal variation. A check reading the tree therefore reports:
//!
//!   * the specification's own variability example, `variation part def TransmissionChoices :>
//!     Transmission { … }` with no `abstract` (spec PDF page 687 / printed page 656);
//!   * the ten `variation part` / `variation requirement` declarations of the official validation
//!     suite (`research/upstream/release/sysml/src/validation/` — `07-Variant Configuration/
//!     7a-Variant Configuration - General Concept.sysml:14`, `:25`; `7a1-…-a.sysml:31`, `:42`;
//!     `7b-Variant Configurations.sysml:82`, `:87`, `:101`, `:120`, `:135`;
//!     `10-Analysis and Trades/10b-Trade-off Among Alternative Configurations.sysml:40`). The
//!     eleventh `variation` there is `variation perform action doXorY` (`7a1-…-a.sysml:14`), which
//!     loses the flag earlier still — see the next section.
//!
//! The other order — derive `isAbstract = true` from `isVariation` first, as the builder does —
//! makes the constraint hold of every element the tree can describe, i.e. a check that cannot
//! fail. `conformance.rs:13-15` forbids listing such a rule. Both readings are wrong, so the rule
//! is dropped rather than decided one way.
//!
//! Our parser is laxer than the grammar and accepts `abstract variation part p`, filling both
//! flags (`sysml-parser/src/parser.rs:251-321` reads a run of prefixes, and `:267-274` handles
//! `abstract` and `variation` in the same loop). That spelling is the only one the OCL can hold
//! of, and the grammar forbids it (`SysML.xtext:559`) — a laxity in our front end is not a legal
//! model. `tests/rules_variation_rules.rs` pins all of this.
//!
//! # `validate*VariationOwnedFeatureMembership`: the membership metaclass is not on the tree, and
//! the variant marker survives only one of the three spellings
//!
//! The consequent's own arithmetic is clear. `ownedFeatureMembership` is
//! `ownedRelationship->selectByKind(FeatureMembership)` (`generated.rs:8093-8096`), and a
//! `VariantMembership` is *not* one: it specializes `OwningMembership` (`generated.rs:1772`) and
//! `FeatureMembership` is its sibling under the same parent (`:1668`). That is why a legal
//! variation, which owns only variant memberships, satisfies
//! `ownedFeatureMembership->isEmpty()` at all.
//!
//! `Element::children` (`sysml-model/src/lib.rs:786`) cannot make that distinction. The tree
//! holds elements, not memberships: which member production introduced a child is not recorded
//! anywhere, and no field names a membership metaclass. Three consequences, each of which turns a
//! check over children into a false positive:
//!
//!   * **A performed variant loses the marker.** `variant perform doX;` is routed through
//!     `statement()` (`parser.rs:853`) to `behavior()` (`:3021-3051`), which takes no prefixes and
//!     returns `Kind::Perform`, so `is_variant` is not set on it. The enclosing `variation perform
//!     action doXorY` loses `isVariation` the same way, because the declaration's prefixes are
//!     consumed before `statement()` runs. `07-Variant Configuration/7a1-Variant Configuration -
//!     General Concept-a.sysml:14-17` is that construct, so even the variation point itself is
//!     invisible to these rules in that spelling.
//!
//!     The plain spellings are fixed: `variant part alt` keeps its kind and sets `is_variant`, and
//!     a bare `variant alt;` keeps `Kind::Variant`. Only the statement-routed spellings above
//!     still lose it.
//!   * **Some children are not memberships at all.** `inherit_ports`
//!     (`sysml-syntax/src/lower.rs:1344-1389`) copies a definition's ports onto every part usage
//!     typed by it and marks the copy `inherited`
//!     (`sysml-model/src/lib.rs:797`, push at `lower.rs:1377`). A variation typed by a definition
//!     therefore owns children no relationship backs — counting them as feature memberships would
//!     report the projection, not the model.
//!
//! A sound but incomplete reading does exist, and it is not enough. Most children of these bodies
//! *are* provably owned feature memberships: every usage-member production returns a
//! `FeatureMembership` (`SysML.xtext:534-548`), and so do the parameter, subject, frame,
//! state-subaction and constraint memberships through
//! `ParameterMembership specializes FeatureMembership` (`generated.rs:1723`) and its own subtypes
//! (`SubjectMembership` `:1755`, `RequirementConstraintMembership` `:1738` with
//! `FramedConcernMembership` under it at `:1678`, `StateSubactionMembership` `:1750`,
//! `ReturnParameterMembership` `:1743`). What is not provable is the *other* direction — that a
//! child is exempt. A `VariantMembership` is exempt (`generated.rs:1772`), and nothing in the tree
//! says a child came from one.
//!
//! The model that breaks the rule in the plainest way — `variation part cfg { attribute x : T; }`,
//! the shape `tests/rules_variation_modifiers.rs:236-251` records — has an `AttributeUsage` child.
//! So does the legal bare variant spelling above, and so does the official suite's
//! `variation requirement engineRqtChoice` (`7b-Variant Configurations.sysml:82-85`). A check that
//! reported every such child would reject the specification's own validation models; one narrowed
//! until it is safe would have to skip the very case the rule is about. Both rules are dropped
//! whole.
//!
//! # `validate*VariationSpecialization`: the OCL as written is self-contradictory
//!
//! Both bodies quantify over `ownedSpecialization.specific`, and `specific` on a
//! `Specialization` is the type doing the specializing (`generated.rs:2556-2560`, where `specific`
//! redefines `source`). `ownedSpecialization` is itself filtered to the specializations whose
//! specific type is the element (`generated.rs:8117-8120`, `select(s | s.special = self)` — the
//! text says `special`, a property no metaclass declares). So `ownedSpecialization.specific` is
//! always the element the rule is asked of, and
//!
//! ```text
//! isVariation implies not ownedSpecialization.specific->exists(P)
//! ```
//!
//! reduces to `isVariation implies not P(self)`. With `P` = "is a `Definition` that is a variation
//! or a `Usage` that is a variation" (usage rule) or "is a `Definition` that is a variation"
//! (definition rule), `P(self)` holds exactly when `self` is a variation, so both reduce to
//! `not isVariation` — they reject every variation.
//!
//! The prose beside the OCL is about `general`, the *specialized* end, which the OCL never names.
//! The upstream semantics layer records the same conflict and runs neither
//! (`research/upstream/sem/sysmlv2-semantics-0.1.2/src/rules.rs:321-323`, and the mis-written list
//! at `:396-428`, naming `validateDefinitionVariationSpecialization` at `:422-423` and
//! `validateUsageVariationSpecialization` at `:426-427`). Implementing the prose instead would mean
//! deciding a rule the specification does not state; implementing the text would reject every
//! variation. The two rules are dropped, and the erratum belongs upstream.
//!
//! # `validateEnumerationDefinitionIsVariation`: constant true on the metaclass
//!
//! `EnumerationDefinition::isVariation` redefines the inherited property with the declared default
//! `true` (`generated.rs:2205`), and the specification states the same in words twice: "An
//! `EnumerationDefinition` is considered semantically to be a variation whose allowed variants are
//! its `enumerationValue`s" (spec PDF page 311 / printed page 280, §8.3.8.2), and "An enumeration
//! definition is always considered to be a variation … The keywords `abstract` and `variation` may
//! not be used with an enumeration definition" (PDF page 79 / printed 48, §7.8.2).
//!
//! The notation has no slot for the keyword either: `EnumerationDefinition` is
//! `DefinitionExtensionKeyword* EnumerationDefKeyword DefinitionDeclaration EnumerationBody`
//! (`research/upstream/SysML.xtext:767-770`), with no `BasicDefinitionPrefix` — so `enum def M` is
//! the only form and its `isVariation` is the default `true`. A constraint whose body is that one
//! property therefore holds of every enumeration definition a model can write.
//!
//! Our tree stores the *authored* prefix, and the lowering applies no defaults (`lower.rs:343-358`
//! copies the flag and nothing else), so `enum def M { a; b; }` carries
//! `modifiers.is_variation == false` — the opposite of the metaclass. A check against the tree's
//! flag would reject every enumeration definition, including the specification's own example
//! `enum def EnumerationDef1 { enum1; enum2; }` (PDF page 79 / printed 48); a check that applies
//! the default first can never fail. Dropped.
//!
//! # What would unblock the group
//!
//! In the order the rules need it:
//!
//!   * the owning membership's metaclass (at least `FeatureMembership` vs `VariantMembership` vs
//!     the rest) recorded per child, and `inherit_ports`' copies not presented as members;
//!   * the `variant` marker kept for the bare and the `perform`/`exhibit` spellings, not only for
//!     the kind-keyword one;
//!   * a decision, in the projection, on whether the metamodel defaults (`generated.rs:2205`) and
//!     the notation's own implications (spec §7.6.7) are applied — until they are, the two flags
//!     mean "as authored", which is not what any of these OCL bodies reads;
//!   * the `specific`/`general` erratum settled upstream before either specialization rule is run.

use sysml_model::Element;
use sysml_resolve::Index;

use super::Violation;

/// The `SEM` code this group reports under.
///
/// Nothing is reported today — see the module note for why each of the seven rules is dropped. The
/// code is reserved here so a check that is written later reports under the group's own code
/// rather than borrowing another group's.
pub const CODE: &str = "SEM032";

pub fn issues(_roots: &[Element], _index: &Index) -> Vec<Violation> {
    // Every rule assigned to this group is undecided, for the reasons the module note gives per
    // rule: the two `IsAbstract` rules cannot be violated by any model the notation can express
    // and are vacuous once the abstract syntax is built as §7.6.7 says; the two
    // `OwnedFeatureMembership` rules need the membership metaclass the tree does not carry, and the
    // variant marker survives only one of the three spellings; the two `Specialization` rules as
    // written reduce to `not isVariation`; and `validateEnumerationDefinitionIsVariation` is a
    // constant `true` on its metaclass.
    //
    // Reporting nothing is the correct answer here. A check standing in for any of these would
    // either reject legal models — the specification's own variability example, the official
    // validation suite's variation declarations, every enumeration definition — or list a rule that
    // cannot fail. `tests/rules_variation_rules.rs` pins the fact behind each drop, so the omission
    // is recorded rather than silent.
    Vec::new()
}
