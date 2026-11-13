//! What the semantics layer decides, and — just as important — what it refuses to decide.
//!
//! Every case here was a real hole: each one used to exit 0.

use sysml_semantics::{check, Severity, Verdict};

fn report(src: &str) -> sysml_semantics::Report {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
}

fn codes(src: &str) -> Vec<&'static str> {
    report(src).issues.iter().map(|i| i.code).collect()
}

#[test]
fn a_connection_between_ports_that_do_not_exist_is_an_error() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Box { port p : Real; }
    part a : Box;
    part b : Box;
    connect a.nonexistent to b.p;
}
"#;
    let r = report(src);
    assert!(
        r.issues
            .iter()
            .any(|i| i.code == "SEM001" && i.severity == Severity::Error),
        "validateConnectorRelatedFeatures: a connection end naming nothing must fail, got {:?}",
        codes(src)
    );
}

#[test]
fn a_connection_between_real_ports_passes() {
    let src = r#"
package P {
    port def Signal;
    part def Box { port p : Signal; port q : Signal; }
    part a : Box;
    part b : Box;
    connect a.p to b.q;
}
"#;
    assert!(report(src).issues.is_empty(), "got {:?}", codes(src));
}

#[test]
fn a_value_expression_reading_an_undeclared_name_is_an_error() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute x : Real = undefinedName / 2; }
}
"#;
    assert!(
        report(src).issues.iter().any(|i| i.code == "SEM003"),
        "the right-hand side used to be dropped entirely, got {:?}",
        codes(src)
    );
}

#[test]
fn a_value_expression_over_declared_attributes_passes() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S {
        attribute a : Real = 2;
        attribute b : Real = 3;
        attribute c : Real = a * b;
    }
}
"#;
    assert!(report(src).issues.is_empty(), "got {:?}", codes(src));
}

#[test]
fn a_constraint_that_holds_passes() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute mass : Real = 5; }
    requirement def R { subject s : S; require constraint { s.mass <= 6 } }
}
"#;
    let verdicts = report(src).verdicts;
    assert_eq!(verdicts.len(), 1);
    assert!(
        matches!(verdicts[0].verdict, Verdict::Pass(_)),
        "5 <= 6 holds, got {:?}",
        verdicts[0].verdict
    );
}

#[test]
fn a_constraint_that_does_not_hold_fails() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute mass : Real = 9; }
    requirement def R { subject s : S; require constraint { s.mass <= 6 } }
}
"#;
    let r = report(src);
    assert!(
        r.issues
            .iter()
            .any(|i| i.code == "SEM005" && i.severity == Severity::Error),
        "9 <= 6 is false and must be reported, got {:?}",
        codes(src)
    );
}

#[test]
fn a_constraint_over_a_valueless_attribute_is_inconclusive_not_pass() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute mass : Real; }
    requirement def R { subject s : S; require constraint { s.mass <= 6 } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Inconclusive(_)),
        "an attribute with no value cannot make a requirement pass, got {:?}",
        verdicts[0].verdict
    );
}

#[test]
fn a_trivially_true_constraint_is_flagged() {
    // `1 > 0` is syntactically a requirement and semantically a placeholder. It passed every
    // earlier gate, which is the most misleading result a tool can produce.
    let src = r#"
package P {
    private import ScalarValues::*;
    requirement def R { require constraint { 1 > 0 } }
}
"#;
    assert!(
        report(src).issues.iter().any(|i| i.code == "SEM007"),
        "a constraint reading no model element must be called out, got {:?}",
        codes(src)
    );
}

#[test]
fn a_comparison_across_units_is_inconclusive_not_wrong() {
    // `endurance` is computed by dividing, which loses the unit; comparing it against minutes
    // cannot be decided here. Saying "false" would be a lie the other way round.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S {
        attribute energy : Real = 360;
        attribute power : Real = 900;
        attribute endurance : Real = energy / power;
    }
    requirement def R { subject s : S; require constraint { s.endurance >= 20 [min] } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Inconclusive(_)),
        "a unit-crossing comparison must not be decided, got {:?}",
        verdicts[0].verdict
    );
}

