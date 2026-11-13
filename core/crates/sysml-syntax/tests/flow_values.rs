use sysml_model::{Element, Kind, Visibility};
use sysml_syntax::load_project;

fn flow(src: &str) -> Element {
    let loaded = load_project(&[src], false).expect(src);
    assert_eq!(loaded.roots.len(), 1, "{src}");
    let flow = loaded.roots.into_iter().next().unwrap();
    assert_eq!(flow.kind, Kind::FlowUsage, "{src}");
    flow
}

#[test]
fn flow_and_payload_values_are_separate_expressions() {
    let f = flow("flow f = source of p : Fuel = amount from a to b;");
    assert_eq!(f.value_text.as_deref(), Some("source"));
    assert_eq!(f.children.len(), 1);
    let p = &f.children[0];
    assert_eq!(p.kind, Kind::PayloadFeature);
    assert_eq!(p.value_text.as_deref(), Some("amount"));
    assert!(f.value_expr.is_some() && p.value_expr.is_some());
    assert_eq!(f.from.as_deref(), Some("a"));
    assert_eq!(f.to.as_deref(), Some("b"));
}

#[test]
fn default_and_initial_markers_survive_on_both_values() {
    for (marker, initial, default) in [
        ("=", false, false),
        (":=", true, false),
        ("default", false, true),
        ("default =", false, true),
        ("default :=", true, true),
    ] {
        let src = format!("flow <f> f {marker} source of <p> p : Fuel {marker} amount;");
        let f = flow(&src);
        assert_eq!((f.is_initial, f.is_default), (initial, default), "{src}");
        assert_eq!(
            (f.children[0].is_initial, f.children[0].is_default),
            (initial, default),
            "{src}"
        );
        assert_eq!(f.short_name.as_deref(), Some("f"));
        assert_eq!(f.children[0].short_name.as_deref(), Some("p"));
    }
}

#[test]
fn anonymous_flows_and_payloads_can_have_only_values() {
    let f = flow("flow = source of = amount;");
    assert!(f.name_span.is_none());
    assert_eq!(f.value_text.as_deref(), Some("source"));
    assert_eq!(f.children.len(), 1);
    assert!(f.children[0].name_span.is_none());
    assert_eq!(f.children[0].value_text.as_deref(), Some("amount"));
}

#[test]
fn braced_values_are_not_confused_with_the_flow_body() {
    let f = flow("message m default { source } of p : Fuel default { amount } { item carried; }");
    assert!(f.value_expr.is_some());
    assert!(f.children[0].value_expr.is_some());
    assert_eq!(f.children.len(), 2);
    assert_eq!(f.children[1].name, "carried");
}

#[test]
fn payloads_keep_all_specializations_types_and_bounds() {
    let f = flow(
        "flow f of <p> p : Fuel, Gas :> base[0..*] :>> prior ::> referent => crossed = amount;",
    );
    let p = &f.children[0];
    assert_eq!(p.type_refs, ["Fuel", "Gas"]);
    assert_eq!(p.subsets, ["base"]);
    assert_eq!(p.redefines, ["prior"]);
    assert_eq!(p.references, ["referent"]);
    assert_eq!(p.crosses, ["crossed"]);
    assert_eq!(p.multiplicity.as_deref(), Some("[0..*]"));
    assert_eq!(p.value_text.as_deref(), Some("amount"));
    for name in ["Fuel", "Gas", "base", "prior", "referent", "crossed"] {
        assert!(
            p.ref_spans.iter().any(|(reference, _)| reference == name),
            "missing {name}"
        );
    }
}

#[test]
fn payload_specializations_accept_keyword_forms_and_multiplicity_modifiers() {
    let f = flow("flow f of p[2] ordered nonunique defined by Fuel subsets base redefines prior references referent crosses crossed default amount;");
    let p = &f.children[0];
    assert_eq!(p.type_refs, ["Fuel"]);
    assert_eq!(p.subsets, ["base"]);
    assert_eq!(p.redefines, ["prior"]);
    assert_eq!(p.references, ["referent"]);
    assert_eq!(p.crosses, ["crossed"]);
    assert!(p.modifiers.is_ordered && p.modifiers.is_nonunique);
    assert_eq!(p.multiplicity.as_deref(), Some("[2]"));
    assert!(p.is_default);
}

#[test]
fn flow_heads_keep_prefixes_and_specializations_before_the_value() {
    let f = flow("private ref flow <f> f : Transfer subsets base = source of : Fuel;");
    assert_eq!(f.visibility, Some(Visibility::Private));
    assert!(f.modifiers.is_reference);
    assert_eq!(f.subsets, ["base"]);
    assert_eq!(f.type_refs, ["Transfer"]);
    assert_eq!(f.value_text.as_deref(), Some("source"));
    assert_eq!(f.children[0].type_refs, ["Fuel"]);
}

#[test]
fn value_and_specialization_spans_are_exact_source_ranges() {
    let src = "flow f = source + 1 of p : Fuel :> base = amount * 2;";
    let f = flow(src);
    let p = &f.children[0];
    let range = f.value_span.expect("flow value span");
    assert_eq!(&src[range.start..range.end], "= source + 1");
    let range = p.value_span.expect("payload value span");
    assert_eq!(&src[range.start..range.end], "= amount * 2");
    for (reference, range) in &p.ref_spans {
        assert_eq!(&src[range.start..range.end], reference);
    }
}

#[test]
fn keyword_named_paths_are_real_default_values_in_both_head_parsers() {
    let f = flow("flow f default frame.localClock of : Fuel = frame.amount;");
    assert_eq!(f.value_text.as_deref(), Some("frame.localClock"));
    assert_eq!(f.children[0].value_text.as_deref(), Some("frame.amount"));
    let loaded = load_project(
        &[
            "function PositionOf { in clock : Clock[1] default frame.localClock; \
           in vector : Vector[3] ordered; }",
        ],
        false,
    )
    .expect("the standard-library declaration shape parses");
    let members = &loaded.roots[0].children;
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].value_text.as_deref(), Some("frame.localClock"));
    assert!(members[0].is_default);
    assert!(members[1].modifiers.is_ordered);
}

#[test]
fn all_is_a_unary_value_expression() {
    // `research/upstream/.../10b-Trade-off Among Alternative Configurations.sysml:76`.
    let loaded = load_project(
        &["analysis engineTradeStudy { subject : Engine[1..*] = all engineChoice; }"],
        false,
    )
    .expect("the official subject value parses");
    let subject = &loaded.roots[0].children[0];
    assert_eq!(subject.value_text.as_deref(), Some("all engineChoice"));
    let flow = flow("flow f = all engineChoice of p = all payloadChoice;");
    assert_eq!(flow.value_text.as_deref(), Some("all engineChoice"));
    assert_eq!(
        flow.children[0].value_text.as_deref(),
        Some("all payloadChoice")
    );
}
