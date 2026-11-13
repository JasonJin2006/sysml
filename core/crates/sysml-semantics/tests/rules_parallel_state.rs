//! A parallel state does not sequence its substates —
//! `validateStateDefinitionParallelSubactions` and
//! `validateStateUsageParallelSubactions`.

use sysml_semantics::{check, Severity};

fn sem059(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM059")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_parallel_state_sequencing_its_substates_is_rejected() {
    let src = r#"
package P {
	state parallel S {
		state a;
		state b;
		first a then b;
	}
}
"#;
    assert_eq!(sem059(src).len(), 1, "got {:?}", sem059(src));
}

#[test]
fn a_parallel_state_without_successions_is_accepted() {
    let src = r#"
package P {
	state parallel S {
		state a;
		state b;
	}
}
"#;
    assert!(sem059(src).is_empty());
}

#[test]
fn a_sequential_state_may_sequence_its_substates() {
    let src = r#"
package P {
	state S {
		state a;
		state b;
		first a then b;
	}
}
"#;
    assert!(
        sem059(src).is_empty(),
        "only a parallel state is barred, got {:?}",
        sem059(src)
    );
}

#[test]
fn parallel_does_not_become_the_states_name() {
    // Before the prefix arm, `state parallel S` named the state `parallel` and left `S` as a
    // stray attribute.
    let src = r#"
package P {
	state parallel S {
		state a;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let names: Vec<_> = loaded
        .roots
        .iter()
        .flat_map(|p| p.children.iter())
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, vec!["S".to_string()], "got {names:?}");
}

#[test]
fn a_bare_then_inside_a_parallel_state_is_rejected() {
    // `then S1_2;` writes only a target. The OCL is
    // `isParallel implies nestedAction.incomingTransition->isEmpty() and
    // nestedAction.outgoingTransition->isEmpty()` (`generated.rs:7474-7477`), so the incoming
    // transition from the enclosing state is enough to violate it — requiring both ends to name
    // a substate skipped this spelling (`TransitionUsage_invalid.sysml.xt:46`).
    let src = r#"
package P {
	state def S1 parallel {
		state S1_1;
		then S1_2;
		state S1_2;
	}
}
"#;
    let found = sem059(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("S1_2"),
        "the report names the substate the succession reaches: {found:?}"
    );
}

#[test]
fn the_succession_carries_the_report_not_the_state() {
    // The state is legal in shape; the succession is the declaration to remove. The pilot writes
    // its assertion at `then S1_2;`.
    let src = r#"
package P {
	state def S1 parallel {
		state S1_1;
		then S1_2;
		state S1_2;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let issues = check(&loaded.roots, &loaded.library).issues;
    let found: Vec<_> = issues.iter().filter(|i| i.code == "SEM059").collect();
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].line, 5, "the succession is on line 5: {found:?}");
}