#[test]
fn adding_like_units_keeps_the_unit_and_the_comparison() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S {
        attribute a : Real = 2.1 [kg];
        attribute b : Real = 3.4 [kg];
        attribute total : Real = a + b;
    }
    requirement def R { subject s : S; require constraint { s.total <= 6 [kg] } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Pass(_)),
        "kilograms plus kilograms compared against kilograms is decidable, got {:?}",
        verdicts[0].verdict
    );
}

#[test]
fn arithmetic_on_a_nonexistent_name_is_an_error_not_a_silent_zero() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute x : Real = ghost * 2; }
}
"#;
    assert!(
        report(src).issues.iter().any(|i| i.code == "SEM003"),
        "got {:?}",
        codes(src)
    );
}

#[test]
fn a_value_that_nests_into_a_feature_with_colons_is_reported() {
    // `f::a` names `a` inside what `f` is, and `a` is declared in `f`'s type rather than in `f`.
    // A feature's members are reached with `.`, which the pilot states as "Must be an accessible
    // feature (use dot notation for nesting)" (`FeaturePath_Invalid.sysml.xt:45`).
    let src = r#"
package P {
    part def F { part a; }
    part f : F;
    part def A { part g = f::a; }
}
"#;
    let issues = report(src).issues;
    assert!(
        issues
            .iter()
            .any(|i| i.code == "SEM003" && i.message.contains("nest into a feature")),
        "got {:?}",
        codes(src)
    );
}

#[test]
fn a_clean_model_produces_no_issues() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Wheel { attribute radius : Real = 0.3; }
    part def Cart {
        part w : Wheel;
        attribute mass : Real = 12;
    }
    requirement def RadiusBound { subject c : Cart; require constraint { c.w.radius >= 0.2 } }
    requirement def MassBound { subject c : Cart; require constraint { c.mass <= 20 } }
}
"#;
    let r = report(src);
    assert!(
        r.issues.is_empty(),
        "nothing here is wrong, got {:?}",
        r.issues.iter().map(|i| i.render()).collect::<Vec<_>>()
    );
    assert_eq!(r.unresolved_requirements(), 0);
}

/// A rate written with a slash is a unit, not an unknown symbol.
///
/// `unit_def` looked up one symbol at a time, so `30 [m/s]` and `60 [km/h]` had no dimension and a
/// comparison between them was reported as "not convertible" — a legal model left undecided
/// because of how its unit was spelled. The compound reader existed; the lookup never reached it.
#[test]
fn a_rate_written_with_a_slash_decides() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def V { attribute speed : Real = 30 [m/s]; }
    requirement def Slow { subject s : V; require constraint { s.speed <= 60 [km/h] } }
    requirement def Fast { subject s : V; require constraint { s.speed <= 150 [km/h] } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Fail(_)),
        "30 m/s is 108 km/h, which is over 60: {:?}",
        verdicts[0].verdict
    );
    assert!(
        matches!(verdicts[1].verdict, Verdict::Pass(_)),
        "and under 150: {:?}",
        verdicts[1].verdict
    );
}
/// be a verdict the model does not support.
#[test]
fn an_energy_compared_to_a_mass_stays_undecided() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def V {
        attribute energy : Real = 900 [Wh];
        attribute mass : Real = 1200 [kg];
    }
    requirement def R { subject s : V; require constraint { s.energy <= s.mass } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Inconclusive(_)),
        "an energy and a mass cannot be compared: {:?}",
        verdicts[0].verdict
    );
}

