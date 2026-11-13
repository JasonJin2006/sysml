//! Constructs the front end parses but does not model, and constructs it started modelling.
//!
//! The failure these tests guard is silence. Before the marker existed, a model with a state
//! machine lowered to a tree with no trace of it: `check` printed nothing, `graph` drew
//! nothing, and the tool called the model clean while half of it had been dropped. That is the
//! same defect class the predicate checker was built for, one level up.
//!
//! When a construct graduates from marker to model, its marker test flips into a "is modelled"
//! test rather than being deleted — the assertion then guards the lowering instead of the gap.

use sysml_model::Kind;

fn markers(src: &str) -> Vec<(String, u32)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let mut out = Vec::new();
    fn walk(el: &sysml_model::Element, out: &mut Vec<(String, u32)>) {
        if el.kind == Kind::Unsupported {
            out.push((el.name.clone(), el.line));
        }
        for child in &el.children {
            walk(child, out);
        }
    }
    for root in &loaded.roots {
        walk(root, &mut out);
    }
    out
}

fn count(src: &str, kind: Kind) -> usize {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let mut n = 0;
    fn walk(el: &sysml_model::Element, kind: &Kind, n: &mut usize) {
        if &el.kind == kind {
            *n += 1;
        }
        for child in &el.children {
            walk(child, kind, n);
        }
    }
    for root in &loaded.roots {
        walk(root, &kind, &mut n);
    }
    n
}

#[test]
fn a_state_machine_is_modelled_not_marked() {
    let src = r#"
package P {
    state def SafeMode { entry; then Nominal; }
    state def Nominal { entry; then SafeMode; }
}
"#;
    assert!(
        markers(src).is_empty(),
        "states are modelled; a marker here means the lowering regressed"
    );
    assert_eq!(count(src, Kind::StateDef), 2);
    assert_eq!(
        count(src, Kind::Transition),
        2,
        "both `then` statements became edges"
    );
    assert_eq!(
        count(src, Kind::StateAction),
        2,
        "both `entry` behaviours lowered"
    );
}

#[test]
fn a_transition_keeps_its_two_ends() {
    let src = r#"
package P {
    state def A { then B; }
    state def B;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let mut edge: Option<&sysml_model::Element> = None;
    fn find<'a>(el: &'a sysml_model::Element, out: &mut Option<&'a sysml_model::Element>) {
        if el.kind == Kind::Transition && out.is_none() {
            *out = Some(el);
        }
        for child in &el.children {
            find(child, out);
        }
    }
    for root in &loaded.roots {
        find(root, &mut edge);
    }
    let edge = edge.expect("the `then` became a transition");
    assert_eq!(
        edge.from.as_deref(),
        Some("A"),
        "source is the enclosing state"
    );
    assert_eq!(edge.to.as_deref(), Some("B"), "target is what `then` named");
}

/// `transition off then on` names its source, and the source is the state — not the machine.
///
/// The parser reads the first word after `transition` as the element's name and leaves a `first`
/// clause to fill the same slot, so this spelling arrives with `from` empty. Filling it with the
/// enclosing element recorded the state machine as the source of a transition between two of its
/// own states: every consumer reading `from -> to` then saw a different fact for this spelling than
/// for `transition then on` written inside a state.
#[test]
fn a_named_transition_takes_its_name_as_the_source_state() {
    let src = r#"
package P {
    state def Power {
        state off;
        state on;
        transition off then on;
    }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    fn find<'a>(el: &'a sysml_model::Element, out: &mut Option<&'a sysml_model::Element>) {
        if el.kind == Kind::Transition && out.is_none() {
            *out = Some(el);
        }
        for child in &el.children {
            find(child, out);
        }
    }
    let mut edge = None;
    for root in &loaded.roots {
        find(root, &mut edge);
    }
    let edge = edge.expect("the transition lowered");
    assert_eq!(
        edge.from.as_deref(),
        Some("off"),
        "the source is the state it names"
    );
    assert_eq!(edge.to.as_deref(), Some("on"));
}

#[test]
fn a_transition_guard_is_kept_as_a_predicate() {
    // A guard is a condition in the same sense a requirement's predicate is, so it lands in the
    // same field and is checked by the same code — names inside it are resolved.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Ctl { attribute armed : Real; }
    state def Armed { transition t if ctl.armed <= 0 then Disarmed; }
    state def Disarmed;
    part ctl : Ctl;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let mut guard: Option<String> = None;
    fn find(el: &sysml_model::Element, out: &mut Option<String>) {
        if el.kind == Kind::Transition && out.is_none() {
            *out = el.constraint.clone();
        }
        for child in &el.children {
            find(child, out);
        }
    }
    for root in &loaded.roots {
        find(root, &mut guard);
    }
    assert_eq!(
        guard.as_deref(),
        Some("ctl.armed <= 0"),
        "the guard keeps its authored condition"
    );
}

#[test]
fn an_action_is_modelled_not_marked() {
    let src = r#"
package P {
    action def Deploy { }
    action run : Deploy;
}
"#;
    assert!(markers(src).is_empty(), "actions are modelled now");
    assert_eq!(count(src, Kind::ActionDef), 1);
    assert_eq!(count(src, Kind::ActionUsage), 1);
}

#[test]
fn an_item_and_an_enum_are_modelled_not_marked() {
    let src = r#"
package P {
    item def TelemetryFrame { attribute size : Integer; }
    enum def ModeKind { enum nominal; enum safe; }
}
"#;
    assert!(markers(src).is_empty(), "items and enums are modelled now");
    assert_eq!(count(src, Kind::ItemDef), 1);
    assert_eq!(count(src, Kind::EnumDef), 1);
    assert_eq!(count(src, Kind::EnumValue), 2, "both enum values lowered");
}

