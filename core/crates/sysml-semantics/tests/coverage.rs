//! What the kernel learned to read: the constructs that used to be dropped, marked, or judged
//! wrongly. Each case here was a real hole found by running the official validation corpus.
//!
//! The tests are written against the *outcome* — a verdict, a resolved name, an error code —
//! rather than against the element tree, so a later restructuring of the lowering does not have
//! to rewrite them.

use sysml_semantics::{check, Severity, Verdict};

fn loaded(src: &str) -> sysml_syntax::Loaded {
    sysml_syntax::load_project(&[src], true).expect("model parses")
}

fn report(src: &str) -> sysml_semantics::Report {
    let l = loaded(src);
    check(&l.roots, &l.library)
}

/// The verdict of the requirement named `name`, if it produced one.
fn verdict_of(src: &str, name: &str) -> Option<Verdict> {
    let l = loaded(src);
    let c = check(&l.roots, &l.library);
    let _ = &c;
    let checker = sysml_semantics::Checker::new(&l.roots, &l.library);
    checker
        .verdicts()
        .into_iter()
        .find(|v| v.qualified.ends_with(name))
        .map(|v| v.verdict)
}

fn codes(src: &str) -> Vec<&'static str> {
    report(src).issues.iter().map(|i| i.code).collect()
}

// -- specialization and inheritance ------------------------------------------

#[test]
fn a_feature_declared_on_a_supertype_is_reachable_through_a_usage() {
    // `part def D :> Base` inherits Base's features. Before this worked, `d.m` was unresolved:
    // a correctly written requirement reported SEM004 and could never be decided.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Base { attribute m : Real = 10; }
    part def D :> Base;
    part d : D;
    requirement def R { subject s : D; require constraint { d.m >= 5 } }
}
"#;
    assert_eq!(
        verdict_of(src, "R"),
        Some(Verdict::Pass(sysml_semantics::Value::Bool(true)))
    );
}

#[test]
fn inheritance_walks_the_whole_chain() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def L3 { attribute deep : Real = 42; }
    part def L2 :> L3;
    part def L1 :> L2;
    part x : L1;
    requirement def R { subject s : L1; require constraint { x.deep == 42 } }
}
"#;
    assert!(
        !codes(src).contains(&"SEM004"),
        "the feature three levels up must resolve"
    );
}

#[test]
fn a_multi_parent_specialization_sees_every_parent() {
    // `part def C :> A, B` is multiple inheritance. Reading only the first parent made `c` not a
    // `B`, so a valid `satisfy` was rejected and `c.y` was unresolved.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def A { attribute x : Real; }
    part def B { attribute y : Real = 5; }
    part def C :> A, B;
    part c : C;
    requirement def R { subject s : B; require constraint { c.y >= 1 } }
    satisfy R by c;
}
"#;
    let r = report(src);
    assert!(
        !r.issues.iter().any(|i| i.code == "SEM009"),
        "c really is a B, so the satisfy is well typed: {:?}",
        r.issues.iter().map(|i| i.render()).collect::<Vec<_>>()
    );
    assert!(
        !r.issues.iter().any(|i| i.code == "SEM004"),
        "c.y is declared on the second parent"
    );
}

#[test]
fn a_redefinition_supplies_the_value_that_is_read() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Base { attribute m : Real = 10; }
    part def D :> Base { attribute m : Real :>> m = 20; }
    part d : D;
    requirement def Is20 { subject s : D; require constraint { d.m == 20 } }
    requirement def Is10 { subject s : D; require constraint { d.m == 10 } }
}
"#;
    assert_eq!(
        verdict_of(src, "Is20"),
        Some(Verdict::Pass(sysml_semantics::Value::Bool(true))),
        "the redefined value must win"
    );
    assert!(
        matches!(verdict_of(src, "Is10"), Some(Verdict::Fail(_))),
        "the inherited value must not also hold"
    );
}

// -- assumptions and requirements are different claims ------------------------

#[test]
fn a_failed_assumption_is_reported_as_an_assumption() {
    // An assumption that does not hold means the model's basis is broken. Conjoining it into the
    // requirement made a false premise read as a failed design.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def D { attribute m : Real = -5; }
    requirement def R {
        subject s : D;
        assume constraint { s.m > 0 }
        require constraint { s.m >= -10 }
    }
}
"#;
    let c = codes(src);
    assert!(c.contains(&"SEM011"), "the assumption is reported: {c:?}");
    assert!(!c.contains(&"SEM005"), "the requirement does hold: {c:?}");
}

#[test]
fn a_failed_requirement_is_not_reported_as_a_broken_assumption() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def D { attribute m : Real = 10; }
    requirement def R {
        subject s : D;
        assume constraint { s.m > 0 }
        require constraint { s.m >= 100 }
    }
}
"#;
    let c = codes(src);
    assert!(c.contains(&"SEM005"), "the requirement fails: {c:?}");
    assert!(!c.contains(&"SEM011"), "the assumption holds: {c:?}");
}