/// A value carrying a unit must carry a *base* spelling, not the authored one.
///
/// `20 [min]` is converted to 1200 at the bracket and then carried as `s`. Carrying `min` instead
/// meant the comparison multiplied by 60 a second time, so `[min]` against `[s]` was wrong in the
/// direction that hides: a true `>=` looked satisfied. Verified here through `20 [min] >= 1200 [s]`,
/// which is equality and must pass.
#[test]
fn a_converted_value_is_compared_in_base_units() {
    let src = r#"
package P {
    private import ScalarValues::*;
    requirement def R { require constraint { 20 [min] >= 1200 [s] } }
    requirement def W { require constraint { 20 [min] >= 0.1 [h] } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Pass(_)),
        "20 min is 1200 s: {:?}",
        verdicts[0].verdict
    );
    assert!(
        matches!(verdicts[1].verdict, Verdict::Pass(_)),
        "20 min is a third of an hour, over 0.1 h: {:?}",
        verdicts[1].verdict
    );
}

/// A derived unit is named by its dimension vector, in the vector's own order.
///
/// Five arms of the named-unit table were written in the order the unit is spelled (`M, L, T` for
/// a joule) rather than in the vector's order (`L, M, T`), so they were unreachable. Nothing
/// noticed while the fallback still produced an answer; once the compound path started depending
/// on the value being named, `[W*h] / [W]` stopped being a time and the project's endurance
/// requirement went undecided again.
#[test]
fn a_derived_unit_is_named_by_its_dimension() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S {
        attribute energy : Real = 900 [J];
        attribute power : Real = 300 [W];
        attribute endurance : Real = energy / power;
    }
    requirement def E { subject s : S; require constraint { s.endurance >= 2 [s] } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Pass(_)),
        "900 J over 300 W is 3 s, which is over 2 s: {:?}",
        verdicts[0].verdict
    );
}
/// A value that nothing reads is still checked.
///
/// The unit check used to reach a value only through a requirement's predicate, so
/// `attribute sum = mass + alt` over a `kg` and an `m` sat in a model reported as clean whenever
/// no requirement happened to read `sum`. That is the gap "units are not in the model" produced:
/// the arithmetic was wrong and nothing in the tool said so.
#[test]
fn a_value_whose_units_do_not_combine_is_reported_without_a_requirement() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S {
        attribute mass : Real = 1200 [kg];
        attribute alt : Real = 3000 [m];
        attribute sum : Real = mass + alt;
    }
}
"#;
    let r = report(src);
    let finding = r
        .issues
        .iter()
        .find(|i| i.code == "UNIT001")
        .unwrap_or_else(|| panic!("no UNIT001 in {:?}", codes(src)));
    assert!(
        finding.at.ends_with("S::sum"),
        "reported on the value that cannot be computed: {}",
        finding.at
    );
    assert!(
        finding.message.contains("kg") && finding.message.contains('m'),
        "and says which units: {}",
        finding.message
    );
}

/// Two lengths add up whatever their units, so nothing is reported.
#[test]
fn values_in_one_dimension_are_not_reported() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S {
        attribute a : Real = 3 [m];
        attribute b : Real = 200 [cm];
        attribute total : Real = a + b;
        attribute mass : Real = 1200 [kg];
    }
}
"#;
    assert!(
        !codes(src).contains(&"UNIT001"),
        "3 m plus 200 cm is a length: {:?}",
        codes(src)
    );
}

/// A value over an unresolved name is one finding, not two.
///
/// `SEM003` already reports the name; promoting the same failure to `UNIT001` as well would make
/// one mistake look like two problems with different codes.
#[test]
fn a_value_over_an_unknown_name_is_not_also_a_unit_error() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S {
        attribute broken : Real = missing + 1 [kg];
    }
}
"#;
    let found = codes(src);
    assert!(found.contains(&"SEM003"), "the name is reported: {found:?}");
    assert!(
        !found.contains(&"UNIT001"),
        "and not counted again as a unit problem: {found:?}"
    );
}

// -- a succession must name something that exists ---------------------------

