//! The `feature-multiplicity` rule group: what bound an `end` feature may state.
//!
//! Eight rules were handed to this module. One is decided here; the other seven read properties
//! the element tree does not carry, and each is named below with the field or metaclass it is
//! missing. A rule whose OCL reaches for something the tree lacks cannot be answered, and
//! claiming otherwise is the defect this layer exists to prevent.
//!
//! # The one rule this module decides
//!
//! `validateFeatureEndMultiplicity` (`data/spec_rules.rs:455`) —
//! `isEnd implies multiplicities().allSuperTypes()->flatten()->selectByKind(MultiplicityRange)->
//! exists(hasBounds(1,1))`, "If a Feature has isEnd = true, then it must have multiplicity 1..1."
//! It is decided from [`Element::is_end`] (`sysml-model/src/lib.rs:849`) and
//! [`Element::multiplicity`] (`:843`).
//!
//! The OCL as written calls `multiplicities().allSuperTypes()`; `allSuperTypes()` is not in this
//! table (`grep allSuperTypes generated.rs` finds only call sites), so what that call contributes
//! is not settled from the files available here. What is implemented is the rule's own prose —
//! "If a Feature has isEnd = true, then it must have multiplicity 1..1" — which is also the
//! reading the reference implementation takes
//! (`research/upstream/sem/sysmlv2-semantics-0.1.2/src/reify.rs:891-910`, which supplies 1..1 for
//! every end). Two facts from the metamodel's own operation table
//! (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs`) shape the check:
//!
//!   * `hasBounds` (`:9420`) is `valueOf(upperBound) = upper and let lowerValue = valueOf(lowerBound)
//!     in (lowerValue = lower or lowerValue = null and (lower = upper or lower = 0 and upper = *))`.
//!   * `deriveMultiplicityRangeLowerBound` (`:7985`) returns null when the range has fewer than
//!     two owned expressions, and `deriveMultiplicityRangeUpperBound` (`:7979`) returns the first
//!     for one and the second for more than one. So `[1]` is upper = 1 with a **null** lower, and
//!     `hasBounds(1,1)` accepts it through the `lower = upper` branch. `[*]` is upper = `*` with a
//!     null lower, and fails the very first comparison. `[1..*]` is upper = `*`, failing likewise.
//!     `[*..1]` has a `*` lower, which is neither 1 nor null, so it fails the second disjunct.
//!
//! A bound written as a name (`[n]`) is left undecided: `valueOf` needs the bound to be
//! model-level evaluable, and deciding whether `n` is 1 needs the evaluator. Reporting a model
//! that may be right costs more than staying silent, so only the three shapes the bound text
//! settles are judged — an integer, `*`, and a pair of those.
//!
//! A feature with `isEnd` and no written bound is accepted. That is a deliberate reading, and it
//! is the one the reference implementation takes: `counts_one`
//! (`research/upstream/sem/sysmlv2-semantics-0.1.2/src/reify.rs:897-911`) materialises a 1..1
//! `MultiplicityRange` for every end whose range has no upper bound, and the doc comment above it
//! (`:891-893`) says "and the notation writes it nowhere". The shipped standard library is written
//! that way — see `core/stdlib/Systems Library/Connections.sysml:43-44`
//! and `Systems Library/Allocations.sysml:19-20`, both `end source: Anything` with no bound — so
//! rejecting a silent end would reject the library every project imports.
//!
//! # Known limit: owned cross features are mis-lowered, and this module inherits the error
//!
//! §7.13.2 splits a connection end that declares an owned cross feature into two declarations.
//! The specification's own example (the `HubDeviceConnection` figure, page 69 of
//! `research/corpus/sysml2-spec.pdf`) writes `end connectingHub [0..1] ordered part hub : Hub`
//! and displays the bound as the *cross* multiplicity, 1..1 being the end's.
//! `lower.rs` does not model the owned cross feature: `lower_declaration_in`
//! (`sysml-syntax/src/lower.rs:273-461`) never names one, marks the **cross feature** `is_end`
//! through `d.head.prefixes.is_end` (`:281`, `:338`), and leaves the real end unflagged, so the
//! two become flat siblings.
//!
//! On that construct this module reads a cross multiplicity as an end boundary and reports it.
//! The tree exposes no `ownedCrossFeature`, no `crosses` target on an end, and no field
//! separating the two bounds, and the two readings are structurally identical:
//! `end cross : X[0..1] part hub : Hub` (a cross bound; the end is implicitly 1..1) and
//! `end cross : X[0..1]; part hub : Hub;` (the bound is the end's own) lower to the same element.
//! A guard narrow enough to exclude the first also drops ordinary ends followed by another
//! member, which is the layout the specification's own example uses, so no sound subset exists.
//! The fix belongs in `lower.rs`; `tests/rules_feature_multiplicity.rs` pins the current shape so
//! it stays visible.
//!
//! # The seven rules that are not decided here
//!
//! `validateFeatureMultiplicityDomain` (`data/spec_rules.rs:407`) —
//! `multiplicity <> null implies multiplicity.featuringType = featuringType`. [`Element`] has no
//! `featuringType`, and a multiplicity is a string ([`Element::multiplicity`],
//! `sysml-model/src/lib.rs:843`), not an element with featuring types of its own, so neither side
//! of the equality exists.
//!
//! `validateFeatureEndNotDerivedAbstractCompositeOrPortion` (`data/spec_rules.rs:461`) — needs
//! `isDerived`, `isAbstract`, `isComposite` and `isPortion` (metamodel features at
//! `generated.rs:2240`, `:2639`, `:2234`, `:2244`). The parser collects all four in its
//! `Prefixes` (`sysml-parser/src/ast.rs:160` `is_derived`, `:157` `is_abstract`, `:166` `portion`),
//! but `lower.rs` reads only `is_end` (`lower.rs:281`, `:338`) and `direction` (`:298`, `:391`);
//! no other name in `lower.rs` mentions a prefix. So `abstract part ap : A` and `part ap : A`
//! lower to identical elements.
//!
//! `validateOccurrenceUsageIsPortion` (`data/spec_rules.rs:905`) and
//! `validateOccurrenceUsagePortionKind` (`:911`) — need `portionKind` and `isPortion`
//! (`generated.rs:2444`, `:2244`). `portion` / `snapshot` / `timeslice` are parsed into
//! `Prefixes.portion` (`sysml-parser/src/ast.rs:166`, `sysml-parser/src/parser.rs:304-306`) and
//! dropped by the lowering. The second rule also tests
//! `owningType.oclIsKindOf(OccurrenceDefinition) or ... (OccurrenceUsage)`, which has no
//! counterpart here: an occurrence *usage* lowers to `Kind::OccurrenceDef`
//! (`lower.rs:253`), the same variant as an occurrence definition, so the two cannot be told
//! apart by kind.
//!
//! `validateOccurrenceUsageIndividualUsage` (`data/spec_rules.rs:899`) and
//! `validateOccurrenceUsageIndividualDefinition` (`:893`) — need `isIndividual` and the
//! `occurrenceDefinition` list. `individual` is parsed into `Prefixes.is_individual`
//! (`sysml-parser/src/ast.rs:159`) and read nowhere in `lower.rs`, so `individual part x : Occ`
//! and `part x : Occ` lower alike; and `occurrenceDefinition` is the usage's list of types, which
//! the tree keeps only as a single [`Element::type_ref`] string whose comma list the lowering
//! truncates (`lower.rs:326` `target_of(RelationOp::Typing)` returns the first target). The
//! derived `individualDefinition` (`generated.rs:8765`) is a first match over that same list.
//!
//! `validateTypeOwnedMultiplicity` (`data/spec_rules.rs:305`) —
//! `ownedMember->selectByKind(Multiplicity)->size() <= 1`. An owned `multiplicity m : [1..*]` is
//! parsed with the KerML classifier keyword `multiplicity`
//! (`sysml-parser/src/parser.rs:1014-1017`) but lowers to `Kind::KermlType` (`lower.rs:258-264`,
//! the catch-all arm), the variant every unrecognised KerML classifier keyword gets without
//! keeping which keyword it was. An owned multiplicity is therefore not identifiable: a
//! `Multiplicity` and an embedded `class` are the same kind, and `deriveTypeMultiplicity`
//! (`generated.rs:8105`) reads the multiplicity as a *member element* the tree does not carry at
//! all.

