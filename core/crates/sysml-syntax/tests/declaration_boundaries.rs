//! Every declaration ends at `;` or at its body, and nothing it leaves unread becomes a member.
//!
//! The parser used to return quietly from a declaration whose next token was neither, and the
//! member loop then read the leftover text as new siblings. Corpus and library both counted as
//! parsed with zero errors while 416 such splits were in them — `connect [1] a to [1] b` with no
//! ends, `= f meta SysML::Usage` as two phantom members, `then action b { … }` as an attribute
//! beside a reference to `action`. Each case below is one shape from the standard library or the
//! validation corpus, with the source line it came from.

use sysml_model::{Element, Kind};
use sysml_syntax::load_project;

fn members(src: &str) -> Vec<Element> {
    let parsed = sysml_parser::parse(src).expect(src);
    assert!(
        parsed.document.errors().is_empty(),
        "{src}: {:?}",
        parsed.document.errors()
    );
    let loaded = load_project(&[src], false).expect(src);
    loaded.roots.into_iter().next().expect("a package").children
}

fn names(elements: &[Element]) -> Vec<&str> {
    elements.iter().map(|el| el.name.as_str()).collect()
}

#[test]
fn unread_text_after_a_declaration_is_an_error_not_a_sibling() {
    let parsed = sysml_parser::parse("package P { attribute a = 1 2; item following; }").unwrap();
    let errors = parsed.document.errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].1.contains("to end the declaration"), "{errors:?}");
}

#[test]
fn connector_ends_keep_their_bounds_and_references() {
    // `ShapeItems.sysml:417`, `Occurrences.kerml:163-164`, `TransitionPerformances.kerml:35`.
    let found = members(
        "package P { connection :MatesWith connect [1] be to [2] ae; \
         connector :HappensDuring from [1] a references x to [0..1] b references y; \
         connector [0..1] t to [1..*] g; }",
    );
    assert_eq!(found.len(), 3, "{:?}", names(&found));
    assert_eq!(found[0].from.as_deref(), Some("[1] be"));
    assert_eq!(found[0].to.as_deref(), Some("[2] ae"));
    // `a references x` keeps the end's own name, written in the `::>` spelling.
    assert_eq!(found[1].from.as_deref(), Some("[1] a ::> x"));
    assert_eq!(found[1].to.as_deref(), Some("[0..1] b ::> y"));
    let ends: Vec<&str> = found[1]
        .children
        .iter()
        .filter(|c| c.is_end)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(
        ends,
        ["a", "b"],
        "the projected ends carry the declared names"
    );
    assert_eq!(found[2].from.as_deref(), Some("[0..1] t"));
    assert_eq!(found[2].to.as_deref(), Some("[1..*] g"));
    let parsed = sysml_parser::parse("package P { interface connect a.p to b.q; }").unwrap();
    assert!(parsed.document.errors().is_empty());
}

#[test]
fn metaclassification_and_null_coalescing_are_expressions() {
    // `CauseAndEffect.sysml:70`, `NumericalFunctions.kerml:36`.
    let found = members(
        "package P { metadata def M { ref :>> baseType = causes meta SysML::Usage; } \
         attribute total = values->reduce '+' ?? zero; attribute test = x @@ SysML::Usage; }",
    );
    assert_eq!(names(&found), ["M", "total", "test"]);
    assert_eq!(names(&found[0].children), ["baseType"]);
    assert_eq!(
        found[0].children[0].value_text.as_deref(),
        Some("causes meta SysML::Usage")
    );
    // The stored name is unquoted; rendering quotes it again, so the text parses back.
    assert_eq!(
        found[1].value_text.as_deref(),
        Some("values->reduce('+') ?? zero")
    );
    assert_eq!(found[2].value_text.as_deref(), Some("x @@ SysML::Usage"));
}

#[test]
fn then_declares_the_action_that_follows() {
    // `3c-Function-based Behavior-structure mod-1.sysml:47-48`, `Actions.sysml:514-517`.
    let found = members(
        "package P { action def A { action a; then action b { action c; } \
         then private action d assign x := 1; then action accept sig : S; \
         private action init assign i := 1; then private action loop while i <= 3 { assign i := i + 1; } } }",
    );
    let body = &found[0].children;
    let actions: Vec<&Element> = body
        .iter()
        .filter(|el| el.kind == Kind::ActionUsage)
        .collect();
    assert_eq!(
        actions
            .iter()
            .map(|el| el.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "d", "<anonymous>", "init", "loop"]
    );
    assert_eq!(
        names(&actions[1].children),
        ["c"],
        "the succession is not inside `b`"
    );
    assert_eq!(actions[2].children[0].kind, Kind::AssignmentActionUsage);
    assert_eq!(
        successions(body),
        [
            (Some("a"), Some("b")),
            (Some("b"), Some("d")),
            // The target of `then action accept …` has no name to refer to it by.
            (Some("d"), None),
            (Some("init"), Some("loop")),
        ]
    );
}

/// `(from, to)` of every succession in a body, in order.
fn successions(body: &[Element]) -> Vec<(Option<&str>, Option<&str>)> {
    body.iter()
        .filter(|el| el.kind == Kind::Transition)
        .map(|el| (el.from.as_deref(), el.to.as_deref()))
        .collect()
}

