//! The `state-transition` rules, each proved by a model that violates it.
//!
//! Only three of the seven assigned rules can be decided from this element tree; each is proved
//! twice — once by a model that breaks it and produces SEM023, once by the nearest legal model
//! that must stay silent. The four that cannot run are pinned down at the bottom by the model
//! that shows why, so the omission is visible rather than silent.
//!
//! The rules that do run:
//!
//!   * `validateStateUsageStateSubactionKind`;
//!   * `validateStateDefinitionStateSubactionKind`;
//!   * `validateTransitionUsageSuccession`.

use sysml_model::{Element, Kind};
use sysml_semantics::{check, Severity};

/// Every diagnostic as `(code, line)`, for the tests that care *where* a finding is reported.
fn issues_with_lines(src: &str) -> Vec<(String, u32)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| (i.code.to_string(), i.line))
        .collect()
}

/// The helper `tests/rules.rs` uses, so a diagnostic is read the same way here.
fn issues(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

/// Every SEM023 diagnostic, as `(element, message)`.
fn sem023(src: &str) -> Vec<(String, String)> {
    issues(src)
        .into_iter()
        .filter(|(code, _, _)| code == "SEM023")
        .map(|(_, at, message)| (at, message))
        .collect()
}

fn has(src: &str, at: &str, needle: &str) -> bool {
    sem023(src)
        .iter()
        .any(|(element, message)| element == at && message.contains(needle))
}

/// The first element named `name`, anywhere in the tree.
fn find<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
    if el.name == name {
        return Some(el);
    }
    el.children.iter().find_map(|child| find(child, name))
}

fn named(src: &str, name: &str) -> Element {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    loaded
        .roots
        .iter()
        .find_map(|root| find(root, name))
        .unwrap_or_else(|| panic!("no element named `{name}` in this model"))
        .clone()
}

/// A projection of everything the tree records about an element and its children.
///
/// `Element` has no `PartialEq`, and comparing two whole trees by hand would be re-listing every
/// field. This keeps the fields these rules read plus the shape, which is enough to show that two
/// sources lowering to the same projection are indistinguishable to every check in this module.
fn project(el: &Element) -> String {
    let mut out = format!(
        "{}|{}|type={:?}|from={:?}|to={:?}|dir={:?}|multi={:?}",
        el.kind.keyword(),
        el.name,
        el.type_ref,
        el.from,
        el.to,
        el.direction.map(|d| d.keyword()),
        el.multiplicity
    );
    for child in &el.children {
        out.push('\n');
        out.push_str(&project(child));
    }
    out
}

// -- validateStateDefinitionStateSubactionKind -------------------------------
//
// `ownedMembership->selectByKind(StateSubactionMembership)->isUnique(kind)`. `entry`, `do` and
// `exit` are the three values of `StateSubactionKind` (`SysML.sysml:376-380`), and the membership
// reaches the tree as `Kind::StateAction` named after the keyword.