/// A transition or succession naming nothing is an error.
///
/// The ends are authored text, not element references, so a typo on either end used to remove an
/// edge from the graph silently: the drawing had one fewer arrow, which is indistinguishable from
/// a model that never stated one. The state and action views draw exactly these edges.
#[test]
fn a_succession_to_a_state_that_does_not_exist_is_rejected() {
    let src = r#"
package P {
    state def Power {
        state off;
        state on;
        transition off then nosuchstate;
    }
}
"#;
    let found = codes(src);
    assert!(found.contains(&"SEM096"), "got {found:?}");
}

#[test]
fn a_succession_to_an_action_that_does_not_exist_is_rejected() {
    let src = r#"
package P {
    action def Mission {
        action survey;
        succession survey then notAnAction;
    }
}
"#;
    let found = codes(src);
    assert!(found.contains(&"SEM096"), "got {found:?}");
}

/// A well-formed machine is not reported, and neither is one whose ends are control nodes.
///
/// `merge continue` declares a node called `continue`, and a succession names it that way. The
/// node's own name is the declared name — the keyword is not part of it — so a rule that looked
/// for a state or an action by keyword would reject every correct control-flow model.
#[test]
fn a_succession_between_declared_nodes_is_accepted() {
    let src = r#"
package P {
    state def Power {
        state off;
        state on;
        transition off then on;
    }
    action def Mission {
        action survey;
        action plan;
        merge continue;
        first survey then continue;
        first continue then plan;
    }
}
"#;
    let found = codes(src);
    assert!(!found.contains(&"SEM096"), "got {found:?}");
}

// -- the language's own sequence functions ----------------------------------
//
// `sum`, `size`, `min` and friends are not model elements, so the resolver found nothing for
// them and every call came back "names no calculation". `10a-Analysis` writes
// `mass = sum((vehicle.engine.mass, ...))` and got no number from it.

#[test]
fn a_sum_over_a_sequence_is_evaluated() {
    let src = r#"
package P {
    requirement R { require constraint { sum((1, 2, 3)) == 6 } }
}
"#;
    let found = codes(src);
    assert!(
        !found.contains(&"SEM005"),
        "the sum is 6, so the requirement holds: {found:?}"
    );
    assert!(
        !found.contains(&"SEM004"),
        "`sum` is not a name the model forgot to declare: {found:?}"
    );
}

#[test]
fn a_sum_that_does_not_match_is_a_failure_not_a_silence() {
    let src = r#"
package P {
    requirement R { require constraint { sum((1, 2, 3)) == 7 } }
}
"#;
    assert!(
        codes(src).contains(&"SEM005"),
        "the sum is 6, so 7 does not hold: {:?}",
        codes(src)
    );
}

#[test]
fn size_counts_the_sequence_it_is_given() {
    let src = r#"
package P {
    requirement R { require constraint { size((1, 2, 3)) == 3 } }
}
"#;
    assert!(!codes(src).contains(&"SEM005"), "got {:?}", codes(src));
}

#[test]
fn a_usage_reads_the_value_its_type_declares() {
    // `part wheels : Wheel` writes no value; `Wheel` declares `mass = 10`. A model that states
    // its data on the definition and its structure on the usage had no number at all.
    let src = r#"
package P {
    part def Wheel {
        attribute mass : Real = 10;
    }
    part def Car {
        part wheels : Wheel;
        requirement R { require constraint { wheels.mass == 10 } }
    }
}
"#;
    assert!(!codes(src).contains(&"SEM005"), "got {:?}", codes(src));
}

#[test]
fn an_inherited_value_reaches_a_sum() {
    let src = r#"
package P {
    part def Wheel {
        attribute mass : Real = 10;
    }
    part def Car {
        part wheels : Wheel;
        requirement R { require constraint { sum((wheels.mass, 5)) == 15 } }
    }
}
"#;
    assert!(!codes(src).contains(&"SEM005"), "got {:?}", codes(src));
}

