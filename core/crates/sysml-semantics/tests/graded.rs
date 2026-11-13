//! Checks added after the five-dimension run, each answering something the tool previously
//! got wrong or refused to answer.

use sysml_semantics::{check, Severity, Verdict};

fn report(src: &str) -> sysml_semantics::Report {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
}

fn codes(src: &str) -> Vec<&'static str> {
    report(src).issues.iter().map(|i| i.code).collect()
}

#[test]
fn a_subsystem_cannot_satisfy_a_requirement_about_the_whole_system() {
    // The spacecraft model did exactly this and the tool said nothing: a requirement whose
    // subject is the whole spacecraft was "satisfied" by one subsystem. Structurally wrong,
    // syntactically perfect — the same family as a dropped constraint clause.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Spacecraft { attribute mass : Real = 240; }
    part def Battery { attribute mass : Real = 20; }
    requirement def MassBudget { subject s : Spacecraft; require constraint { s.mass <= 300 } }
    part battery : Battery;
    satisfy MassBudget by battery;
}
"#;
    let r = report(src);
    assert!(
        r.issues
            .iter()
            .any(|i| i.code == "SEM009" && i.severity == Severity::Error),
        "a battery is not a spacecraft, got {:?}",
        codes(src)
    );
}

#[test]
fn a_subtype_can_satisfy_a_requirement_about_its_supertype() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def Base { attribute m : Real = 5; }
    part def Derived :> Base { }
    requirement def R { subject b : Base; require constraint { b.m <= 10 } }
    part d : Derived;
    satisfy R by d;
}
"#;
    assert!(
        report(src).issues.is_empty(),
        "Derived specializes Base, so it may satisfy it; got {:?}",
        codes(src)
    );
}

#[test]
fn an_exact_type_satisfaction_is_accepted() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute m : Real = 5; }
    requirement def R { subject b : S; require constraint { b.m <= 10 } }
    part s : S;
    satisfy R by s;
}
"#;
    assert!(report(src).issues.is_empty(), "got {:?}", codes(src));
}

#[test]
fn hours_compare_against_minutes() {
    // Not convertible used to mean undecided, which is a false negative on a requirement that
    // any engineer would call decided.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute e : Real = 0.4 [h]; }
    requirement def R { subject s : S; require constraint { s.e >= 20 [min] } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Pass(_)),
        "0.4 h is 24 min, which is at least 20; got {:?}",
        verdicts[0].verdict
    );
}

#[test]
fn centimetres_add_to_metres() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute total : Real = 3 [m] + 200 [cm]; }
    requirement def R { subject s : S; require constraint { s.total >= 4.9 [m] } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Pass(_)),
        "3 m + 200 cm is 5 m; got {:?}",
        verdicts[0].verdict
    );
}

#[test]
fn energy_over_power_is_a_duration() {
    // Wh / W is hours. Refusing to say so left the endurance requirement permanently
    // undecided in the project's own example model for three rounds.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S {
        attribute energy : Real = 360 [W*h];
        attribute power : Real = 900 [W];
        attribute endurance : Real = energy / power;
    }
    requirement def Endurance { subject s : S; require constraint { s.endurance >= 20 [min] } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Pass(_)),
        "360 Wh over 900 W is 24 min; got {:?}",
        verdicts[0].verdict
    );
}

#[test]
fn a_genuine_dimension_mismatch_stays_undecided() {
    // The point of the conversion table is not to answer everything. Hours against kilograms
    // is not a question about the system, and inventing an answer would be worse than saying
    // nothing.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute t : Real = 5 [h]; }
    requirement def R { subject s : S; require constraint { s.t >= 3 [kg] } }
}
"#;
    let verdicts = report(src).verdicts;
    assert!(
        matches!(verdicts[0].verdict, Verdict::Inconclusive(_)),
        "time against mass is not decidable, got {:?}",
        verdicts[0].verdict
    );
}

#[test]
fn a_succession_leaves_an_ordering_edge() {
    let src = r#"
package P {
    action def Launch {
        action release;
        action deploy;
        succession first release then deploy;
    }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let mut edges: Vec<(String, String)> = Vec::new();
    fn walk(el: &sysml_model::Element, out: &mut Vec<(String, String)>) {
        if el.kind == sysml_model::Kind::Transition {
            out.push((
                el.from.clone().unwrap_or_default(),
                el.to.clone().unwrap_or_default(),
            ));
        }
        for child in &el.children {
            walk(child, out);
        }
    }
    for root in &loaded.roots {
        walk(root, &mut edges);
    }
    assert_eq!(
        edges,
        vec![("release".to_string(), "deploy".to_string())],
        "an ordering must be visible as an edge"
    );
}
