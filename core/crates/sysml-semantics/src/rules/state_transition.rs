//! `state-transition` — a state's own subactions, and the edge a transition leaves.
//!
//! Seven rules from the specification's table were assigned to this group:
//!
//!   * `validateStateUsageParallelSubactions` — `isParallel implies
//!     nestedAction.incomingTransition->isEmpty() and nestedAction.outgoingTransition->isEmpty()`;
//!   * `validateStateDefinitionParallelSubactions` — the same on a definition's `ownedAction`;
//!   * `validateStateUsageStateSubactionKind` and `validateStateDefinitionStateSubactionKind` —
//!     `ownedMembership->selectByKind(StateSubactionMembership)->isUnique(kind)`;
//!   * `validateTransitionUsageSuccession` — a `TransitionUsage` must own a `Succession` whose
//!     target feature's `featureTarget` is an `ActionUsage`;
//!   * `validateTransitionUsageTriggerActions` — a non-`StateUsage` source implies no trigger;
//!   * `validateTransitionUsageParameters` — at least one input parameter, two if there is a
//!     trigger.
//!
//! Three are decided here: the two `StateSubactionKind` rules and the succession rule. The other
//! four cannot be decided from this element tree, and the sections below say exactly which field
//! or which statement is missing.
//!
//! The report code reserved for this group is `SEM023`.
//!
//! # The parallel rules: `isParallel` is consumed and thrown away
//!
//! `isParallel` is a stored attribute of both metaclasses: `attribute isParallel :
//! Boolean[1..1]` on `StateUsage`
//! (`research/.venv/Lib/site-packages/sysmlpy/library/systems/SysML.sysml:389`) and on
//! `StateDefinition` (`:368`). [`Element`] has no field for it: the full field list is
//! `sysml-model/src/lib.rs:721-850`, and the only feature modifiers carried are `direction`
//! (`sysml-model/src/lib.rs:777`) and `is_end` (`sysml-model/src/lib.rs:849`).
//!
//! The keyword does not survive the front end at all. `Prefixes`
//! (`sysml-parser/src/ast.rs:155-169`) has no `is_parallel`, and `parallel` is read as a
//! multiplicity modifier and discarded in two `while` loops inside `head()`
//! (`sysml-parser/src/parser.rs:1055-1060` and `:1063-1068`; the comment above the first loop
//! says so). So `state s : Mode parallel { … }` and `state s : Mode { … }` lower to the same
//! tree, which is what the test at the bottom of `tests/rules_state_transition.rs` pins down.
//! Both rules are dropped rather than guessed at.
//!
//! Even if the flag were carried, the consequent needs something the tree does not have directly:
//! the rule quantifies over `nestedAction` / `ownedAction` and asks about each one's
//! `incomingTransition` and `outgoingTransition`. Those are reversed links. A
//! [`Kind::Transition`] stores its two ends as text in `from` / `to`
//! (`sysml-model/src/lib.rs:779-781`), so both sets would have to be reconstructed by resolving
//! every transition's ends and grouping by endpoint. That is derivable rather than missing, but it
//! is only worth building once the antecedent exists.
//!
//! # The trigger rule: `triggerAction` is discarded, not missing
//!
//! `sysml-parser/src/parser.rs:2254-2260` reads the `accept …` clause of a transition into a
//! local `trigger`, and `sysml-parser/src/parser.rs:2294` writes `let _ = trigger;`. Nothing
//! reaches the tree, and no `AcceptActionUsage` kind exists — [`Kind::ControlFlow`] named
//! `accept` is the *state-level* message statement (`sysml-syntax/src/lower.rs:850-859`), not the
//! transition's trigger. [`Kind::Transition`] carries no field for it either: `sysml-model/src/lib.rs:721-850`
//! has `predicate` for the guard and nothing for a trigger.
//!
//! The antecedent (`source <> null and not source.oclIsKindOf(StateUsage)`) is decidable: the
//! source is `Element::from`, resolved through the index. The consequent is not. So the rule is
//! dropped: an implementation could only ever answer "no trigger on this transition", which is
//! true for every transition in the tree and therefore says nothing.
//!
//! # The parameter rule: the parameters are never attached to the transition
//!
//! The grammar gives a `TransitionUsage` two `emptyParameterMember` slots before the trigger
//! (`research/.venv/Lib/site-packages/sysmlpy/grammar/antlr4/SysMLv2Parser.g4:1772`, with
//! `emptyParameterMember : emptyUsage_` at `:1612-1614`). The parser reads neither: its
//! transition branch reads a name, `relations`, then `first` / `accept` / `if` / `do` in a loop
//! (`sysml-parser/src/parser.rs:2238-2294`). `transition t in p first a then b;` therefore leaves
//! `in p` in the stream, where it is re-parsed as a member of the enclosing state next to the
//! transition — the parameter is a *sibling*, not a child. The transition's own action body is
//! parsed and dropped at `sysml-parser/src/parser.rs:2292`, so `then b { in x : Real; }` leaves no
//! child either.
//!
//! This rule is worse than undecidable: it would fire on every legal transition. With parameters
//! never owned and `triggerAction` gone as above, `inputParameters()->size() >= 1` is false for
//! every transition this tree can hold, so enforcing it would report every transition in the
//! validation corpus. Dropped.
//!
//! # The succession rule: the target half is decidable, the "has a succession" half is structural
//!
//! A `TransitionUsage` and its `Succession` collapse into one [`Kind::Transition`]: both spellings
//! (`transition t first a then b` and the shorthand `then b`) lower through `Statement::Succession`
//! at `sysml-syntax/src/lower.rs:744-767`. So `successions->notEmpty()` holds by construction —
//! every transition in the tree came from a succession statement, and a truncated one keeps its
//! element — and only `targetFeature.featureTarget->forAll(oclIsKindOf(ActionUsage))` has anything
//! left to decide. That half is decided: the `then` end is `Element::to`
//! (`sysml-model/src/lib.rs:781`), it is resolved through the index, and the resolved element must
//! project an `ActionUsage`.
//!
//! Three boundaries are deliberate.
//!
//! * Only a transition that is a *child of a state* is judged. `TransitionUsage` occurs in a state
//!   body only (`stateBodyItem`, `SysMLv2Parser.g4:1701-1708`); an action body writes a plain
//!   `succession` (`guardedSuccessionMember`, `SysMLv2Parser.g4:1458`), and a `Succession`'s
//!   `targetFeature` may be any feature — `first a then p` with `p` an attribute is a legal
//!   succession and not a `TransitionUsage` at all. Both spellings lower to [`Kind::Transition`],
//!   so the owner is the only thing that tells them apart, and the walk below reaches a transition
//!   only through its state's body.
//! * An unresolved `then` end is not judged. The OCL's `forAll` runs over
//!   `targetFeature.featureTarget`; a reference that does not resolve has no feature target in the
//!   metamodel either, and `forAll` over an empty collection is true. Reporting a *kind* violation
//!   here would be inventing a fact the model does not state. This is an under-report, and it is
//!   worth naming: no other check in the kernel resolves `Element::to` either
//!   (`Index::references` at `sysml-resolve/src/lib.rs:992-1039` collects `type_ref`, the ends of a
//!   `satisfy`, and predicate names — not a transition's ends), so a transition naming nothing is
//!   currently silent. That is a gap in name resolution, not in this rule, and it is not fixed
//!   here by turning a missing target into a well-formedness error.
//! * A `then` end that resolves to [`Kind::PseudoState`] is not judged. That kind has no
//!   counterpart in the metamodel — `ElementKind` declares no `Pseudostate` (no occurrence in
//!   `research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs`), and the grammar spells the
//!   statement `FIRST qualifiedName` (`SysMLv2Parser.g4:1475-1477`), so `first` is a keyword and
//!   never a member. The lowering invents the element at `sysml-syntax/src/lower.rs:769-773`,
//!   naming it `first`; judging it would report on the projection rather than on the model.
//!
//! # Why the state body is cut at a half-read member
//!
//! A state's entry/do/exit membership reaches the tree as [`Kind::StateAction`] named after the
//! keyword: `sysml-syntax/src/lower.rs:399-405` retypes the bare spelling, and
//! `sysml-syntax/src/lower.rs:991-1008` lowers the parsed one. So the kind is readable and the
//! rule is decidable in principle.
//!
//! The catch is that the child list is not always the authored membership list. `accept when
//! <expression>` is not a form the parser finishes reading. The `accept` branch eats an optional
//! `at` and then reads one succession end (`sysml-parser/src/parser.rs:2254-2260`), so
//! `accept when a.Tmax > 0` takes `when` as the trigger — `when` is a keyword token and
//! `succession_end` reads any name (`:2851-2875`) — then breaks at `>`; the guard and everything
//! after it are re-parsed as members of the enclosing state. In `5-State-based Behavior-1.sysml` a
//! state declares one `do` at `:130`; the effect at `:148` then arrives as a second
//! [`Kind::StateAction`] named `do`, and a legal model would be reported as having two.
//! (`5-State-based Behavior-2.sysml:79-81` spills the same way through the state-level `accept`
//! form, and `5-State-based Behavior-1a.sysml:146-150` likewise.)
//!
//! So a state body is judged only up to the first member the parser demonstrably did not consume
//! whole; the members before it are the source's own, and everything after is re-parsed material.
//! Two shapes mark that point, and both are exact rather than heuristic:
//!
//!   * a [`Kind::Transition`] child with no `to` — the `then` clause was never read
//!     (`sysml-parser/src/parser.rs:2286-2291`), so everything after the half-read trigger was
//!     re-parsed as a member;
//!   * a [`Kind::ControlFlow`] named `accept` or `send` whose `to` is `when` or `at` — the payload
//!     was read as the clause keyword, and the tail of that statement spilled. `message` starts
//!     with `name = self.name()` (`sysml-parser/src/parser.rs:2979`), and `name()` accepts a
//!     keyword token (`at_name` at `sysml-parser/src/parser.rs:175-180`, used by `name()` at
//!     `:218`), so `accept when <guard>` takes `when` as the payload and `accept at <time>` takes
//!     `at` as it — `at` is not in the lexer's keyword table (`sysml-parser/src/lexer.rs:114-133`)
//!     but is still an identifier token, which `at_name` accepts too. The lowering then writes that
//!     text into [`Element::to`] (`sysml-syntax/src/lower.rs:856-858`), and the remainder of the
//!     statement is re-parsed.
//!
//! Both spellings are legal SysML — `TriggerKind` is `when` / `at` / `after`
//! (`SysML.sysml:434-438`) — so what the cut suppresses is a mis-reading of a legal model, not a
//! defect in one.
//!
//! Cutting the tail rather than skipping the whole state keeps the rules live on the members that
//! were read correctly: a state that declares two `entry`s and then contains a spilled statement
//! is still reported, and only the material after the spill is ignored. The cut applies to the
//! transitions too, for the same reason: a `then` clause inside re-parsed material is not an
//! authored edge.
//!
//! One more correction is needed at the member level. `lower.rs:399-405` retypes *every*
//! `AttributeUsage` named `entry`, `do` or `exit` inside a state, so `attribute entry : Real;`
//! arrives as [`Kind::StateAction`] too. A membership carries the behaviour it names in `to`
//! (`lower.rs:1000-1002`); the retyped feature carries a type instead and never a `to`. That
//! signature is what [`is_subaction`] filters on.
//!
//! Three further gaps, all in the under-reporting direction:
//!
//! * the retype only runs inside [`Kind::StateDef`] / [`Kind::StateUsage`] (`lower.rs:313`), so a
//!   bare `entry;` written inside an `exhibit state` body stays an attribute and is not counted.
//!   The exhibit form is still walked because it is a `StateUsage`
//!   (`ElementKind::ExhibitStateUsage => &[PerformActionUsage, StateUsage]`,
//!   `research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:1661`), and the parsed
//!   `entry action x;` spelling is visible there;
//! * a membership written with a type and no name (`entry action : Foo;`) carries `to = None`
//!   together with a type, the same signature as the retyped feature, so it is not counted either;
//! * a `then` end that names a state's entry/do/exit action resolves to nothing, because the
//!   projected [`Kind::StateAction`] is named after the keyword while `to` holds the behaviour's
//!   name (`lower.rs:998-1002`) and the behaviour itself is not a separate element. Such an end is
//!   unresolved and therefore not judged, per the second boundary above. Its mirror image — an end
//!   that spells the keyword (`then entry`) — resolves to the membership. The metamodel gives a
//!   membership no name of its own, so a reference that reaches it names nothing the rule can test;
//!   [`transition_target`] skips it for the same reason it skips a pseudo state.