#[test]
fn a_succession_without_a_source_starts_at_the_feature_before_it() {
    // `SourceSuccession` and `EmptySuccession` (`SysML.xtext:874-881`, `:1703-1706`) leave their
    // source unwritten; the reference implementation takes the previous feature of the body,
    // passing over what is not one (`UsageUtil.getPreviousFeature`). `first start;` counts as
    // `start`, a definition and a parameter do not count, and a feature with no name gives no
    // source rather than an earlier one.
    let found = members(
        "package P { action def A { in x; first start; then a; action a; action def Inner; \
         then action b; accept sig; then c; action c; } }",
    );
    assert_eq!(
        successions(&found[0].children),
        [
            (Some("start"), Some("a")),
            (Some("a"), Some("b")),
            (None, Some("c"))
        ]
    );
}

#[test]
fn an_action_node_keeps_its_declaration() {
    // `3a-Function-based Behavior-1.sysml:102`.
    let found = members("package P { action engineStarted accept engineStart : EngineStart; }");
    assert_eq!(names(&found), ["engineStarted"]);
    assert_eq!(found[0].kind, Kind::ActionUsage);
    assert_eq!(
        found[0].children.len(),
        1,
        "the accept node is the action's content"
    );
}

#[test]
fn a_directed_kind_keyword_is_the_kind_not_the_name() {
    // `Actions.sysml:399`.
    let found =
        members("package P { action def IfThen { in ifTest[1]; in action thenClause[0..1]; } }");
    let body = &found[0].children;
    assert_eq!(names(body), ["ifTest", "thenClause"]);
    assert_eq!(body[1].kind, Kind::ActionUsage);
    assert!(body[1].direction.is_some());
}

#[test]
fn kernel_relationship_statements_declare_nothing_extra() {
    // `Occurrences.kerml:125`, `:729`, `12a-Dependency.sysml:8`, `FeatureInvertingTest_valid.kerml.xt:34-35`.
    let found = members(
        "package P { class A; class B; disjoint A.x from B.y; subclassifier A specializes B; \
         dependency from A to B; inverse B::g of A::f; inverting I inverse B::g.f of A::h; }",
    );
    assert_eq!(
        found.iter().map(|el| el.kind.clone()).collect::<Vec<_>>(),
        [
            Kind::ClassDef,
            Kind::ClassDef,
            Kind::Disjoining,
            Kind::Specialization,
            Kind::Dependency,
            Kind::Inverting,
            Kind::Inverting,
        ]
    );
    assert_eq!(found[2].from.as_deref(), Some("A.x"));
    assert_eq!(found[2].to.as_deref(), Some("B.y"));
    assert_eq!(found[6].from.as_deref(), Some("B::g.f"));
    assert_eq!(found[6].to.as_deref(), Some("A::h"));
}

#[test]
fn a_cross_feature_may_specialize_before_the_end() {
    // `Occurrences.kerml:767`.
    let found = members(
        "package P { assoc HW { end happensWhile [1..*] subsets coincident feature that : Occ redefines longer; } }",
    );
    let end = &found[0].children[0];
    assert_eq!(end.name, "that");
    assert!(end.is_end);
    let cross = end
        .children
        .iter()
        .find(|c| c.name == "happensWhile")
        .expect("cross feature");
    assert_eq!(cross.subsets, ["coincident"]);
    assert_eq!(cross.multiplicity.as_deref(), Some("[1..*]"));
}

#[test]
fn extension_keywords_and_extended_declarations() {
    // `SemanticMetadata_valid.sysml.xt:72-90`, `14c-Language Extensions.sysml:145`.
    let found = members(
        "package P { metadata def B; #B def C1 { } #B f1 { } \
         requirement r { actor #B a; } #B requirement req1 { } }",
    );
    assert_eq!(names(&found), ["B", "C1", "f1", "r", "req1"]);
    assert_eq!(found[1].kind, Kind::KermlType);
    assert!(found[1].is_definition);
    assert_eq!(found[2].kind, Kind::KermlFeature);
    let actor = &found[3].children[0];
    assert_eq!(actor.name, "a");
    assert!(actor.children.iter().any(|c| c.kind == Kind::MetadataUsage));
}

#[test]
fn transition_payloads_and_satisfy_requirement_are_read_whole() {
    // `Actions.sysml:222`, `Views.sysml:37`.
    let found = members(
        "package P { state def S { transition t first start accept p : Anything via receiver then done; } \
         requirement def R { satisfy requirement vc by that { } } }",
    );
    assert_eq!(names(&found[0].children), ["t"]);
    let t = &found[0].children[0];
    assert_eq!(
        t.trigger_type.as_deref(),
        Some("Anything"),
        "the payload's type is kept"
    );
    assert_eq!(
        t.addressed.as_deref(),
        Some("receiver"),
        "and the port it arrives through"
    );
    assert_eq!(found[1].children.len(), 1);
}

#[test]
fn a_reserved_word_may_be_a_call_argument_and_a_perform_may_have_a_value() {
    // `StatePerformances.kerml:41`, `7a1-Variant Configuration - General Concept-a.sysml:66`.
    let found = members(
        "package P { step nonDoMiddle[*] subsets middle = middle->excluding(do); \
         part p { perform action :>> doXorY = doX; } }",
    );
    assert_eq!(names(&found), ["nonDoMiddle", "p"]);
    assert_eq!(found[1].children.len(), 1);
    assert_eq!(found[1].children[0].value_text.as_deref(), Some("doX"));
}

#[test]
fn a_multi_line_note_is_not_model_content() {
    // `4a-Functional Allocation.sysml:24-34`: `//* … */` comments out two declarations.
    let found = members("package P { //*\n part ghost;\n */ part real; }");
    assert_eq!(names(&found), ["real"]);
}
