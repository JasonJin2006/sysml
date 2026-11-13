//! `validateElementFilterMembershipConditionIsBoolean`: a filter selects with a predicate.
//!
//! `validateElementFilterMembershipConditionIsModelLevelEvaluable`: and the predicate is a
//! constant, so a condition that needs a function the kernel does not evaluate is rejected.

use sysml_semantics::{check, Severity};

fn sem087(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM087")
        .map(|i| i.message.clone())
        .collect()
}

const PRELUDE: &str = r#"
package P {
	attribute def Real;
	attribute def Boolean;
	attribute mass : Real = 2;
	attribute ready : Boolean = true;
"#;

#[test]
fn a_filter_on_a_non_boolean_is_rejected() {
    let src = format!("{PRELUDE}\tview def V {{ filter mass; }}\n}}\n");
    let found = sem087(&src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].contains("validateElementFilterMembershipConditionIsBoolean"),
        "got {found:?}"
    );
}

#[test]
fn a_filter_on_a_boolean_is_accepted() {
    let src = format!("{PRELUDE}\tview def V {{ filter ready; }}\n}}\n");
    assert!(sem087(&src).is_empty(), "got {:?}", sem087(&src));
}

#[test]
fn a_filter_on_a_comparison_is_accepted() {
    let src = format!("{PRELUDE}\tview def V {{ filter mass > 0; }}\n}}\n");
    assert!(sem087(&src).is_empty(), "got {:?}", sem087(&src));
}

#[test]
fn a_filter_on_a_metadata_tag_is_not_judged() {
    // `@Safety` reaches the tree as an opaque metadata access: nothing in it says what the tag's
    // value is, so the rule leaves the model alone rather than reporting every filter.
    let src = r#"
package P {
	metadata def Safety;
	view def V { filter @Safety; }
}
"#;
    assert!(sem087(src).is_empty(), "got {:?}", sem087(src));
}

#[test]
fn a_filter_joining_two_tags_is_accepted() {
    // The corpus's shape: `filter @Safety | @Security;` — the join fixes the value as Boolean
    // whatever the tags themselves are.
    let src = r#"
package P {
	metadata def Safety;
	metadata def Security;
	view def V { filter @Safety | @Security; }
}
"#;
    assert!(sem087(src).is_empty(), "got {:?}", sem087(src));
}

#[test]
fn a_filter_over_a_conjugation_is_rejected() {
    // `filter ~ready` — `~` written where a value belongs is a conjugation, and no function of the
    // operand's value is applied, so there is nothing to evaluate when the view is selected. The
    // Boolean half is silent here: `~ready` is still claimed to be a Boolean.
    let src = format!("{PRELUDE}\tview def V {{ filter ~ready; }}\n}}\n");
    let found = sem087(&src);
    assert!(
        found
            .iter()
            .any(|m| m.contains("validateElementFilterMembershipConditionIsModelLevelEvaluable")),
        "got {found:?}"
    );
}

#[test]
fn a_filter_calling_a_model_function_is_rejected() {
    // `f(1)` is a call to a function the model declares, so whether it has a value depends on a
    // body this layer does not read. The pilot states the same verdict for `filter f((as A).y)`
    // (`MetadataFeature_invalid.kerml.xt:54`).
    let src = r#"
package P {
	attribute def Boolean;
	function f { in x; return : Boolean; }
	view def V { filter f(1); }
}
"#;
    let found = sem087(src);
    assert!(
        found
            .iter()
            .any(|m| m.contains("validateElementFilterMembershipConditionIsModelLevelEvaluable")),
        "got {found:?}"
    );
}

#[test]
fn a_filter_on_a_language_function_is_not_judged() {
    // `sum` is one of the functions the evaluator computes, so calling it does not make the
    // condition non-constant. A builtin's argument is a number here, which is a value already.
    let src = r#"
package P {
	attribute def Boolean;
	view def V { filter sum((1, 2)) > 0; }
}
"#;
    assert!(sem087(src).is_empty(), "got {:?}", sem087(src));
}

#[test]
fn a_cast_without_an_operand_does_not_decide_the_verdict() {
    // The corpus's shape: `filter @Safety and (as Safety).isMandatory;`. The cast's operand is the
    // element the clause sits on, which is not in the tree, so the answer is "cannot tell" rather
    // than "not a constant" — reporting it would flag a legal model.
    let src = r#"
package P {
	metadata def Safety {
		attribute isMandatory : Boolean;
	}
	view def V { filter @Safety and (as Safety).isMandatory; }
}
"#;
    assert!(sem087(src).is_empty(), "got {:?}", sem087(src));
}

#[test]
fn the_condition_reaches_the_expression_tree() {
    // Before the lowering kept it, the condition was text on the element and no rule could ask
    // what it was.
    let src = format!("{PRELUDE}\tview def V {{ filter mass > 0; }}\n}}\n");
    let loaded = sysml_syntax::load_project(&[src.as_str()], true).expect("model parses");
    fn find<'a>(el: &'a sysml_model::Element) -> Option<&'a sysml_model::Element> {
        if el.kind == sysml_model::Kind::Filter {
            return Some(el);
        }
        el.children.iter().find_map(find)
    }
    let filter = loaded
        .roots
        .iter()
        .find_map(find)
        .expect("the filter element");
    assert!(
        filter.value_expr.is_some(),
        "the condition must be an expression, not only text: {:?}",
        filter.constraint
    );
    assert!(filter.children.is_empty(), "and still not a member");
}
