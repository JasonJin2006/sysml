//! `type-multiplicity` — how many multiplicities a type owns, what a feature's multiplicity is
//! bound to, and what an `end` boundary may state.
//!
//! Three rules were handed to this group. One is decided here; the other two reach for properties
//! the element tree does not carry, and each is named below with the field or metaclass it is
//! missing. The group reports under `SEM033`.
//!
//! The three, as they appear in `data/spec_rules.rs`:
//!
//!   * `validateTypeOwnedMultiplicity` (`:305-308`) — Type,
//!     `ownedMember->selectByKind(Multiplicity)->size() <= 1`,
//!     "A Type may have at most one ownedMember that is a Multiplicity."
//!   * `validateFeatureMultiplicityDomain` (`:407-410`) — Feature,
//!     `multiplicity <> null implies multiplicity.featuringType = featuringType`,
//!     "If a Feature has a multiplicity, then the featuringTypes of the multiplicity must be the
//!     same as those of the Feature itself."
//!   * `validateFeatureEndMultiplicity` (`:455-458`) — Feature,
//!     `isEnd implies multiplicities().allSuperTypes()->flatten()->selectByKind(MultiplicityRange)
//!     ->exists(hasBounds(1,1))`, "If a Feature has isEnd = true, then it must have multiplicity
//!     1..1."
//!
//! # The one rule this module decides
//!
//! `validateFeatureEndMultiplicity`, from [`Element::is_end`] (`sysml-model/src/lib.rs:860`) and
//! [`Element::multiplicity`] (`sysml-model/src/lib.rs:854`). Two facts from the metamodel's own
//! operation table decide what the text settles:
//!
//!   * `hasBounds` (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:9420-9423`) is
//!     `valueOf(upperBound) = upper and let lowerValue = valueOf(lowerBound) in (lowerValue =
//!     lower or lowerValue = null and (lower = upper or lower = 0 and upper = *))`.
//!   * `deriveMultiplicityRangeUpperBound` (`generated.rs:7979-7982`) returns the only owned
//!     expression when there is one and the second when there are more;
//!     `deriveMultiplicityRangeLowerBound` (`:7985-7988`) returns null for fewer than two. So
//!     `[1]` is upper = 1 with a **null** lower, which `hasBounds(1,1)` accepts through its
//!     `lower = upper` branch. `[*]` is upper = `*`, which fails the very first comparison;
//!     `[1..*]` and `[0..*]` fail it the same way; `[2..4]` fails it because the upper is 4;
//!     `[*..1]` has a `*` lower, which is neither 1 nor null, so its second disjunct fails.
//!
//! `valueOf` (`generated.rs:9426-9429`) answers null for a bound that is not model-level
//! evaluable, so a bound written as a name (`[n]`) is left undecided: deciding whether `n` is 1
//! needs the evaluator, and rejecting a model that may be right costs more than staying silent.
//! Only the three shapes the bound text settles are judged — an integer, `*`, and a pair of those.
//!
//! A feature with `isEnd` and no written bound is accepted. The reference implementation
//! materialises a 1..1 range for every end whose range has no upper bound
//! (`research/upstream/sem/sysmlv2-semantics-0.1.2/src/reify.rs:889-911`, whose comment says the
//! notation writes it nowhere); the shipped library writes `end source: Anything` with no bound
//! (`core/stdlib/Systems Library/Connections.sysml:43-44`),
//! and the official validation corpus does the same (`research/upstream/release/sysml/src/
//! validation/02-Parts Interconnection/2a-Parts Interconnection.sysml:82-83`). Rejecting a silent
//! end would reject both.
//!
//! # The inherited half of the OCL is not implemented, and why it must not be guessed
//!
//! `multiplicities()` (`generated.rs:9534-9538`) is the feature's own multiplicity if it has one,
//! otherwise the multiplicities found by closing over `ownedSpecialization.general`. Reaching the
//! inherited half needs two things the tree does not give:
//!
//!   * `allSuperTypes()` is not in the metamodel's operation table — `generated.rs` mentions it
//!     only inside three rules' OCL text (`:7230`, `:9585`, `:9849`), never as an
//!     `Operation { name: "allSuperTypes", .. }` — so what the call contributes on a multiplicity
//!     is not settled from the files available here.
//!   * `ownedSpecialization.general` is not separable from subsetting. The parser maps `subsets`
//!     / `subset` onto the same operator as `specializes` (`sysml-parser/src/parser.rs:1167`)
//!     and the lowering folds that clause and every `:>>` target into
//!     [`Element::supertypes`] (`sysml-syntax/src/lower.rs:367-402`). Walking that list as if it
//!     were the specialization closure would fire on an end that merely subsets a bounded
//!     feature, which `multiplicities()` does not cover — a false positive on a legal model.
//!
//! An end that inherits its bound through `:>` is therefore not reported. That is a missed
//! violation, never a false one; a check that guessed the relation would trade it for the
//! opposite error.
//!
//! # Inherited: the owned-cross-feature lowering defect
//!
//! §7.13.2 splits an end that declares an owned cross feature into two declarations, and the
//! figure `rules/feature_multiplicity.rs:48-67` reads as the specification's own example writes
//! `end connectingHub [0..1] ordered part hub : Hub` with the bound belonging to the *cross*
//! feature. Whichever declaration wrote `end` is the one the lowering marks
//! (`sysml-syntax/src/lower.rs:338`, `el.is_end = d.head.prefixes.is_end`), and in that form it is
//! the cross feature — so the cross multiplicity lands on the element this module checks, and the
//! real end is left unflagged. `rules/feature_multiplicity.rs:48-67` analyses the shape in full;
//! the two readings (`end cross : X[0..1] part hub : Hub` and `end cross : X[0..1]; part hub : Hub`)
//! are structurally identical in the tree, so no sound guard exists here.
//! `tests/rules_type_multiplicity.rs` pins the current behaviour rather than hiding it, and the fix
//! belongs in `lower.rs`.
//!
//! # Overlap with `SEM021`, for the integrator
//!
//! `validateFeatureEndMultiplicity` is also enforced by
//! [`super::feature_multiplicity`], which reports it under `SEM021`
//! (`sysml-semantics/src/conformance.rs:62`). This module was handed the same rule, so on a model
//! with a non-1..1 end both modules fire: two diagnostics, one defect. The message is deliberately
//! worded the same in both, so retiring either code leaves the reader the same sentence. One of
//! the two must be dropped when `conformance::implemented()` is settled; the duplicate is recorded
//! here rather than silently left for a reader to discover in a report.
//!
//! # The two rules this module cannot decide
//!
//! `validateTypeOwnedMultiplicity` — an owned `Multiplicity` is not identifiable in the tree.
//! Every KerML classifier keyword the lowering does not model lands on [`Kind::KermlType`]
//! (`sysml-syntax/src/lower.rs:256-264`): `multiplicity`, `class`, `datatype`, `struct`, `assoc`,
//! `association`, `metaclass`, `behavior`, `function`, `predicate`, `interaction` and `classifier`
//! are all collected as `Kind::Kerml(String)` by the parser
//! (`sysml-parser/src/parser.rs:1010-1016`) and then reduced to one kind with the keyword thrown
//! away. [`Kind::Other`] exists as a variant (`sysml-model/src/lib.rs:146`) but is constructed
//! nowhere — `grep -rn "Kind::Other" core/crates/` finds only its `keyword()` arm
//! (`sysml-model/src/lib.rs:215`) and its `group()` arm (`:298`). So
//! `multiplicity a [1..*]; multiplicity b [1..*];` and `datatype a [1..*]; datatype b [1..*];`
//! lower to identical elements, and counting `KermlType` children would report the datatype model
//! — which owns no multiplicity at all. `tests/rules_type_multiplicity.rs` pins that equality.
//!
//! `validateFeatureMultiplicityDomain` — neither side of the equality exists. [`Element`] has no
//! featuring-type field (`grep -rn "featuring" sysml-model/src/ sysml-resolve/src/` finds nothing),
//! while the metamodel declares it derived (`generated.rs:2238`, `FeatureMeta { name:
//! "featuringType", .. derived: true }`) through `deriveFeatureFeaturingType`
//! (`generated.rs:8213-8216`). A multiplicity is the authored text, `Option<String>`
//! (`sysml-model/src/lib.rs:848-854`), not an element, so it has no featuring types of its own;
//! and the notation that would write a domain, `featured by`
//! (`research/upstream/KerML.xtext:568-571`), is not read by the parser — `sysml-parser/src` has no
//! `"featured"` arm, and the lexer's keyword table does not list the word
//! (`sysml-parser/src/lexer.rs:114-136`). With both sides unreadable the consequent cannot be
//! falsified by any model, and a rule whose check cannot fail must not be listed as enforced
//! (`rules/port_attribute.rs:62-71` reaches the same conclusion for a derivation default).

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::{walk, Violation};