#[test]
fn metadata_view_and_alias_are_modelled_not_marked() {
    // All three were markers until this round; they now lower to elements of their own, so the
    // test guards the lowering instead of the gap. What remains unmodelled is reported through
    // the same channel — see the `variant` and `bind` cases below.
    let src = r#"
package P {
    part def Wheel;
    metadata def Safety { attribute level : Integer; }
    alias Hub for Wheel;
    view def StructureView;
}
"#;
    assert!(
        markers(src).is_empty(),
        "metadata, alias and view are modelled now"
    );
    assert_eq!(count(src, Kind::MetadataDef), 1);
    assert_eq!(count(src, Kind::Alias), 1);
    assert_eq!(count(src, Kind::ViewDef), 1);
}

#[test]
fn an_alias_keeps_its_target() {
    let src = r#"
package P {
    part def Wheel;
    alias Hub for Wheel;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let mut target: Option<String> = None;
    fn find(el: &sysml_model::Element, out: &mut Option<String>) {
        if el.kind == Kind::Alias && out.is_none() {
            *out = el.type_ref.clone();
        }
        for child in &el.children {
            find(child, out);
        }
    }
    for root in &loaded.roots {
        find(root, &mut target);
    }
    assert_eq!(
        target.as_deref(),
        Some("Wheel"),
        "the alias must keep what it points at, or it is just a name"
    );
}

#[test]
fn a_fully_modelled_model_has_no_markers() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Wheel { attribute radius : Real; }
    part def Cart { part w : Wheel; }
    state def Idle { entry; then Moving; }
    state def Moving { then Idle; }
    item def Load { attribute mass : Real; }
    enum def Mode { enum idle; enum moving; }
    alias Hub for Wheel;
    metadata def Safety { attribute level : Integer; }
    view def StructureView;
    requirement def R { subject c : Cart; require constraint { c.w.radius >= 1 } }
}
"#;
    assert!(
        markers(src).is_empty(),
        "structure, ports, states, actions, items, enums, aliases, metadata and views are modelled"
    );
}

#[test]
fn a_marker_always_carries_a_keyword_a_reader_can_act_on() {
    // The rule this whole module exists to enforce: no construct disappears, and no marker
    // says only "unsupported". A report line has to name the thing the tool still owes.
    // Variants are the construct still unmodelled in quantity, so they are the probe.
    let src = r#"
package P {
    part def Engine;
    part def Car { variant part e : Engine; }
}
"#;
    let found = markers(src);
    for (name, _) in &found {
        assert!(!name.is_empty(), "a marker with no name says nothing");
        assert_ne!(
            name, "unsupported",
            "a generic marker tells a reader nothing they can act on"
        );
    }
}

/// A message keeps the feature it is addressed to.
///
/// `send ping to ground` used to consume `to ground` and drop it, so the element recorded what it
/// carried and not where it went. That is the one fact a sequence view is made of, and no rule
/// could ask whether the receiver exists.
#[test]
fn a_message_keeps_the_feature_it_is_addressed_to() {
    let src = r#"
package P {
    part def Ground;
    part ground : Ground;
    action def Contact {
        send ping to ground;
        accept beacon;
    }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let mut sent: Option<&sysml_model::Element> = None;
    let mut accepted: Option<&sysml_model::Element> = None;
    fn find<'a>(
        el: &'a sysml_model::Element,
        sent: &mut Option<&'a sysml_model::Element>,
        accepted: &mut Option<&'a sysml_model::Element>,
    ) {
        if el.kind == Kind::SendActionUsage {
            *sent = Some(el);
        }
        if el.kind == Kind::AcceptActionUsage {
            *accepted = Some(el);
        }
        for child in &el.children {
            find(child, sent, accepted);
        }
    }
    for root in &loaded.roots {
        find(root, &mut sent, &mut accepted);
    }
    let sent = sent.expect("the send lowered");
    assert_eq!(
        sent.addressed.as_deref(),
        Some("ground"),
        "where the message goes"
    );
    assert_eq!(
        sent.to.as_deref(),
        Some("ping"),
        "what it carries stays on `to`"
    );
    let accepted = accepted.expect("the accept lowered");
    assert_eq!(accepted.addressed, None, "an accept names no destination");
}

#[test]
fn a_kerml_step_is_a_feature_not_an_attribute() {
    // A `step` is a `Step` — a `Feature` that is also a `Behavior` — and lowering it to the
    // attribute kind made SysML's attribute rules judge it: `composite step b : B { in x1; }` was
    // reported as "an attribute usage is always referential, but it states `composite`" and as
    // "an attribute typed by `B`, a behavior def" (`Redefinition_DirectionConformance_invalid
    // .kerml.xt:28`). Neither statement is about a step.
    let loaded = sysml_syntax::load_project(
        &["package P {\n\tbehavior B;\n\tbehavior A { composite step b : B { in x1; } }\n}"],
        true,
    )
    .expect("model parses");
    fn find<'a>(el: &'a sysml_model::Element, name: &str) -> Option<&'a sysml_model::Element> {
        if el.name == name {
            return Some(el);
        }
        el.children.iter().find_map(|c| find(c, name))
    }
    let b = find(&loaded.roots[0], "b").expect("the step");
    assert_eq!(
        b.kind,
        Kind::KermlFeature,
        "a step is a KerML feature, got {:?}",
        b.kind
    );
}