use sysml_model::{Element, Kind, NameRef};
use sysml_resolve::{Index, Resolution};

use super::Violation;

/// The code every rule in this group reports under.
const CODE: &str = "SEM023";

/// The kinds whose body is a `stateBodyItem*` sequence, i.e. whose members are
/// `StateSubactionMembership`s.
///
/// `ExhibitStateUsage` specializes `StateUsage`
/// (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:1661`).
fn is_state(kind: &Kind) -> bool {
    matches!(kind, Kind::StateDef | Kind::StateUsage | Kind::ExhibitState)
}

/// The kinds that project an `ActionUsage`, directly or through one of its specializations.
///
/// `ActionUsage` is `Step`/`OccurrenceUsage` (`generated.rs:1608`), and the subtypes this
/// projection can produce are `StateUsage` (`:1751`), `ExhibitStateUsage` (`:1661`),
/// `TransitionUsage` (`:1764`), `ControlNode` (`:1646`), `PerformActionUsage` (`:1727`),
/// `CalculationUsage` (`:1627`), `FlowUsage` (`:1675`), `AcceptActionUsage` (`:1606`),
/// `SendActionUsage` (`:1746`), `AssignmentActionUsage` (`:1617`), `IfActionUsage` (`:1680`),
/// `WhileLoopActionUsage` (`:1780`) and `ForLoopActionUsage` (`:1676`) — all in
/// `research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs`.
///
/// Definitions are not usages, so `ActionDef`, `StateDef` and `CalcDef` are absent on purpose: a
/// transition ends at an *instance* of a behaviour, and `oclIsKindOf(ActionUsage)` is false for
/// the definition.
///
/// [`Kind::StateAction`] is absent as well: the projected element stands for a
/// `StateSubactionMembership` (`SysML.sysml:382-386`), which is a `FeatureMembership`, not an
/// `ActionUsage`.
fn is_action_usage(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::ActionUsage
            | Kind::StateUsage
            | Kind::ExhibitState
            | Kind::Transition
            | Kind::ControlNode
            | Kind::ControlFlow
            | Kind::Perform
            | Kind::CalcUsage
            | Kind::Flow
    )
}

