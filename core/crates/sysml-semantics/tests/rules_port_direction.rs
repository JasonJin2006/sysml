//! `port-direction` — a connector joins two ports whose directions can meet.
//!
//! Both sides of every test are here because the rule's value is entirely in not firing: a rule
//! that rejects `out` against `out` and also rejects the legal spelling costs more than it gives.

use sysml_semantics::{check, Severity};

fn sem097(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.code == "SEM097")
        .map(|i| {
            (
                match i.severity {
                    Severity::Error => "error".to_string(),
                    Severity::Warning => "warning".to_string(),
                },
                i.at.clone(),
                i.message.clone(),
            )
        })
        .collect()
}

/// A model with two parts joined by one connector, both typed by the same port definition.
fn joined(port: &str) -> String {
    format!(
        r#"
package P {{
    port def {port} {{ {dir} item value; }}
    part def A {{ port p : {port}; }}
    part def B {{ port q : {port}; }}
    part a : A;
    part b : B;
    connect a.p to b.q;
}}
"#,
        port = port_with(port),
        dir = direction_word(port),
    )
}

fn port_with(kind: &str) -> &'static str {
    match kind {
        "out" => "OutPort",
        "in" => "InPort",
        "inout" => "InOutPort",
        _ => "UndirectedPort",
    }
}

fn direction_word(kind: &str) -> &'static str {
    match kind {
        "out" => "out",
        "in" => "in",
        "inout" => "inout",
        _ => "",
    }
}

#[test]
fn two_producing_ports_are_reported() {
    let found = sem097(&joined("out"));
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].0, "warning", "a tap is unusual, not illegal");
    assert!(
        found[0].2.contains("producing"),
        "the message says which way both sides flow: {}",
        found[0].2
    );
}

#[test]
fn two_consuming_ports_are_reported() {
    let found = sem097(&joined("in"));
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("consuming"),
        "the message says which way both sides flow: {}",
        found[0].2
    );
}

#[test]
fn a_producer_and_a_consumer_are_accepted() {
    let src = r#"
package P {
    port def OutPort { out item value; }
    port def InPort { in item value; }
    part def A { port p : OutPort; }
    part def B { port q : InPort; }
    part a : A;
    part b : B;
    connect a.p to b.q;
}
"#;
    assert!(sem097(src).is_empty(), "got {:?}", sem097(src));
}

/// `inout` is both directions, so it meets anything.
#[test]
fn an_inout_port_meets_anything() {
    for other in ["out", "in", "inout"] {
        let src = format!(
            r#"
package P {{
    port def InOutPort {{ inout item value; }}
    port def Other {{ {word} item value; }}
    part def A {{ port p : InOutPort; }}
    part def B {{ port q : Other; }}
    part a : A;
    part b : B;
    connect a.p to b.q;
}}
"#,
            word = direction_word(other)
        );
        assert!(
            sem097(&src).is_empty(),
            "inout against {other} is legal, got {:?}",
            sem097(&src)
        );
    }
}

/// A port that states no direction would need the rule to compare `None` against `out`.
#[test]
fn a_port_with_no_direction_is_not_compared() {
    let src = r#"
package P {
    port def Bare { item value; }
    port def OutPort { out item value; }
    part def A { port p : Bare; }
    part def B { port q : OutPort; }
    part a : A;
    part b : B;
    connect a.p to b.q;
}
"#;
    assert!(sem097(src).is_empty(), "got {:?}", sem097(src));
}
#[test]
fn a_severity_is_a_warning_so_the_model_still_loads() {
    let found = sem097(&joined("out"));
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].0, "warning",
        "the language does not forbid this join, so it must not fail the model"
    );
}