use sysml_model::Element;
use sysml_resolve::Index;

use super::{walk, Violation};

/// The `SEM` code this group reports under.
const CODE: &str = "SEM021";

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    walk(roots, |el| {
        if !el.is_end {
            return;
        }
        let Some(stated) = &el.multiplicity else {
            // No bound written. The reference implementation supplies 1..1 for this case
            // (reify.rs:897-911) and the shipped library relies on it
            // (`Systems Library/Connections.sysml:43-44`), so silence is legal.
            return;
        };
        let Some(range) = Range::parse(stated) else {
            // A bound written as an expression (`[n]`) is left undecided: `hasBounds` needs the
            // evaluated value, and guessing would report a violation the model may not have.
            return;
        };
        if !range.has_bounds_one_one() {
            out.push(Violation::new(
                CODE,
                el,
                qualified_name(el, index),
                format!("an `end` feature must have multiplicity 1..1, but `{stated}` is declared"),
            ));
        }
    });
    out
}

/// One bound of a multiplicity range, in the three shapes `valueOf` distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bound {
    /// No bound expression on this side — `deriveMultiplicityRangeLowerBound` returns null for a
    /// one-member range, and `hasBounds` handles that null separately from a value.
    Absent,
    /// `*`, i.e. `LiteralInfinity`, which `valueOf` returns as `*`.
    Infinity,
    /// A whole-number literal, which `valueOf` returns as an Integer when it is non-negative.
    Literal(i64),
}