/// `entry` / `do` / `exit`, the `kind` of a `StateSubactionMembership`.
///
/// The enum has exactly these three values (`SysML.sysml:376-380`), and the lowering stores the
/// authored keyword as the element's name.
fn subaction_kind(name: &str) -> Option<&'static str> {
    match name {
        "entry" => Some("entry"),
        "do" => Some("do"),
        "exit" => Some("exit"),
        _ => None,
    }
}

/// Whether this member is the statement the parser stopped reading partway, so that everything
/// after it in the body is re-parsed material rather than authored membership.
///
/// See the module note for the two shapes and the corpus that shows them.
fn spilled_here(child: &Element) -> bool {
    match child.kind {
        // A transition of a state has a mandatory `then` (`SysMLv2Parser.g4:1772`), so a missing
        // target means the clause was never reached and the tail spilled.
        Kind::Transition => child.to.is_none(),
        // A message whose payload was read as the keyword: the tail of the statement spilled.
        Kind::ControlFlow => {
            matches!(child.name.as_str(), "accept" | "send")
                && matches!(child.to.as_deref(), Some("when") | Some("at"))
        }
        _ => false,
    }
}

fn qualified(index: &Index, el: &Element) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .unwrap_or_else(|| el.name.clone())
}

/// Whether a [`Kind::StateAction`] child is an authored `StateSubactionMembership`.
///
/// The kind alone does not say so. Besides the two spellings that carry the membership — the
/// parsed `entry <name> : <type>` (`sysml-parser/src/parser.rs:2437-2456`) and the bare `entry;`,
/// which arrives as a declaration and is retyped at `sysml-syntax/src/lower.rs:399-405` — the
/// retype sweep also catches an ordinary feature that merely happens to be *named* `entry`, `do`
/// or `exit`: `attribute entry : Real;` inside a state becomes `StateAction` with `to = None` and a
/// type. A membership written with a type always names its behaviour, so `to = None` together with
/// a `type_ref` is the signature of the retyped feature, and it is not counted.
fn is_subaction(child: &Element) -> bool {
    child.kind == Kind::StateAction && !(child.to.is_none() && child.type_ref.is_some())
}