#[test]
fn a_member_reached_after_an_instance_chain_is_not_a_declaration() {
    // `v2.m::mm` is `v2`'s `m`, then `mm` of that instance. No declaration scope can name it, and
    // `::` after a chain is the wrong operator — `FeatureChain_invalid.kerml.xt:59` states it as
    // "Must be a valid feature". The resolver split on `::` alone, so `v2.m` never resolved and the
    // value was accepted.
    let src = r#"
package P {
    private import ScalarValues::*;
    struct V {
        feature m : Real {
            feature mm;
        }
    }
    feature v2 : V;
    feature x = v2.m::mm;
}
"#;
    assert!(
        codes(src).contains(&"SEM003"),
        "the path reaches a member of an instance: {:?}",
        codes(src)
    );
}

#[test]
fn a_member_the_grammar_rejects_is_an_error_not_a_note() {
    // `import X;` without a visibility keyword is a syntax error (`SysML.xtext:239-241`), and the
    // reference reports it as one (`Import_Visibility_Invalid.sysml.xt:23`). It used to reach the
    // caller as `ok: false` with an empty `issues` list and a `notModelled` note beside it, so a
    // reader consulting the verdict saw a clean model.
    let src = r#"
package P {
    private import ScalarValues::*;
    import ScalarValues;
}
"#;
    let r = report(src);
    assert!(
        r.issues
            .iter()
            .any(|i| i.code == "SYN001" && i.severity == Severity::Error),
        "a parse failure is an error, got {:?}",
        r.issues.iter().map(|i| i.code).collect::<Vec<_>>()
    );
}

#[test]
fn a_null_literal_is_read_as_a_value_not_as_unreadable_input() {
    // `null` used to lower to an empty `Opaque`, the same shape a construct the front end could
    // not read becomes. A constraint built on it then said "expression form is not evaluated",
    // which claims the parser did not understand the text. It did; null is simply not a number.
    let r = report(
        r#"
package P {
    private import ScalarValues::*;
    part def A { attribute x : Integer = null; }
    requirement R { subject a : A; require constraint { a.x == null } }
}
"#,
    );
    let messages: Vec<&str> = r.issues.iter().map(|i| i.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("`null` has no value")),
        "the reason names null: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("expression form is not evaluated")),
        "null is not an unreadable form: {messages:?}"
    );
}

#[test]
fn a_subject_bound_to_an_instance_reads_that_instance() {
    // `subject u : Ue = deployment;` binds the subject to one deployment. Without the binding the
    // evaluator resolves `u.cycle` through `Ue`, where the attribute has no value, and every
    // requirement written that way is inconclusive.
    let r = report(
        r#"
package P {
    private import ScalarValues::*;
    private import ISQ::*;
    attribute def Cycle [s];
    part def Ue { attribute cycle : Cycle; }
    part deployment : Ue { attribute :>> cycle = 20.48 [s]; }
    requirement R {
        subject u : Ue = deployment;
        require constraint { u.cycle > 10.24 [s] }
    }
}
"#,
    );
    assert!(
        r.verdicts
            .iter()
            .all(|v| !matches!(v.verdict, Verdict::Inconclusive(_))),
        "the bound subject has a value: {:?}",
        r.verdicts.iter().map(|v| &v.verdict).collect::<Vec<_>>()
    );
    assert!(
        matches!(r.verdicts[0].verdict, Verdict::Pass(_)),
        "20.48 > 10.24: {:?}",
        r.verdicts[0].verdict
    );
}

#[test]
fn a_subject_bound_to_a_value_below_the_threshold_fails() {
    let r = report(
        r#"
package P {
    private import ScalarValues::*;
    private import ISQ::*;
    attribute def Cycle [s];
    part def Ue { attribute cycle : Cycle; }
    part deployment : Ue { attribute :>> cycle = 5.12 [s]; }
    requirement R {
        subject u : Ue = deployment;
        require constraint { u.cycle > 10.24 [s] }
    }
}
"#,
    );
    assert!(
        matches!(r.verdicts[0].verdict, Verdict::Fail(_)),
        "5.12 is not above 10.24: {:?}",
        r.verdicts[0].verdict
    );
}