// -- units -------------------------------------------------------------------

fn unit_verdict(lhs: &str, op: &str, rhs: &str) -> Verdict {
    let src = format!(
        r#"
package P {{
    private import ScalarValues::*;
    part def E {{ attribute v : Real = {lhs}; }}
    requirement def R {{ subject s : E; require constraint {{ s.v {op} {rhs} }} }}
}}
"#
    );
    verdict_of(&src, "R").expect("a verdict")
}

#[test]
fn prefixed_units_convert() {
    // `kN` was not in the old table, and an unknown unit silently became a bare number, so this
    // comparison was decided — wrongly. Prefixed SI units now convert through one prefix table.
    assert_eq!(
        unit_verdict("10 [kN]", ">=", "5000 [N]"),
        Verdict::Pass(sysml_semantics::Value::Bool(true))
    );
    assert!(matches!(
        unit_verdict("10 [kN]", ">=", "50000 [N]"),
        Verdict::Fail(_)
    ));
}

#[test]
fn units_of_the_same_dimension_convert_across_scales() {
    assert_eq!(
        unit_verdict("0.4 [h]", ">=", "20 [min]"),
        Verdict::Pass(sysml_semantics::Value::Bool(true))
    );
    assert_eq!(
        unit_verdict("1000 [m]", ">=", "1 [km]"),
        Verdict::Pass(sysml_semantics::Value::Bool(true))
    );
    assert_eq!(
        unit_verdict("1 [MPa]", ">=", "1e5 [Pa]"),
        Verdict::Pass(sysml_semantics::Value::Bool(true))
    );
}

#[test]
fn different_dimensions_are_undecided_not_true() {
    // `1 [N] == 1 [V]` used to evaluate `true`, because both sides were the number 1 once the
    // brackets were dropped. A wrong verdict that looks decided is the worst output available.
    assert!(
        matches!(
            unit_verdict("1 [N]", "==", "1 [V]"),
            Verdict::Inconclusive(_)
        ),
        "a force is not a voltage"
    );
}

#[test]
fn an_unknown_unit_is_still_a_unit() {
    // Two values in the same unknown unit need no conversion table to be compared.
    assert_eq!(
        unit_verdict("1 [furlong]", "==", "1 [furlong]"),
        Verdict::Pass(sysml_semantics::Value::Bool(true))
    );
}

// -- evaluation --------------------------------------------------------------

#[test]
fn a_conditional_is_evaluated_by_branch() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def E { attribute x : Real = 5; }
    part e : E;
    requirement def R { subject s : E; require constraint { e.x > 0 and (if e.x > 0 ? 10 else 20) == 10 } }
}
"#;
    assert_eq!(
        verdict_of(src, "R"),
        Some(Verdict::Pass(sysml_semantics::Value::Bool(true)))
    );
}

#[test]
fn a_type_test_is_decided_against_the_specialization_graph() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Engine;
    part def V6 :> Engine;
    part def Car { part engine : V6; }
    requirement def Direct { require constraint { Car::engine istype V6 } }
    requirement def ViaSuper { require constraint { Car::engine istype Engine } }
}
"#;
    assert_eq!(
        verdict_of(src, "Direct"),
        Some(Verdict::Pass(sysml_semantics::Value::Bool(true)))
    );
    assert_eq!(
        verdict_of(src, "ViaSuper"),
        Some(Verdict::Pass(sysml_semantics::Value::Bool(true))),
        "a V6 is an Engine"
    );
}

// -- nothing is dropped without a trace --------------------------------------

#[test]
fn a_construct_this_layer_does_not_model_is_named_in_the_report() {
    // The rule the whole front end exists to keep: a construct either lowers to something a
    // reader can act on, or appears in the report under its own keyword.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def E;
    requirement def R { subject s : E; require constraint { true } }
}
"#;
    let r = report(src);
    for issue in &r.issues {
        assert!(
            issue.severity != Severity::Error || !issue.message.contains("unsupported unsupported"),
            "a marker must name the construct, got: {}",
            issue.render()
        );
    }
}

#[test]
fn a_calculation_keeps_its_formula() {
    let l = loaded(
        r#"
package P {
    private import ScalarValues::*;
    calc def Power { in torque : Real; in speed : Real; return : Real = torque * speed; }
}
"#,
    );
    let calc = l
        .roots
        .iter()
        .find(|e| e.name == "P")
        .and_then(|p| p.children.iter().find(|c| c.name == "Power"))
        .expect("the calculation");
    assert!(
        calc.constraint
            .as_deref()
            .is_some_and(|c| c.contains("torque") && c.contains("speed")),
        "the returned expression is the calculation: {:?}",
        calc.constraint
    );
}