/// One state's body, in authored order.
///
/// Both rules of this group that a state owns run here, and both stop at the same point: the first
/// member the parser did not consume whole. A [`Kind::Transition`] is judged only from this walk,
/// so "the transition is a child of a state" is structural rather than re-derived from
/// `Index::parent`.
fn state_body(state: &Element, index: &Index, out: &mut Vec<Violation>) {
    let mut seen: Vec<&'static str> = Vec::new();
    for child in &state.children {
        // Everything after the first half-read member is re-parsed material, not the authored
        // membership list; the prefix before it is the source's own.
        if spilled_here(child) {
            return;
        }
        match child.kind {
            // `validateStateUsageStateSubactionKind` / `validateStateDefinitionStateSubactionKind`
            // — at most one `entry`, one `do` and one `exit` membership per state.
            Kind::StateAction => {
                if !is_subaction(child) {
                    continue;
                }
                // The role is the clause keyword, not the element name: `entry boot;` declares an
                // action called `boot`, so the name answers "which behaviour" and only `clause`
                // answers "which role". Reading the name worked while the lowering put the
                // keyword there; once the behaviour's own name became the element name, every
                // duplicate-entry check silently stopped matching.
                let role = child.clause.as_deref().unwrap_or(&child.name);
                let Some(kind) = subaction_kind(role) else {
                    continue;
                };
                if seen.contains(&kind) {
                    // Reported at the *second* subaction, not at the state: the pilot suite's own
                    // assertions point at `exit action c1;` — the duplicate — and a reader looking
                    // for the defect wants the line that has to go.
                    let owner = qualified(index, state);
                    out.push(Violation::new(
                        CODE,
                        child,
                        qualified(index, child),
                        format!(
                            "`{}` is a second `{kind}` subaction of `{owner}`; at most one of each \
                             kind is allowed",
                            qualified(index, child)
                        ),
                    ));
                    continue;
                }
                seen.push(kind);
            }
            // `validateTransitionUsageSuccession` — the `then` end names an `ActionUsage`.
            Kind::Transition => transition_target(child, index, out),
            _ => {}
        }
    }
}

