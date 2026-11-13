//! A `FlowDefinition`'s library base is `MessageAction`, not the usage base `Flow`.
//!
//! `core/stdlib/Systems Library/Flows.sysml:22-27` says `MessageAction` is "the base type of all
//! FlowDefinitions", and `:38-47` says `Message`, below it, is "the base type of all FlowUsages";
//! `Flow` (`:77`) is a more specific usage base. Mapping `Kind::Flow` to `Flows::Flow` therefore
//! gave every flow definition a usage base as a supertype and hid the ends it inherits from
//! `Links::BinaryLink`, which is the shape `SEM096` has to reject
//! (`FlowUsage_End_Invalid.sysml.xt:33`).

use sysml_resolve::Index;

const MODEL: &str = r#"
package P {
	abstract flow def F :> Links::BinaryLink;
	part def A;
	part a : A;
	part b : A;
	flow f : F from a to b;
}
"#;

fn built() -> Index {
    let loaded = sysml_syntax::load_project(&[MODEL], true).expect("model parses");
    Index::build(&loaded.roots).with_library(&loaded.library)
}

/// The id of the element whose qualified name is exactly `q`.
fn id(index: &Index, q: &str) -> usize {
    index
        .qualified
        .iter()
        .position(|one| one == q)
        .unwrap_or_else(|| {
            let near: Vec<&String> = index
                .qualified
                .iter()
                .filter(|one| one.contains(q.rsplit("::").next().unwrap_or(q)))
                .take(8)
                .collect();
            panic!("no element qualified `{q}`; near: {near:?}")
        })
}

fn names(index: &Index, ids: &[usize]) -> Vec<String> {
    ids.iter()
        .map(|&one| index.qualified[one].clone())
        .collect()
}

/// The direct supertypes of `element` plus the ids of the library types the assertions name.
fn bases(index: &Index) -> (usize, usize, usize, usize, usize) {
    (
        id(index, "P::F"),
        id(index, "P::f"),
        id(index, "Links::BinaryLink"),
        id(index, "Flows::MessageAction"),
        id(index, "Flows::Flow"),
    )
}

#[test]
fn a_flow_definition_is_a_definition_whose_base_is_message_action() {
    let index = built();
    let (f, _, binary_link, message_action, flow) = bases(&index);

    assert!(
        index.is_kind_of(f, "FlowDefinition"),
        "P::F is a FlowDefinition"
    );
    assert!(index.is_kind_of(f, "Definition"), "P::F is a Definition");
    assert!(!index.is_kind_of(f, "Usage"), "P::F is not a Usage");
    assert!(!index.is_kind_of(f, "FlowUsage"), "P::F is not a FlowUsage");

    let direct = index.supertypes_of_public(f);
    for wanted in [binary_link, message_action] {
        assert!(
            direct.contains(&wanted),
            "`P::F` must directly specialize `{}`, got {:?}",
            index.qualified[wanted],
            names(&index, &direct)
        );
    }
    assert!(
        !direct.contains(&flow),
        "`P::F` must not specialize the usage base `Flows::Flow`, got {:?}",
        names(&index, &direct)
    );
    assert_eq!(
        direct,
        vec![binary_link, message_action],
        "the written base and the implicit base are the whole list"
    );
}

#[test]
fn message_inherits_message_action_and_never_flow() {
    let index = built();
    let message = id(&index, "Flows::Message");
    let message_action = id(&index, "Flows::MessageAction");
    let flow = id(&index, "Flows::Flow");

    assert!(index.is_kind_of(message, "FlowDefinition"));
    assert!(
        index
            .supertypes_of_public(message)
            .contains(&message_action),
        "`Flows::Message` must directly inherit `Flows::MessageAction`, got {:?}",
        names(&index, &index.supertypes_of_public(message))
    );
    assert!(
        !index.is_subtype(message, flow),
        "`Flows::Message` is above `Flows::Flow`, not below it"
    );
    // And the reverse edge the old mapping introduced is gone: a flow definition no longer
    // reaches the usage base, directly or transitively.
    let f = id(&index, "P::F");
    assert!(
        !index.is_subtype(f, flow),
        "`P::F` must not reach `Flows::Flow`"
    );
}

#[test]
fn a_flow_definitions_effective_ends_are_the_binary_link_ends() {
    let index = built();
    let (f, _, binary_link, _, _) = bases(&index);

    let ends = index.effective_ends_of(f);
    let link_ends = index.effective_ends_of(binary_link);
    assert_eq!(
        ends,
        link_ends,
        "`P::F` inherits the ends of `Links::BinaryLink`, got {:?}",
        names(&index, &ends)
    );
    let source = id(&index, "Links::BinaryLink::source");
    let target = id(&index, "Links::BinaryLink::target");
    assert_eq!(
        ends,
        vec![source, target],
        "the exact end identities survive"
    );
    for &end in &ends {
        assert!(
            !index.is_kind_of(end, "Usage"),
            "`{}` is a KerML end feature, not a Usage",
            index.qualified[end]
        );
    }
}

#[test]
fn a_normal_flow_usage_has_message_as_its_direct_implicit_base() {
    let index = built();
    let (f, usage, _, message_action, flow) = bases(&index);
    let message = id(&index, "Flows::Message");

    assert!(index.is_kind_of(usage, "FlowUsage"));
    assert!(index.is_kind_of(usage, "Usage"));
    assert!(!index.is_kind_of(usage, "Definition"));

    let direct = index.supertypes_of_public(usage);
    assert!(
        direct.contains(&message),
        "`P::f` must directly specialize `Flows::Message`, got {:?}",
        names(&index, &direct)
    );
    assert!(direct.contains(&f), "`P::f : F` keeps its written type");
    assert!(
        !direct.contains(&flow) && !direct.contains(&message_action),
        "the implicit base of a usage is `Message` itself, got {:?}",
        names(&index, &direct)
    );
}

#[test]
fn the_supertype_cache_repeats_the_same_answers() {
    let index = built();
    let (f, usage, binary_link, message_action, flow) = bases(&index);
    let message = id(&index, "Flows::Message");
    let ends = index.effective_ends_of(f);

    index.enable_supertype_cache();

    assert!(index.is_kind_of(f, "FlowDefinition"));
    assert!(!index.is_kind_of(f, "Usage"));
    assert_eq!(
        index.supertypes_of_public(f),
        vec![binary_link, message_action]
    );
    assert!(!index.supertypes_of_public(f).contains(&flow));
    assert!(index
        .supertypes_of_public(message)
        .contains(&message_action));
    assert!(!index.supertypes_of_public(message).contains(&flow));
    assert_eq!(index.supertypes_of_public(usage).contains(&message), true);
    assert_eq!(index.effective_ends_of(f), ends);
    assert_eq!(
        index.effective_ends_of(f),
        vec![
            id(&index, "Links::BinaryLink::source"),
            id(&index, "Links::BinaryLink::target")
        ]
    );
}