#[test]
fn an_interface_definition_keeps_its_ends_and_connector() {
    let l = loaded(
        r#"
package P {
    private import ScalarValues::*;
    port def PP { in fuel : Real; }
    interface def Link { end a : PP; end b : PP; connect a to b; }
}
"#,
    );
    let iface = l
        .roots
        .iter()
        .find(|e| e.name == "P")
        .and_then(|p| p.children.iter().find(|c| c.name == "Link"))
        .expect("the interface");
    let ends = iface
        .children
        .iter()
        .filter(|c| c.kind == sysml_model::Kind::PortUsage)
        .count();
    assert_eq!(ends, 2, "both ends are ends");
    assert!(
        iface
            .children
            .iter()
            .any(|c| c.kind == sysml_model::Kind::Connection),
        "the connect statement is kept"
    );
}

#[test]
fn a_constraint_definition_keeps_its_condition() {
    let l = loaded(
        r#"
package P {
    private import ScalarValues::*;
    constraint def Positive { in m : Real; m > 0 }
}
"#,
    );
    let def = l
        .roots
        .iter()
        .find(|e| e.name == "P")
        .and_then(|p| p.children.iter().find(|c| c.name == "Positive"))
        .expect("the constraint");
    assert_eq!(
        def.constraint.as_deref(),
        Some("m > 0"),
        "the expression body is the constraint"
    );
}

#[test]
fn an_action_keeps_its_parameters_and_control_flow() {
    let l = loaded(
        r#"
package P {
    private import ScalarValues::*;
    action def Deploy {
        in target : Real;
        action prep;
        if 1 > 0 then action launch; else action abort;
    }
}
"#,
    );
    let act = l
        .roots
        .iter()
        .find(|e| e.name == "P")
        .and_then(|p| p.children.iter().find(|c| c.name == "Deploy"))
        .expect("the action");
    assert!(
        act.children
            .iter()
            .any(|c| c.kind == sysml_model::Kind::Parameter && c.name == "target"),
        "the parameter is part of the signature"
    );
    let branch = act
        .children
        .iter()
        .find(|c| c.kind == sysml_model::Kind::ControlFlow && c.name == "if")
        .expect("the conditional");
    assert_eq!(
        branch.constraint.as_deref(),
        Some("1 > 0"),
        "the condition is kept"
    );
    assert_eq!(
        branch
            .children
            .iter()
            .filter(|c| c.kind == sysml_model::Kind::ControlFlow)
            .count(),
        2,
        "both branches are kept"
    );
}

#[test]
fn an_applied_metadata_annotation_keeps_its_arguments() {
    let l = loaded(
        r#"
package P {
    metadata def Safety { attribute level : Integer; }
    part def W;
    part wheel {@Safety{level = 3;}}
}
"#,
    );
    let wheel = l
        .roots
        .iter()
        .find(|e| e.name == "P")
        .and_then(|p| p.children.iter().find(|c| c.name == "wheel"))
        .expect("the part");
    let meta = wheel
        .children
        .iter()
        .find(|c| c.kind == sysml_model::Kind::MetadataUsage)
        .expect("the annotation");
    // Anonymous and typed, not named after its type: `@Safety{}` declares a usage *typed by* Safety,
    // and naming it Safety made it a symbol that shadowed the type — an annotation naming a type
    // that does not exist then resolved its own `type_ref` to itself and passed `check`.
    assert_eq!(
        meta.name, "<anonymous>",
        "an annotation is not a declaration of its type's name"
    );
    assert_eq!(meta.type_ref.as_deref(), Some("Safety"), "the applied type");
    let arg = meta
        .children
        .iter()
        .find(|c| c.name == "level")
        .expect("the argument");
    assert_eq!(
        arg.value_expr.as_ref().map(|e| e.render()).as_deref(),
        Some("3"),
        "the value written for `level` is kept"
    );
}

#[test]
fn a_variant_alternative_keeps_its_own_body() {
    let l = loaded(
        r#"
package P {
    private import ScalarValues::*;
    part def Base;
    variation part config {
        variant part alt : Base { attribute x : Real = 1; }
    }
}
"#,
    );
    let config = l
        .roots
        .iter()
        .find(|e| e.name == "P")
        .and_then(|p| p.children.iter().find(|c| c.name == "config"))
        .expect("the variation point");
    let alt = config
        .children
        .iter()
        .find(|c| c.is_variant)
        .expect("the alternative");
    assert_eq!(alt.name, "alt", "the alternative is named, not anonymous");
    assert!(
        alt.children.iter().any(|c| c.name == "x"),
        "the alternative's body is kept"
    );
}