/// `validateTransitionUsageSuccession` — the `then` end of a transition names an `ActionUsage`.
///
/// Called only for a transition that is a child of a state, and only for one the state body read
/// whole (see [`state_body`]).
fn transition_target(transition: &Element, index: &Index, out: &mut Vec<Violation>) {
    let Some(text) = transition.to.as_deref() else {
        return;
    };
    let resolution = if text.contains('.') {
        index.resolve_path(transition.id, &NameRef::new(text, transition.line))
    } else {
        index.resolve(transition.id, text)
    };
    // No feature target, so the OCL's `forAll` is vacuously true. See the module note for why this
    // is left alone rather than reported.
    let Resolution::Found(target) = resolution else {
        return;
    };
    let kind = &index.kind[target];
    if is_action_usage(kind) {
        return;
    }
    // A [`Kind::PseudoState`] is not a metaclass. It is invented by the lowering for the
    // `first <target>;` statement (`sysml-syntax/src/lower.rs:769-773`) and named `first`; the
    // metamodel has no such element, and the grammar's `initialNodeMember` is
    // `FIRST qualifiedName` (`SysMLv2Parser.g4:1475-1477`), so `first` there is a keyword and never
    // a member name. A `then` end resolving to it says something about the projection, not about
    // the metamodel, and `oclIsKindOf(ActionUsage)` has no answer to give.
    if *kind == Kind::PseudoState {
        return;
    }
    // A [`Kind::StateAction`] is a `StateSubactionMembership`'s owned action. A `then` end that
    // resolves to it is naming an action usage the membership introduced, which is legal — the
    // guard stays for the shape the projection can still produce: a bare `entry;` with no
    // behaviour has no action of its own, and judging it would report the projection rather than
    // the model.
    if *kind == Kind::StateAction {
        return;
    }
    let at = qualified(index, transition);
    out.push(Violation::new(
        CODE,
        transition,
        at.clone(),
        format!(
            "the `then` end of `{at}` names `{text}`, which is a {kind}; a transition must end at an action usage"
        ),
    ));
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if is_state(&el.kind) {
            state_body(el, index, &mut out);
        }
    });
    out
}