/// The code every rule in this group reports under.
const CODE: &str = "SEM033";

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    walk(roots, |el| {
        if !el.is_end {
            return;
        }
        let Some(stated) = &el.multiplicity else {
            // No bound written. The reference implementation supplies 1..1 for this case
            // (`reify.rs:897-911`) and both the shipped library and the official corpus rely on
            // it, so silence is legal.
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
    // validateTypeOwnedMultiplicity — `ownedMember->selectByKind(Multiplicity)->size() <= 1`.
    //
    // `multiplicity m : [1..*]` now keeps its keyword, so an owned Multiplicity can be counted.
    // The rule is stated of a Type, which in this tree is any element that can own members.
    //
    // Reported at the member that is one too many, not at the type: the pilot writes the assertion
    // on the second `multiplicity` line (`Type_Multiplicity_invalid.kerml.xt:21`), and a single
    // report on the type line was both a miss at the member and a false positive at the type — the
    // same defect counted twice, once on each side of the score.
    walk(roots, |el| {
        let mut owned = el
            .children
            .iter()
            .filter(|child| child.kind == Kind::MultiplicityDef);
        let Some(_first) = owned.next() else {
            return;
        };
        for extra in owned {
            // The member is anonymous — `multiplicity subsets Base::zeroToMany;` writes no name —
            // so the index gives it its owner's qualified name. The line is what tells the two
            // apart, and it is what the pilot's assertion points at.
            let owner = qualified_name(el, index);
            let line = index.line[extra.id];
            out.push(Violation::new(
                CODE,
                extra,
                format!("{owner} (line {line})"),
                format!(
                    "`{owner}` owns a second multiplicity, at line {line}; a type may own at most \
                     one"
                ),
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
///
/// The parse is the same shape as `rules/feature_multiplicity.rs`'s `Range`; the two modules are
/// kept independent because that one is outside this group's edit scope, and a checker that read
/// the other's private helper would couple two report codes to one implementation.
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
