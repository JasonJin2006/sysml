//! `port-attribute` — whether a port or an attribute is referential, what the members a port or
//! an attribute owns may be, and which modifiers an `end` feature carries.
//!
//! Ten rules were handed to this group. Their `name`, `metaclass` and `ocl` are in
//! `data/spec_rules.rs` at the line cited per rule:
//!
//!   * `validatePortUsageIsReference` (1025) —
//!     `owningType = null or not owningType.oclIsKindOf(PortDefinition) and not
//!     owningType.oclIsKindOf(PortUsage) implies isReference`;
//!   * `validateAttributeUsageIsReference` (977) — `isReference`;
//!   * `validatePortUsageNestedUsagesNotComposite` (1019) —
//!     `nestedUsage->reject(oclIsKindOf(PortUsage))->forAll(not isComposite)`;
//!   * `validatePortDefinitionOwnedUsagesNotComposite` (1007) — the same over `ownedUsage`;
//!   * `validateAttributeUsageFeatures` (983) and `validateAttributeDefinitionFeatures` (971) —
//!     `feature->forAll(not isComposite)`;
//!   * `validateFeatureEndIsConstant` (383) — `isEnd and isVariable implies isConstant`;
//!   * `validateFeatureIsVariable` (389) —
//!     `isVariable implies owningType <> null and
//!     owningType.specializes('Occurrences::Occurrence')`;
//!   * `validateFeatureConstantIsVariable` (419) — `isConstant implies isVariable`;
//!   * `validateFeaturePortionNotVariable` (425) — `isPortion implies not isVariable` (the
//!     specification writes no prose sentence for this one; `says` is empty).
//!
//! The report code reserved for this group is `SEM022`.
//!
//! # What the element tree carries
//!
//! [`Element`]'s field list is `sysml-model/src/lib.rs:721-850`. The feature-level facts it
//! carries are `direction` (`:777`), `multiplicity` (`:843`), `inherited` (`:786`, "copied from a
//! type rather than written here") and `is_end` (`:849`). There is no `is_reference`,
//! `is_composite`, `is_variable`, `is_constant` or `is_portion`, and no element that is a
//! `ReferenceUsage`.
//!
//! The notation states the modifiers these five properties answer to, and the parser collects
//! four of them into `Prefixes` (`sysml-parser/src/ast.rs:155-169`): `is_constant` (`:161`),
//! `is_end` (`:163`), `is_ref` (`:164`) and `portion` (`:166`), set at
//! `sysml-parser/src/parser.rs:292-294` (`constant`), `:300-303` (`end`), `:304-306`
//! (`composite` / `portion` / `snapshot` / `timeslice`) and `:307-310` (`ref`). `var`, the
//! spelling for `isVariable`, is not collected at all (see below).
//!
//! The lowering layer reads exactly two prefix fields — `is_end` (`sysml-syntax/src/lower.rs:281`,
//! `:338`) and `direction` (`:298`, `:391`). No other prefix is named anywhere in `lower.rs`. So
//! `composite port p : PD`, `ref port p : PD` and `port p : PD` produce the same [`Element`], and
//! likewise `constant attribute k` and `attribute k`, `portion part q` and `part q`.
//!
//! # `composite` and `portion` would not be separable even with a field
//!
//! The grammar keeps them apart — `( isComposite ?= 'composite' | isPortion ?= 'portion' )`
//! (`research/upstream/KerML.xtext:518`). The parser folds both into `Prefixes.portion`
//! (`sysml-parser/src/parser.rs:304-306`). Propagating that field as-is would give the five
//! `isComposite` rules and `validateFeaturePortionNotVariable` one shared, ambiguous input; a
//! check would have to read `portion` for `composite` and guess which one was written.
//!
//! # `var` is not collected at all
//!
//! The general prefix run (`sysml-parser/src/parser.rs:251-321`) has no `var` arm. `var x : Real`
//! therefore lowers to a feature named `var` followed by an unrelated `x`, as
//! `tests/rules_port_attribute.rs` records. The one other place the parser accepts the word is the
//! modifier run of a directed parameter (`parser.rs:607-612`), where it is discarded unread; a
//! lambda parameter does not accept it at all (`parser.rs:1538-1540`).
//!
//! # Why the missing properties are not defaulted
//!
//! Two shortcuts would make rules look enforced while deciding nothing:
//!
//!   * Taking `isComposite`'s stored default. The metamodel declares it
//!     `derived: false, default: Some(false)` (`generated.rs:2234`) and derives
//!     `isReference = not isComposite` (`deriveUsageIsReference`, `generated.rs:8957-8961`). Take
//!     both and `isReference` is `true` for every element the tree can describe, so
//!     `validateAttributeUsageIsReference` — whose OCL is the single word `isReference` — can
//!     never fail. A rule whose check cannot fail must not be listed as enforced.
//!   * Reading the notation instead of the model. The reference implementation treats an
//!     ownerless-in-notation `part wheel;` as composite unless it says `ref`
//!     (`research/upstream/sem/mod/sysmlv2-model-0.1.2/src/build.rs:2257-2284`), which is the
//!     opposite of the metamodel's default. Two readings of the same spelling disagree, so the
//!     tree holds no answer and picking one would decide the rules on a convention.
//!
//! # Per rule: which half is decidable
//!
//!   * `validatePortUsageIsReference` — the antecedent mostly is. `Index::parent` gives the
//!     owning element and `Index::kind` its kind, so `owningType = null`,
//!     `oclIsKindOf(PortDefinition)` and `oclIsKindOf(PortUsage)` can be answered from the kind of
//!     the owner (which is *not* the same as `owningType`, the nearest owning `Type`; for the
//!     ordinary cases — a port in a part def, a port in a port, a root-level port — the two
//!     coincide, and a port owned by a package is `owningType = null`). The consequent
//!     `isReference` is not on the tree, so the rule is undecidable regardless.
//!   * `validateAttributeUsageIsReference` — the OCL is `isReference` alone, with no antecedent to
//!     fall back on. The metaclass also derives it with default `true`
//!     (`generated.rs:2031-2034`) and the reference builder forces every `AttributeUsage`
//!     non-composite (`build.rs:2250-2256`), so even with the field there is no expressible
//!     violation in that reading.
//!   * `validatePortUsageNestedUsagesNotComposite`, `validatePortDefinitionOwnedUsagesNotComposite`,
//!     `validateAttributeUsageFeatures`, `validateAttributeDefinitionFeatures` — the selection
//!     half is decidable: `nestedUsage = ownedFeature->selectByKind(Usage)`
//!     (`deriveUsageNestedUsage`, `generated.rs:8879-8882`) and `feature` / `ownedUsage` are the
//!     element's `children`, whose kinds `lower.rs` does distinguish
//!     (`element_kind`, `lower.rs:147-171`, maps `port def` / `port` to `Kind::PortDef` /
//!     `Kind::PortUsage`). The consequent `not isComposite` on the selected children is not
//!     readable. A check that selected them and asserted nothing would report every port that
//!     owns an attribute.
//!   * `validateFeatureEndIsConstant` — `is_end` is a field (`sysml-model/src/lib.rs:849`), so the
//!     first conjunct is readable. `isVariable` and `isConstant` are not.
//!   * `validateFeatureIsVariable` — the reverse: the owner is readable through
//!     `Element::children` / `Index::parent`, and the direct supertypes of the owner through
//!     `Index::debug_supertypes` (`sysml-resolve/src/lib.rs:303`), which wraps a private
//!     one-level walk. A transitive `specializes('Occurrences::Occurrence')` would have to be
//!     iterated from that. The antecedent `isVariable` is absent either way, so the rule is not
//!     decidable.
//!   * `validateFeatureConstantIsVariable` — `isConstant` and `isVariable`, both absent.
//!   * `validateFeaturePortionNotVariable` — `isPortion` and `isVariable`, both absent, and the
//!     portion keywords are dropped before either could be read.
//!
//! # Nothing in this group is enforced
//!
//! `issues` reports no violation, and `conformance::implemented()` must list none of the ten. Each
//! rule is one field away, and the field has to arrive through `lower.rs` — not through a
//! heuristic here, which would either pass every model that breaks the rule or reject models that
//! do not. `tests/rules_port_attribute.rs` records a violating model per rule, asserts nothing is
//! reported, and pins the lowering facts the drop rests on, so the omission is a recorded fact
//! rather than a silence.

use sysml_model::Element;
use sysml_resolve::Index;

use super::Violation;

pub fn issues(_roots: &[Element], _index: &Index) -> Vec<Violation> {
    // Every rule assigned to this group reads `isReference`, `isComposite`, `isVariable`,
    // `isConstant` or `isPortion`, and none of the five survives lowering (see the module note).
    // Returning nothing is the correct answer: any heuristic standing in for one of those
    // properties would either pass every model that breaks the rule or reject models that do not,
    // and both are worse than saying the rule is not decided here.
    Vec::new()
}
