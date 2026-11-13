//! Two silent drops that an agent found while using the tool in anger.
//!
//! Neither was caught by a test, because both produced *plausible* output: a requirement that
//! passed, an action sequence that looked like a set. The failure mode is the one this whole
//! project exists to prevent — the tool reporting a clean result it has not earned.

use sysml_model::Kind;
use sysml_semantics::Verdict;

fn load(src: &str) -> sysml_syntax::Loaded {
    sysml_syntax::load_project(&[src], true).expect("model parses")
}

#[test]
fn every_require_constraint_in_a_body_is_evaluated() {
    // The first implementation returned the first constraint and ignored the rest. A
    // requirement whose first clause held and whose second did not was reported PASS — the
    // worst possible answer, because it is the one nobody investigates.
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute x : Real = 10; attribute y : Real = 100; }
    part s : S;
    requirement def TwoClauses {
        subject o : S;
        require constraint { s.x >= 5 }
        require constraint { s.y <= 50 }
    }
}
"#;
    let loaded = load(src);
    let report = sysml_semantics::check(&loaded.roots, &loaded.library);
    assert_eq!(report.verdicts.len(), 1);
    assert!(
        matches!(report.verdicts[0].verdict, Verdict::Fail(_)),
        "the second clause (100 <= 50) is false and must decide the verdict, got {:?}",
        report.verdicts[0].verdict
    );
}

#[test]
fn both_clauses_holding_still_passes() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S { attribute x : Real = 10; attribute y : Real = 100; }
    part s : S;
    requirement def Both {
        subject o : S;
        require constraint { s.x >= 5 }
        require constraint { s.y >= 50 }
    }
}
"#;
    let report = sysml_semantics::check(&load(src).roots, &load(src).library);
    assert!(
        matches!(report.verdicts[0].verdict, Verdict::Pass(_)),
        "both clauses hold, got {:?}",
        report.verdicts[0].verdict
    );
}

#[test]
fn the_conjunction_keeps_the_authored_order() {
    // A diagnostic that quotes the predicate back must read the way the author wrote it.
    let src = r#"
package P {
    private import ScalarValues::*;
    requirement def R {
        require constraint { 1 < 2 }
        require constraint { 2 < 3 }
    }
}
"#;
    let loaded = load(src);
    let mut predicate = None;
    fn find(el: &sysml_model::Element, out: &mut Option<String>) {
        if el.predicate.is_some() && out.is_none() {
            *out = el.constraint.clone();
        }
        for child in &el.children {
            find(child, out);
        }
    }
    for root in &loaded.roots {
        find(root, &mut predicate);
    }
    let text = predicate.expect("predicate lowered");
    assert_eq!(
        text, "(1 < 2) and (2 < 3)",
        "clauses are conjoined in the order written, each parenthesised so a nested  cannot swallow the join"
    );
}

#[test]
fn a_succession_becomes_an_ordering_edge() {
    // This used to vanish without a diagnostic, leaving an ordered sequence looking like an
    // unordered set. It is now an edge with two ends, which is what the statement means.
    let src = r#"
package P {
    action def Seq {
        action s1;
        action s2;
        succession first s1 then s2;
    }
}
"#;
    let loaded = load(src);
    let mut edges: Vec<(String, String)> = Vec::new();
    fn walk(el: &sysml_model::Element, out: &mut Vec<(String, String)>) {
        if el.kind == Kind::Transition {
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
        vec![("s1".to_string(), "s2".to_string())],
        "the succession must be an edge from `first` to `then`"
    );
}

#[test]
fn an_action_body_with_no_succession_leaves_no_marker() {
    let src = r#"
package P {
    action def Plain {
        action s1;
        action s2;
    }
}
"#;
    let loaded = load(src);
    let mut markers = 0;
    fn walk(el: &sysml_model::Element, out: &mut usize) {
        if el.kind == Kind::Unsupported {
            *out += 1;
        }
        for child in &el.children {
            walk(child, out);
        }
    }
    for root in &loaded.roots {
        walk(root, &mut markers);
    }
    assert_eq!(markers, 0, "nothing was dropped, so nothing is reported");
}
