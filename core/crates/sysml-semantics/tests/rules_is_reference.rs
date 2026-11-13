//! `Element::is_reference` versus `Modifiers::is_reference` — one flag, two different questions.
//!
//! The split exists because conflating them produced a wrong diagnostic. Before it, the only way to
//! ask "is this element referential" was `modifiers.is_reference`, which records the authored `ref`
//! word, and three metaclasses make the property `true` *by definition* regardless of that word:
//! `AttributeUsage` (`generated.rs:2033`), `EventOccurrenceUsage` (`:2212`) and `ReferenceUsage`
//! (`:2492`, whose kind this kernel does not have).
//!
//! Measured, the split is load-bearing in one direction: `event occurrence e : Real;` must report
//! that an *occurrence* is typed by a data type, not that a *reference* is. Reading the effective
//! field in `typed_by_definition` produced the second message instead, and its own test caught it.
//!
//! These tests pin both directions: the element field answers the metaclass question, the modifier
//! answers the authored-word question, and neither is allowed to answer the other's.

use sysml_semantics::check;

fn lowered(source: &str) -> sysml_syntax::Loaded {
    sysml_syntax::load_project(&[source], true).expect("model parses")
}

fn find<'a>(roots: &'a [sysml_model::Element], name: &str) -> &'a sysml_model::Element {
    fn go<'a>(
        elements: &'a [sysml_model::Element],
        name: &str,
    ) -> Option<&'a sysml_model::Element> {
        for element in elements {
            if element.name == name {
                return Some(element);
            }
            if let Some(found) = go(&element.children, name) {
                return Some(found);
            }
        }
        None
    }
    go(roots, name).unwrap_or_else(|| panic!("no element named `{name}`"))
}

/// The first element of a kind, for the cases whose declared name does not survive lowering.
fn find_kind(roots: &[sysml_model::Element], kind: sysml_model::Kind) -> &sysml_model::Element {
    fn go<'a>(
        elements: &'a [sysml_model::Element],
        kind: &sysml_model::Kind,
    ) -> Option<&'a sysml_model::Element> {
        for element in elements {
            if &element.kind == kind {
                return Some(element);
            }
            if let Some(found) = go(&element.children, kind) {
                return Some(found);
            }
        }
        None
    }
    go(roots, &kind).unwrap_or_else(|| panic!("no element of kind {kind:?}"))
}

#[test]
fn an_event_usage_is_referential_without_writing_ref() {
    // `event a;` writes no `ref`, so the modifier stays false — the authored word is not there.
    // The element field is true because the metaclass says an EventOccurrenceUsage is a reference.
    //
    // The element is `<anonymous>` by design, not by defect: `EventOccurrenceUsage` is
    // `'event' ( OwnedReferenceSubsetting FeatureSpecializationPart? | ... )`, so the first
    // alternative declares nothing and `a` is a feature the event *denotes*
    // (`sysml-parser/src/ast.rs:886-895`). So the lookup goes through the kind.
    let loaded = lowered(
        r#"
package P {
	part def Base;
	event a : Base;
}
"#,
    );
    let event = find_kind(&loaded.roots, sysml_model::Kind::EventOccurrenceUsage);
    assert!(
        !event.modifiers.is_reference,
        "`event a;` writes no `ref`, so the authored word must stay false"
    );
    assert!(
        event.is_reference,
        "an EventOccurrenceUsage is `isReference` by definition (generated.rs:2212)"
    );
}
#[test]
fn a_ref_usage_is_referential_through_the_authored_word() {
    // `ref` reaches both: the modifier records the word, and the element field reflects it.
    let loaded = lowered(
        r#"
package P {
	part def Base;
	ref r : Base;
}
"#,
    );
    let reference = find(&loaded.roots, "r");
    assert!(
        reference.modifiers.is_reference,
        "`ref` must survive lowering onto the modifier"
    );
    assert!(
        reference.is_reference,
        "the effective field must agree with an authored `ref`"
    );
}

#[test]
fn a_plain_part_usage_is_not_referential() {
    // Neither source applies: no `ref` word, and `PartUsage` does not redefine the property.
    let loaded = lowered(
        r#"
package P {
	part def Base;
	part p : Base;
}
"#,
    );
    let part = find(&loaded.roots, "p");
    assert!(!part.modifiers.is_reference);
    assert!(
        !part.is_reference,
        "a composite part usage is not a reference by any reading"
    );
}

#[test]
fn an_attribute_usage_is_referential_by_definition() {
    // The third metaclass with the constant: `AttributeUsage` (generated.rs:2033). It is also the one
    // that could not be tried as a live check, because the projection already treats an attribute as
    // non-composite — so the field is the thing worth pinning.
    let loaded = lowered(
        r#"
package P {
	attribute def Base;
	attribute a : Base;
}
"#,
    );
    let attribute = find(&loaded.roots, "a");
    assert!(!attribute.modifiers.is_reference);
    assert!(
        attribute.is_reference,
        "an AttributeUsage is `isReference` by definition (generated.rs:2033)"
    );
}

#[test]
fn an_event_occurrence_typed_by_a_data_type_is_still_reported_as_an_occurrence() {
    // The regression the split fixes. With `effective_reference` read in `typed_by_definition`, the
    // message became "a reference typed by a data type" and the assertion below failed on its own
    // wording. The rule is about what the author wrote, so it must read the modifier.
    let loaded = lowered(
        r#"
package P {
	attribute def Real;
	event occurrence e : Real;
}
"#,
    );
    let issues = check(&loaded.roots, &loaded.library).issues;
    let messages: Vec<&str> = issues
        .iter()
        .map(|issue| issue.message.as_str())
        .filter(|message| message.contains("event occurrence e") || message.contains("::e"))
        .collect();
    assert!(
        messages.iter().any(|m| m.contains("occurrence")),
        "an event occurrence typed by a data type is an occurrence mistake, got {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("a reference typed by")),
        "it must not be reported as a reference, got {messages:?}"
    );
}