/// A parsed multiplicity range, in the terms `MultiplicityRange` states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Range {
    lower: Bound,
    upper: Bound,
}

impl Range {
    /// Read `[..]` into the lower and upper bound, or `None` when a side is not a literal or `*`
    /// and so cannot be settled without the evaluator.
    fn parse(multiplicity: &str) -> Option<Self> {
        let inner = multiplicity
            .trim()
            .strip_prefix('[')?
            .strip_suffix(']')?
            .trim();
        let bound = |text: &str| -> Option<Bound> {
            let t = text.trim();
            if t == "*" {
                Some(Bound::Infinity)
            } else {
                t.parse::<i64>().ok().map(Bound::Literal)
            }
        };
        match inner.split_once("..") {
            // Two expressions: the first is the lower bound, the second the upper.
            Some((lower, upper)) => Some(Self {
                lower: bound(lower)?,
                upper: bound(upper)?,
            }),
            // One expression: `deriveMultiplicityRangeUpperBound` takes it as the upper bound and
            // `deriveMultiplicityRangeLowerBound` leaves the lower null.
            None => Some(Self {
                lower: Bound::Absent,
                upper: bound(inner)?,
            }),
        }
    }

    /// `hasBounds(1, 1)`, with the null-lower branch `valueOf` allows spelled out.
    ///
    /// The upper bound must be exactly 1. The lower must then be 1, or absent — `hasBounds`
    /// accepts a null lower through `lower = upper`, which the upper check has already made true.
    /// A `*` lower is neither 1 nor null, so it fails.
    fn has_bounds_one_one(self) -> bool {
        self.upper == Bound::Literal(1) && matches!(self.lower, Bound::Absent | Bound::Literal(1))
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
