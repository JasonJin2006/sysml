//! `variation-modifiers` — variation points, their alternatives, and referential usages.
//!
//! Eight rules were assigned to this group (`data/spec_rules.json`, the rule's own `name` /
//! `metaclass` / `ocl`):
//!
//!   * `validateUsageVariationIsAbstract` (line 915) — `isVariation implies isAbstract`;
//!   * `validateUsageVariationOwnedFeatureMembership` (921) —
//!     `isVariation implies ownedFeatureMembership->isEmpty()`;
//!   * `validateUsageVariationSpecialization` (927) — a variation `Usage` may not specialize a
//!     variation `Definition` or `Usage`;
//!   * `validateDefinitionVariationSpecialization` (951), `validateDefinitionVariationIsAbstract`
//!     (957), `validateDefinitionVariationOwnedFeatureMembership` (963) — the same three on
//!     `Definition`;
//!   * `validateUsageIsReferential` (933) —
//!     `direction <> null or isEnd or featuringType->isEmpty() implies isReference`;
//!   * `validateReferenceUsageIsReference` (939) — `isReference`, on `ReferenceUsage`.
//!
//! The report code reserved for this group is `SEM024`.
//!
//! # Nothing in this group is decided here
//!
//! The eight rules read three properties: `isVariation`, `isAbstract` and `isReference`. None is
//! a field of [`Element`]. The struct is `sysml-model/src/lib.rs:721-850`; the only feature-level
//! facts on it are `direction` (`:777`), `multiplicity` (`:843`) and `is_end` (`:849`).
//! `featuringType`, which `validateUsageIsReferential` also reads, is absent too — the
//! metamodel states it as derived at `research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:2238`
//! (declared `derived var feature featuringType` in
//! `research/.venv/Lib/site-packages/sysmlpy/library/kernel/KerML.kerml:182`), and the tree has no
//! counterpart.
//!
//! The modifiers are not missing from the front end. They are parsed and then dropped:
//!
//!   * the parser collects them into `Prefixes.is_abstract` (`sysml-parser/src/ast.rs:157`),
//!     `Prefixes.is_variation` (`:158`) and `Prefixes.is_ref` (`:164`), set at
//!     `sysml-parser/src/parser.rs:267-270`, `:271-274` and `:307-310`, plus `:609` for a
//!     directed parameter and `:1033-1036` for a `ref` written after the kind keyword;
//!   * the lowering layer reads two prefix fields and no others — `is_end`
//!     (`sysml-syntax/src/lower.rs:281`, `:338`) and `direction` (`:298`, `:391`). A grep for
//!     `prefix` in `lower.rs` returns those four lines and nothing else.
//!
//! So `variation part config` and `part config` lower to the same [`Element`], and `ref r : Base`
//! lowers to `Kind::AttributeUsage` exactly as `attribute r : Base` does.
//!
//! # `isReference` cannot be reconstructed from what the tree does carry
//!
//! `isReference` is derived: `deriveUsageIsReference: isReference = not isComposite`
//! (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:8957-8961`). `isComposite` is a
//! stored `Feature` attribute (`research/.venv/Lib/site-packages/sysmlpy/library/kernel/KerML.kerml:167`)
//! with a declared default of `false` (`generated.rs:2234`), and lowering never writes it.
//! Both readings available to a checker are wrong:
//!
//!   * take the declared default, and `isReference` comes out `true` of every element the tree
//!     can describe, so `validateUsageIsReferential` could never fail. A rule whose check cannot
//!     fail must not be listed as enforced — that is the rule `conformance.rs:13-15` states.
//!   * read `part` / `attribute` as composite unless written `ref` (the convention the reference
//!     implementation applies), and the tree still cannot tell `ref part p` from `part p`, because
//!     the modifier is discarded before the element is built.
//!
//! Writing `composite` does not help either. The parser files that keyword under
//! `Prefixes.portion` — the same field as `portion` / `snapshot` / `timeslice`
//! (`parser.rs:304-305`) — and lowering drops the field, so the one spelling that states
//! `isComposite` positively leaves no trace in the tree.
//!
//! # `ReferenceUsage` has no element
//!
//! `validateReferenceUsageIsReference` is asked of the `ReferenceUsage` metaclass, and no element
//! in the tree is one. `Kind::Ref` is declared with exactly that meaning
//! (`sysml-model/src/lib.rs:47-48`) and appears in the keyword and group tables (`:168`, `:267`),
//! but no lowering path constructs it: `sysml-syntax/src/lower.rs` contains no occurrence.
//!
//! The metaclass also makes the rule structurally true. `ReferenceUsage` redefines `isReference`
//! with the default `true` (`generated.rs:2491-2493`; the library states it at
//! `research/.venv/Lib/site-packages/sysmlpy/library/systems/SysML.sysml:296-298`), so a `ref`
//! declaration cannot be non-referential by construction: even after the metaclass existed there
//! would be no model for this rule to reject.
//!
//! # The two specialization rules are mis-stated, independently of the missing field
//!
//! Their OCL reads `ownedSpecialization.specific`, the specializing element — which is the element
//! the rule is asked of — while the prose says "may not specialize any variation", which is about
//! `general`. Read literally, `isVariation implies not ownedSpecialization.specific->exists(isVariation)`
//! reduces to `not isVariation`, so it rejects every well-formed variation. The upstream semantics
//! layer records the same conflict (`research/upstream/sem/sysmlv2-semantics-0.1.2/src/rules.rs:321`)
//! and lists both rules as mis-written (`:419-428`); the same note reports that the literal reading
//! flags the corpus's 17 variation usages and 16 variation definitions. Deciding either rule would
//! need the `general` end, which the OCL does not name, and the antecedent `isVariation` besides.
//!
//! # Why the variant flag is not a stand-in for `isVariation`
//!
//! Two of the rules are about the members a variation point owns, so a check could be aimed at a
//! variant child instead of at the variation point. `variant` does reach the tree, as
//! `Element::is_variant` (`sysml-syntax/src/lower.rs`). Using it as the antecedent would be unsound
//! in the direction that matters: `isVariation` is a stored attribute written by the `variation`
//! keyword (`systems/SysML.sysml:441` for `Usage`, `:128` for `Definition`; default `false` at
//! `generated.rs:2128` and `:2704`), not derived from variant memberships. So
//! `variation part config { attribute x : Real; }` violates
//! `validateUsageVariationOwnedFeatureMembership` and owns no variant member at all. A check that
//! fired only when an alternative was present would list the rule as enforced while passing a
//! model that breaks it.
//!
//! The converse inference fails too: `variant part alt : T;` written under a plain `part` is the
//! subject of `validateVariantMembershipOwningNamespace` (`data/spec_rules.json:945-949`), a
//! different rule that is not among the eight assigned here and itself needs `isVariation` on the
//! owning namespace.
//!
//! # Per rule
//!
//! * `validateUsageVariationIsAbstract`, `validateDefinitionVariationIsAbstract`: need
//!   `isVariation` and `isAbstract`. Neither is on [`Element`].
//! * `validateUsageVariationOwnedFeatureMembership`,
//!   `validateDefinitionVariationOwnedFeatureMembership`: the consequent is readable — the owned
//!   members are `Element::children` — but the antecedent is not, and no reading of the members
//!   recovers it (see the `Kind::Variant` note above).
//! * `validateUsageVariationSpecialization`, `validateDefinitionVariationSpecialization`: the
//!   specialized targets are readable (`Element::supertypes`), but both the antecedent and the
//!   `exists(...)` side need `isVariation`; and the OCL is mis-stated for both.
//! * `validateUsageIsReferential`: `direction` (`sysml-model/src/lib.rs:777`) and `is_end`
//!   (`:849`) make two of the antecedent's three disjuncts readable, and neither can be turned
//!   into a violation — the reference implementation derives `isComposite = false`, hence
//!   `isReference = true`, for exactly those features
//!   (`research/upstream/sem/mod/sysmlv2-model-0.1.2/src/build.rs:2242-2249`). The third
//!   disjunct `featuringType->isEmpty()` is derived (`generated.rs:2238`) and has no field, and
//!   neither does the consequent `isReference` (`generated.rs:8957-8961`).
//! * `validateReferenceUsageIsReference`: no `ReferenceUsage` element exists, and `isReference` is
//!   absent besides. The metaclass also makes the rule structurally true — see below.

use sysml_model::Element;
use sysml_resolve::Index;

use super::Violation;

pub fn issues(_roots: &[Element], _index: &Index) -> Vec<Violation> {
    // Every rule assigned to this group reads `isVariation`, `isAbstract` or `isReference`, and
    // none of the three survives lowering (see the module note). Reporting nothing is the correct
    // answer: a heuristic standing in for one of those properties would either pass models that
    // break the rule or reject models that do not, and both are worse than saying the rule is not
    // decided here. `tests/rules_variation_modifiers.rs` records a violating model per rule and
    // asserts nothing is reported, so the drop is a recorded fact rather than an omission.
    Vec::new()
}