#[test]
fn a_state_definition_with_two_entries_is_rejected() {
    let src = r#"
package P {
    state def S {
        entry action boot;
        entry action warm;
    }
}
"#;
    assert!(
        has(src, "P::S::warm", "second `entry` subaction"),
        "two `entry` memberships must be reported, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_state_definition_with_two_do_actions_is_rejected() {
    // `do` and `exit` are the other two kinds; the rule is per-kind, so each is checked.
    let src = r#"
package P {
    state def S {
        do action sense;
        do action log;
    }
}
"#;
    assert!(
        has(src, "P::S::log", "second `do` subaction"),
        "two `do` memberships must be reported, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_state_definition_with_two_exits_is_rejected() {
    let src = r#"
package P {
    state def S {
        exit action save;
        exit action flush;
    }
}
"#;
    assert!(
        has(src, "P::S::flush", "second `exit` subaction"),
        "two `exit` memberships must be reported, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_state_definition_with_one_of_each_subaction_is_accepted() {
    let src = r#"
package P {
    state def S {
        entry action boot;
        do action sense;
        exit action save;
    }
}
"#;
    assert!(
        sem023(src).is_empty(),
        "one of each kind is legal, got {:?}",
        sem023(src)
    );
}

#[test]
fn the_bare_entry_spelling_is_counted_too() {
    // `entry;` parses as a declaration and is retyped into a `StateAction` by the state's own
    // lowering pass (sysml-syntax/src/lower.rs:399-405). It carries neither `to` nor a type, so it
    // is a membership. Two of them are the same violation.
    let src = r#"
package P {
    state def S {
        entry;
        entry;
    }
}
"#;
    let state = named(src, "S");
    assert_eq!(
        state
            .children
            .iter()
            .filter(|c| c.kind == Kind::StateAction)
            .count(),
        2,
        "both bare `entry` members must be retyped, or this test is no longer testing the rule"
    );
    assert!(
        has(src, "P::S::entry", "second `entry` subaction"),
        "two bare `entry` memberships must be reported, got {:?}",
        sem023(src)
    );
}

#[test]
fn an_ordinary_feature_named_entry_is_not_a_subaction() {
    // The lowering retypes every `AttributeUsage` called `entry`/`do`/`exit` inside a state
    // (sysml-syntax/src/lower.rs:401-405), so `attribute entry : Real;` arrives as a state action
    // with a type and no behaviour. It is a feature, not a membership.
    let src = r#"
package P {
    state def S {
        attribute entry : Real;
        do action sense;
        attribute exit : Real;
    }
}
"#;
    let state = named(src, "S");
    let retyped: Vec<&Element> = state
        .children
        .iter()
        .filter(|c| c.kind == Kind::StateAction && c.to.is_none() && c.type_ref.is_some())
        .collect();
    assert_eq!(
        retyped.len(),
        2,
        "both attributes should have been retyped, or this test is no longer testing the guard"
    );
    assert!(
        sem023(src).is_empty(),
        "a feature named `entry` is not an entry membership, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_retyped_feature_does_not_hide_a_real_duplicate() {
    // The other half of the same guard: the retyped attribute must be skipped without letting a
    // genuine second `entry` through.
    let src = r#"
package P {
    state def S {
        attribute entry : Real;
        entry action boot;
        entry action warm;
    }
}
"#;
    assert_eq!(
        sem023(src).len(),
        1,
        "exactly one duplicate, got {:?}",
        sem023(src)
    );
    assert!(
        has(src, "P::S::warm", "second `entry` subaction"),
        "the two memberships must still be counted, got {:?}",
        sem023(src)
    );
}

// -- validateStateUsageStateSubactionKind ------------------------------------

#[test]
fn a_state_usage_with_two_entries_is_rejected() {
    let src = r#"
package P {
    state def Mode;
    state m : Mode {
        entry action boot;
        entry action warm;
    }
}
"#;
    assert!(
        has(src, "P::m::warm", "second `entry` subaction"),
        "two `entry` memberships on a state usage must be reported, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_state_usage_with_two_exits_is_rejected() {
    // The usage side of the pair, on the third kind: `exit`.
    let src = r#"
package P {
    state def Mode;
    state m : Mode {
        exit action save;
        exit action flush;
    }
}
"#;
    assert!(
        has(src, "P::m::flush", "second `exit` subaction"),
        "two `exit` memberships on a state usage must be reported, got {:?}",
        sem023(src)
    );
}

#[test]
fn an_exhibit_state_usage_is_judged_by_the_same_predicate() {
    // `ExhibitStateUsage` specializes `StateUsage` (generated.rs:1661), so it owns the same kind
    // of membership.
    let src = r#"
package P {
    state def Mode;
    part def Holder {
        exhibit state mode : Mode {
            entry action boot;
            entry action warm;
        }
    }
}
"#;
    assert!(
        has(src, "P::Holder::mode::warm", "second `entry` subaction"),
        "an exhibit state usage owns subactions too, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_state_usage_with_one_of_each_subaction_is_accepted() {
    let src = r#"
package P {
    state def Mode;
    state m : Mode {
        entry action boot;
        do action sense;
        exit action save;
    }
}
"#;
    assert!(
        sem023(src).is_empty(),
        "one of each kind is legal, got {:?}",
        sem023(src)
    );
}

// -- validateTransitionUsageSuccession ---------------------------------------
//
// `successions->notEmpty()` is true by construction: a `TransitionUsage` and its `Succession`
// lower to one `Kind::Transition` (sysml-syntax/src/lower.rs:744-767). What is decided here is the
// target half, `targetFeature.featureTarget->forAll(oclIsKindOf(ActionUsage))`, read off
// `Element::to`.

#[test]
fn a_transition_ending_at_a_part_definition_is_rejected() {
    let src = r#"
package P {
    part def Wheel;
    state def S {
        state a;
        transition t first a then Wheel;
    }
}
"#;
    assert!(
        has(src, "P::S::t", "must end at an action usage"),
        "a transition whose `then` end is a part def must be reported, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_transition_ending_at_an_item_definition_is_rejected() {
    let src = r#"
package P {
    item def Bolt;
    state def S {
        state a;
        transition t first a then Bolt;
    }
}
"#;
    assert!(
        has(src, "P::S::t", "must end at an action usage"),
        "the resolved kind is what matters, not the name, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_transition_ending_at_an_attribute_is_rejected() {
    // A feature of the state is not an `ActionUsage` either; the rule is about the metaclass, not
    // about whether the name resolves at all.
    let src = r#"
package P {
    state def S {
        attribute p : Real;
        state a;
        transition t first a then p;
    }
}
"#;
    assert!(
        has(src, "P::S::t", "must end at an action usage"),
        "an attribute is not an action usage, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_transition_ending_at_a_state_is_accepted() {
    // `StateUsage` specializes `ActionUsage` (generated.rs:1751), so a state is a legal end.
    let src = r#"
package P {
    state def S {
        state a;
        state b;
        transition t first a then b;
    }
}
"#;
    assert!(
        sem023(src).is_empty(),
        "a state is an action usage, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_transition_ending_at_a_plain_action_usage_is_accepted() {
    // Not only states: any `ActionUsage` will do, including one declared inside the state body.
    let src = r#"
package P {
    action def SomeAction;
    state def S {
        state a;
        action doIt : SomeAction;
        perform action go : SomeAction;
        transition t first a then doIt;
        transition u first a then go;
    }
}
"#;
    assert!(
        sem023(src).is_empty(),
        "an action usage and a perform action usage are legal ends, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_succession_in_an_action_body_is_not_a_transition_usage() {
    // A `TransitionUsage` occurs in a state body only (`stateBodyItem`,
    // SysMLv2Parser.g4:1701-1708); an action body writes a plain `Succession`, whose
    // `targetFeature` may be any feature. `first a then p` with `p` an attribute is legal, and it
    // lowers to the same `Kind::Transition`.
    let src = r#"
package P {
    action def Seq {
        action a;
        attribute p : Real;
        succession first a then p;
    }
}
"#;
    let transition = named(src, "succession");
    assert_eq!(
        transition.kind,
        Kind::Transition,
        "the model must still lower to a transition, or the guard is untested"
    );
    assert_eq!(
        transition.to.as_deref(),
        Some("p"),
        "the succession's target must be recorded, or the guard is untested"
    );
    assert!(
        sem023(src).is_empty(),
        "a succession outside a state is not this rule's subject, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_transition_to_an_unresolved_name_is_not_judged() {
    // `forAll` over an absent `featureTarget` is vacuously true: a reference that resolves to no
    // element has no metaclass to test. Reporting a *kind* violation would invent a fact the model
    // does not state. This is an under-report, and the module note records that nothing else in
    // the kernel resolves a transition's ends either.
    let src = r#"
package P {
    state def S {
        state a;
        transition t first a then Nowhere;
    }
}
"#;
    assert!(
        sem023(src).is_empty(),
        "an unresolved `then` end carries no kind to judge, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_transition_to_the_initial_node_is_not_judged() {
    // A pseudo state has no counterpart in the metamodel: `ElementKind` declares no `Pseudostate`
    // (no occurrence in research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs), and the
    // grammar spells the statement `FIRST qualifiedName` (SysMLv2Parser.g4:1475-1477), so `first`
    // there is a keyword and never a member name. The lowering still invents an element for it,
    // named `first`, with the target kept in `to` (sysml-syntax/src/lower.rs:769-773).
    //
    // `initialNodeMember` belongs to `actionBodyItem` (SysMLv2Parser.g4:1456), not to
    // `stateBodyItem` (:1701-1708), so the model below is not legal SysML. The parser accepts it
    // anyway, and `name()` takes a keyword token (sysml-parser/src/parser.rs:175-180), so
    // `then first` reaches the invented element. This test pins the guard that keeps the
    // projection's own artifact out of the report — it is not a statement about a legal model.
    let src = r#"
package P {
    state def S {
        state a;
        first start;
        transition t first a then first;
    }
}
"#;
    let state = named(src, "S");
    let pseudo = state
        .children
        .iter()
        .find(|c| c.kind == Kind::PseudoState)
        .expect("the projection must still produce a pseudo state, or this test is moot");
    assert_eq!(
        pseudo.name, "first",
        "the pseudo state is named after the keyword"
    );
    assert_eq!(
        pseudo.to.as_deref(),
        Some("start"),
        "the initial node's target is kept in `to`"
    );
    let transition = state
        .children
        .iter()
        .find(|c| c.kind == Kind::Transition)
        .expect("the transition must survive lowering");
    assert_eq!(
        transition.to.as_deref(),
        Some("first"),
        "the `then` end must name the projected element, or the guard is untested"
    );
    assert!(
        sem023(src).is_empty(),
        "a pseudo state is a projection artifact, not a metaclass, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_transition_may_end_at_a_declared_state_behaviour() {
    // `entry action boot;` owns a `StateSubactionMembership` whose action is a `ActionUsage`
    // named `boot` (SysML.sysml:382-386). The projection used to name the element after the
    // keyword and put `boot` in `to`, which made `boot` unreachable and `entry` resolve to the
    // membership — a name the metamodel does not give it. Both spellings were therefore illegal
    // as a `then` end, and the guard below existed to avoid judging the projection's shape.
    //
    // The element now carries the behaviour's own name and its role in `clause`, so `then boot`
    // resolves to a real action and is judged like any other end.
    let src = r#"
package P {
    state def S {
        entry action boot;
        state a;
        transition t first a then boot;
    }
}
"#;
    let state = named(src, "S");
    assert!(
        state.children.iter().any(|c| {
            c.kind == Kind::StateAction
                && c.name == "boot"
                && c.clause.as_deref() == Some("entry")
        }),
        "the membership projects as a state action named after the behaviour, with its role in `clause`"
    );
    let transition = state
        .children
        .iter()
        .find(|c| c.kind == Kind::Transition)
        .expect("the transition must survive lowering");
    assert_eq!(
        transition.to.as_deref(),
        Some("boot"),
        "the `then` end names the action, not the role keyword"
    );
    assert!(
        sem023(src).is_empty(),
        "a transition to a declared behaviour is legal, got {:?}",
        sem023(src)
    );
}

// -- the four rules that cannot run ------------------------------------------
//
// Each test below pins down the projection gap, so a later fix to the front end flips the test
// rather than leaving the rule silently unenforced.

#[test]
fn parallel_is_discarded_before_the_tree_is_built() {
    // `isParallel` has no field on `Element` (sysml-model/src/lib.rs:721-850), and the `parallel`
    // keyword is consumed as a multiplicity modifier and thrown away
    // (sysml-parser/src/parser.rs:1055-1060 and `:1063-1068`). So the model below — which violates
    // `validateStateUsageParallelSubactions`, because its parallel substates carry a transition —
    // is indistinguishable from the legal one, and the rule cannot be decided.
    let with_parallel = r#"
package P {
    state def Mode;
    state m : Mode parallel {
        state a;
        state b;
        transition t first a then b;
    }
}
"#;
    let without = r#"
package P {
    state def Mode;
    state m : Mode {
        state a;
        state b;
        transition t first a then b;
    }
}
"#;
    assert_eq!(
        project(&named(with_parallel, "m")),
        project(&named(without, "m")),
        "the keyword must leave no trace in the tree, or the rule becomes decidable"
    );
    assert!(
        sem023(with_parallel).is_empty(),
        "violating model, but nothing here can see `parallel`, got {:?}",
        sem023(with_parallel)
    );
}

#[test]
fn parallel_on_a_state_definition_is_discarded_too() {
    // `validateStateDefinitionParallelSubactions` needs the same flag on `StateDefinition`
    // (`SysML.sysml:368`), so it is dropped for the same reason.
    let with_parallel = r#"
package P {
    state def S parallel {
        state a;
        state b;
        transition t first a then b;
    }
}
"#;
    let without = r#"
package P {
    state def S {
        state a;
        state b;
        transition t first a then b;
    }
}
"#;
    assert_eq!(
        project(&named(with_parallel, "S")),
        project(&named(without, "S")),
        "the keyword must leave no trace in the tree, or the rule becomes decidable"
    );
    assert!(
        sem023(with_parallel).is_empty(),
        "violating model, but nothing here can see `parallel`, got {:?}",
        sem023(with_parallel)
    );
}

#[test]
fn a_transition_trigger_never_reaches_the_tree() {
    // The parser reads `accept <payload>` into a local and discards it
    // (sysml-parser/src/parser.rs:2254-2260, `:2294`). `validateTransitionUsageTriggerActions`
    // needs `triggerAction` on the transition, so its consequent is unreadable. The model below is
    // a transition inside an `action def` — its source is not a `StateUsage`, so a trigger there
    // is exactly what the rule forbids.
    let with_trigger = r#"
package P {
    action def Seq {
        action a;
        action b;
        transition t first a accept 'Sig' then b;
    }
}
"#;
    let without = r#"
package P {
    action def Seq {
        action a;
        action b;
        transition t first a then b;
    }
}
"#;
    assert_eq!(
        project(&named(with_trigger, "t")),
        project(&named(without, "t")),
        "the trigger must leave no trace in the tree, or the rule becomes decidable"
    );
    assert!(
        sem023(with_trigger).is_empty(),
        "violating model, but the trigger is gone, got {:?}",
        sem023(with_trigger)
    );
}

#[test]
fn transition_parameters_are_not_owned_by_the_transition() {
    // `transition t in p first a then b;` leaves `in p` in the stream, where it is re-parsed as a
    // sibling of the transition inside the state (sysml-parser/src/parser.rs:2238-2294 reads no
    // parameter). `validateTransitionUsageParameters` counts `inputParameters()` on the transition
    // and finds none, so it would fire on every legal transition — the model below satisfies the
    // rule and still has nothing the check could count.
    let with_parameter = r#"
package P {
    private import ScalarValues::*;
    attribute p : Real;
    state def S {
        state a;
        state b;
        transition t in p first a then b;
    }
}
"#;
    let transition = named(with_parameter, "t");
    assert!(
        transition.children.is_empty(),
        "the parameter must not be a child of the transition, or the rule becomes decidable; got {:?}",
        transition.children.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    let state = named(with_parameter, "S");
    assert!(
        state
            .children
            .iter()
            .any(|c| c.kind == Kind::Parameter && c.name == "p"),
        "the parameter must land as a sibling inside the state, got {:?}",
        state
            .children
            .iter()
            .map(|c| (c.kind.keyword(), c.name.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        sem023(with_parameter).is_empty(),
        "violating model, but its input parameter is not on the transition, got {:?}",
        sem023(with_parameter)
    );
}

#[test]
fn an_accept_when_transition_is_read_to_its_end() {
    // This test used to guard the opposite: `accept when <expr>` was not a form the parser
    // finished reading, so the guard's head became the trigger payload and the rest of the
    // statement — the `do` and the `then` — was re-parsed as members of the state. The parser now
    // reads `when`/`after` as whole expressions and records which keyword introduced the trigger,
    // so the statement is read to its end and nothing spills.
    let src = r#"
package P {
    attribute def Sig;
    part def VehicleA {
        attribute Tmax : Real;
    }
    part vehicle1_c1 : VehicleA;
    state def S {
        do action sense;
        state normal;
        transition bad
            first normal
            accept when vehicle1_c1.Tmax > 0
            do send new Sig() to vehicle1_c1
            then normal;
    }
}
"#;
    let state = named(src, "S");
    let transition = state
        .children
        .iter()
        .find(|c| c.kind == Kind::Transition && c.name == "bad")
        .expect("the transition must be a child of the state");
    assert_eq!(
        transition.to.as_deref(),
        Some("normal"),
        "the `then` clause must reach the transition"
    );
    assert!(
        !state
            .children
            .iter()
            .any(|c| c.kind == Kind::StateAction && c.name == "do" && c.line > transition.line),
        "`do send` must not spill into the state as a second `do`: {:?}",
        state
            .children
            .iter()
            .map(|c| (c.kind.keyword(), c.name.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        sem023(src).is_empty(),
        "the model states nothing the transition rules forbid, got {:?}",
        sem023(src)
    );
}

#[test]
fn a_trigger_and_its_target_stay_on_one_transition() {
    // The same statement, with a target that is not an action. Before the parser read `when` as an
    // expression, the tail of this statement was re-parsed and `then Wheel` became a second,
    // fabricated transition; this pins that one authored transition lowers to one element.
    let src = r#"
package P {
    part def Wheel;
    state def S {
        state a;
        transition bad first a accept when a then Wheel;
    }
}
"#;
    let state = named(src, "S");
    let transition = state
        .children
        .iter()
        .find(|c| c.kind == Kind::Transition && c.name == "bad")
        .expect("the transition must survive lowering");
    assert_eq!(
        transition.to.as_deref(),
        Some("Wheel"),
        "the target must reach the transition instead of being re-parsed as its own"
    );
    assert_eq!(
        state
            .children
            .iter()
            .filter(|c| c.kind == Kind::Transition)
            .count(),
        1,
        "one authored transition must lower to one element"
    );
}

// -- the `do` clause stays on the transition (parser) ----------------------

#[test]
fn a_guarded_accept_does_not_leave_a_phantom_do() {
    // `accept when <expr> do <action>`: the `when` keyword used to be read as the payload's name
    // and `<expr>` as its type, so the expression stayed in the stream and `do <action>` was
    // re-parsed as a state subaction. The state then carried a second `do` the model never wrote,
    // and `validateStateUsageStateSubactionKind` reported a duplicate on a conforming model.
    let src = r#"
package P {
	state def S {
		do sense;
		state a;
		state normal;
		accept when t > 0 do sense then normal;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let state = &loaded.roots[0].children[0];
    let dos = state
        .children
        .iter()
        .filter(|c| c.type_ref.as_deref().or(c.to.as_deref()) == Some("sense"))
        .count();
    assert_eq!(
        dos,
        1,
        "one `do` clause means one subaction: {:?}",
        state
            .children
            .iter()
            .map(|c| (c.kind.keyword(), c.name.as_str(), c.line))
            .collect::<Vec<_>>()
    );
    assert!(
        sem023(src).is_empty(),
        "the model states no duplicate subaction, got {:?}",
        sem023(src)
    );
}

#[test]
fn the_effect_clause_stays_on_the_transition() {
    // Before the parser consumed `do` after a bare succession's target, `do w` became a state
    // subaction of the enclosing machine: the effect the model wrote reached no element. What the
    // clause names is judged by `validatePerformActionUsageReference` (SEM044), because a `do` is a
    // `PerformedActionUsage`; this test pins only that the clause reaches the transition.
    let src = r#"
package P {
	part def Wheel;
	part w : Wheel;
	state def S {
		state a;
		state b;
		a then b do w;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let state = &loaded.roots[0].children[0];
    assert!(
        !state.children.iter().any(|c| c.name == "do"),
        "`do` must not become a member of the state: {:?}",
        state.children.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
}

#[test]
fn a_guard_is_reported_on_its_own_line() {
    // A transition may write each clause on its own line, and the guard is often the third of
    // four. `Element::line` is the `transition` keyword's line, so reporting the guard there put
    // the finding three lines above the construct a reader has to fix — and Xpect asserts this
    // rule `at "if \\"test\\""`, which is the guard's line
    // (`TransitionUsage_invalid.sysml.xt:60`, where the statement starts on `:58`).
    let src = r#"
package P {
    state def S {
        state a;
        state b;
        transition
            first a
            if "test"
            then b;
    }
}
"#;
    let found = issues_with_lines(src);
    let (_, line) = found
        .iter()
        .find(|(code, _)| code == "SEM061")
        .unwrap_or_else(|| panic!("the guard must be reported, got {found:?}"));
    assert_eq!(
        *line, 8,
        "the guard sits on the `if` line, not on `transition`; got {found:?}"
    );
}
