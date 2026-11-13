//! `deriveUsageMayTimeVary` — which features an assignment is allowed to write.
//!
//! The derivation is a conjunction of four conditions, and the kernel used to answer with one of
//! them (`isVariable`). The case that exposes the difference is a **composite action nested in an
//! occurrence**: `item i { action d; }` gives `d` an owner that specializes `Occurrences::Occurrence`,
//! so a rule reading only the owner says `d` varies — but the derivation excludes it, because a
//! composite feature that is an `Actions::Action` is part of its owner's structure rather than a value
//! that changes over the owner's lifetime.
//!
//! Every test here is a pair or a triple, because the interesting property is not "this model is
//! rejected" but "this model is rejected and the one that differs by a single word is not". A rule
//! that rejected everything would pass a single-case test.

use sysml_semantics::{check, Severity};

/// Assignments the kernel rejects, by the line of the `assign`.
fn rejected(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM044")
        .map(|i| i.message.clone())
        .collect()
}

#[test]
fn a_composite_action_nested_in_an_item_cannot_be_assigned() {
    // The pilot asserts exactly this at `AssignmentActionUsage_invalid.sysml.xt:45`.
    let src = r#"
package P {
    item i {
        action d;
    }
    action def A {
        assign i.d := null;
    }
}
"#;
    assert_eq!(rejected(src).len(), 1, "got {:?}", rejected(src));
}

#[test]
fn a_reference_action_nested_in_an_item_can_be_assigned() {
    // `ref` makes the action a value rather than structure, so the exclusion does not apply. Without
    // this case the rule could be rejecting every nested action and still pass the test above.
    let src = r#"
package P {
    item i {
        ref action e;
    }
    action def A {
        assign i.e := null;
    }
}
"#;
    assert!(rejected(src).is_empty(), "got {:?}", rejected(src));
}

#[test]
fn an_attribute_in_an_item_can_be_assigned() {
    // An attribute is not an `Action`, so the composite exclusion does not reach it. This is the
    // case that separates "exclude composite actions" from "exclude everything composite".
    let src = r#"
package P {
    item i {
        attribute a;
    }
    action def A {
        assign i.a := null;
    }
}
"#;
    assert!(rejected(src).is_empty(), "got {:?}", rejected(src));
}

#[test]
fn a_variable_inside_an_occurrence_can_be_assigned() {
    let src = r#"
package P {
    occurrence def Engine {
        attribute var rpm : Integer;
    }
    action def Tune {
        assign rpm := 1;
    }
}
"#;
    assert!(rejected(src).is_empty(), "got {:?}", rejected(src));
}

#[test]
fn a_constant_is_never_writable() {
    // `constant` is a written denial and outranks the derivation in both directions.
    //
    // Two details of the notation matter here and both were got wrong first time round.
    // `constant` must precede the kind keyword: `RefPrefix` (`SysML.xtext:556-561`) puts it before
    // `BasicUsagePrefix` consumes the keyword, so `constant attribute rpm` is the legal order and
    // `attribute constant rpm` splits into two declarations. And the assignment must reach the
    // feature by a path that resolves — a bare `rpm` inside a sibling definition names nothing, so
    // the earlier version of this test never reached the variability question at all.
    let src = r#"
package P {
    occurrence def Engine {
        constant attribute rpm : Integer = 0;
    }
    action def Tune {
        assign Engine::rpm := 1;
    }
}
"#;
    assert_eq!(rejected(src).len(), 1, "got {:?}", rejected(src));
}

#[test]
fn a_variable_outside_any_occurrence_cannot_be_assigned() {
    // No owner that specializes `Occurrences::Occurrence` means the derivation's first condition
    // fails, and `var` alone does not rescue it: the value has nowhere to change.
    let src = r#"
package P {
    attribute def D {
        var attribute v : Integer;
    }
    action def A {
        assign D::v := 1;
    }
}
"#;
    assert_eq!(rejected(src).len(), 1, "got {:?}", rejected(src));
}
