//! Every name a declaration head states is a reference, and an unresolvable one is reported.
//!
//! `check` verified the `:` type, both sides of `satisfy`, and predicate names. `:>`, `subsets`,
//! `chains`, `conjugates`, `references` and `crosses` were never checked: `part def A :> Nowhere;`
//! came back silent while `ref_spans` covered the name for renames.

use sysml_resolve::Resolution;

fn target_of(src: &str, name: &str, clause: &str) -> Resolution {
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = index
        .name
        .iter()
        .position(|n| n == name)
        .unwrap_or_else(|| panic!("{name} not found"));
    index.resolve(id, clause)
}

fn reported(name: &str, clause: &str) -> bool {
    let loaded = sysml_syntax::load_project(
        &[&format!(
            "package P {{\n\tpart def Target;\n\t{clause}\n}}\n"
        )],
        true,
    )
    .expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    index
        .references(&loaded.roots)
        .iter()
        .any(|r| r.text == name && matches!(r.target, Resolution::Unresolved))
}

#[test]
fn a_dangling_specialization_is_a_reference_that_fails() {
    let src = "package P {\n\tpart def A :> Nowhere;\n}\n";
    assert!(matches!(
        target_of(src, "A", "Nowhere"),
        Resolution::Unresolved
    ));
}

#[test]
fn a_dangling_subset_is_reported() {
    assert!(reported(
        "Missing",
        "part def H { part p subsets Missing; }"
    ));
}

#[test]
fn a_dangling_chain_is_reported() {
    // Each segment of a chain is its own `OwnedFeatureChaining` (`KerML.xtext:1046-1047`), so the
    // reference that fails is the segment that names nothing, not the whole dotted text. Both
    // halves are checked: the first segment when it is missing, the second when the first resolved
    // and the step into it found nothing.
    assert!(reported("Gone", "feature c chains Gone.x;"));
    assert!(reported("x", "feature a; feature c chains a.x;"));
}

#[test]
fn a_chain_between_untyped_features_resolves_to_both_names() {
    // `feature two chains a.b` with `a` and `b` declared beside it: the second segment is not a
    // member of `a`, so the step falls back to the enclosing scope and both names resolve. That
    // is a resolution question only — `validateFeatureChainingFeatureConformance` rejects the
    // chain, because `b` is not featured within `a`. This test used to say the pilot called the
    // model legal; the pilot says nothing about it (`FeatureChain_invalid.kerml.xt` asserts only
    // the one-segment case), and silence is not a statement.
    let src = "package P {\n\tfeature a;\n\tfeature b;\n\tfeature two chains a.b;\n}\n";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let refs = index.references(&loaded.roots);
    let unresolved: Vec<&str> = refs
        .iter()
        .filter(|r| matches!(r.target, Resolution::Unresolved))
        .map(|r| r.text.as_str())
        .collect();
    assert!(unresolved.is_empty(), "got {unresolved:?}");
}

#[test]
fn a_qualified_path_through_a_feature_is_marked() {
    // `f::a` names `a` inside what `f` is. `::` is for namespaces; a feature's members are reached
    // with `.`, which is the notation error the pilot states as "Must be an accessible feature
    // (use dot notation for nesting)".
    let src = "package P {\n\tpart def F { part a; }\n\tpart f : F;\n\tpart def A { part g = f::a; }\n}\n";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let g = index.name.iter().position(|n| n == "g").expect("g");
    assert!(index.crosses_a_feature(g, "f::a"), "`f` is a feature");
    assert!(!index.crosses_a_feature(g, "P::F"), "`P` is a package");
}

#[test]
fn a_qualified_path_that_names_a_declaration_is_not_marked() {
    // The two shapes the official models use legally. `K::f::m` names a multiplicity `f` declares
    // (`MultiplicityAlias_valid.kerml.xt:34`), and `a::b` names a variant `a` declares, reached
    // through the redefinition that says which `a` is meant (`7a-Variant Configuration`).
    let src = r#"
package P {
    class K { feature f : T { multiplicity m [1..2]; } }
    class T;
    class C { alias m for K::f::m; }
    part def Q { variation part a { variant part b; } }
    part def D :> Q { part :>> a = a::b; }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let by_qualified = |q: &str| {
        (0..index.len())
            .find(|&i| index.qualified[i] == q)
            .unwrap_or_else(|| panic!("{q}"))
    };
    assert!(
        !index.crosses_a_feature(by_qualified("P::C"), "K::f::m"),
        "a multiplicity `f` declares is reachable"
    );
    assert!(
        !index.crosses_a_feature(by_qualified("P::D::a"), "a::b"),
        "a variant `a` declares is reachable through the redefinition"
    );
}

#[test]
fn a_dangling_conjugation_is_reported() {
    assert!(reported("Lost", "port def D conjugates Lost;"));
}

#[test]
fn a_resolvable_clause_is_not_reported() {
    let loaded = sysml_syntax::load_project(
        &["package P {\n\taction target;\n\tref action pp :> target;\n}\n"],
        true,
    )
    .expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let refs = index.references(&loaded.roots);
    let dangling: Vec<&str> = refs
        .iter()
        .filter(|r| matches!(r.target, Resolution::Unresolved))
        .map(|r| r.text.as_str())
        .collect();
    assert!(
        dangling.is_empty(),
        "nothing dangles here, got {dangling:?}"
    );
}

#[test]
fn effective_ends_replace_inherited_ends_by_identity_without_recursion() {
    let loaded = sysml_syntax::load_project(
        &[r#"
package P {
    part def T;
    assoc Base {
        end left : T;
        end right : T;
    }
    assoc Derived :> Base {
        end replacement : T;
    }
    assoc Named :> Base {
        end :>> right : T;
    }
}
"#],
        true,
    )
    .expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let by_name = |name: &str| {
        index
            .name
            .iter()
            .position(|n| n == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    let base_left = by_name("left");
    let base_right = by_name("right");
    let replacement = by_name("replacement");
    let derived = by_name("Derived");
    let named = (0..index.len())
        .find(|&id| index.qualified[id] == "P::Named")
        .unwrap();
    let named_end = index.owned_ends_of(named)[0];
    assert_eq!(
        index.effective_ends_of(derived),
        vec![replacement, base_right]
    );
    assert_eq!(index.effective_ends_of(named), vec![named_end, base_left]);
    assert!(index
        .implicit_redefinition_targets(replacement)
        .contains(&base_left));
    assert!(index.implicit_redefinition_targets(named_end).is_empty());
}

#[test]
fn effective_ends_replace_explicit_reordered_and_diamond_identities() {
    let loaded = sysml_syntax::load_project(
        &[r#"
package P {
    part def T;
    assoc Base { end left : T; end right : T; }
    assoc Explicit :> Base { end last : T :>> right; }
    assoc ReversedExplicit :> Base { end last : T :>> right; end first : T :>> left; }
    assoc Qualified :> Base { end last : T :>> P::Base::right; }
    assoc Unrelated { end right : T; }
    assoc SameNameFeature :> Base { part right : T; end local : T :>> P::SameNameFeature::right; }
    assoc UnrelatedEnd :> Base { end local : T :>> P::Unrelated::right; }
    assoc Reordered :> Base { end right : T; end left : T; }
    assoc Left :> Base { end replacement : T :>> left; }
    assoc Right :> Base;
    assoc Diamond :> Left, Right;
    assoc Sibling :> Base { end other : T :>> right; }
    assoc Multibranch :> Left, Sibling;
    part external : T;
    assoc Referenced :> Base { end local : T :>> external; }
}
"#],
        true,
    )
    .expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = |q: &str| index.qualified.iter().position(|name| name == q).unwrap();
    let left = id("P::Base::left");
    let right = id("P::Base::right");
    let replacement = id("P::Left::replacement");
    let other = id("P::Sibling::other");
    assert_eq!(
        index.effective_ends_of(id("P::Explicit")),
        [id("P::Explicit::last"), left]
    );
    assert_eq!(
        index.effective_ends_of(id("P::ReversedExplicit")),
        [
            id("P::ReversedExplicit::last"),
            id("P::ReversedExplicit::first")
        ]
    );
    assert!(index
        .implicit_redefinition_targets(id("P::Explicit::last"))
        .is_empty());
    assert!(index
        .implicit_redefinition_targets(id("P::ReversedExplicit::last"))
        .is_empty());
    assert!(index
        .implicit_redefinition_targets(id("P::ReversedExplicit::first"))
        .is_empty());
    assert_eq!(
        index.effective_ends_of(id("P::Referenced")),
        [id("P::Referenced::local"), right]
    );
    assert_eq!(
        index.implicit_redefinition_targets(id("P::Referenced::local")),
        [left]
    );
    assert_eq!(
        index.effective_ends_of(id("P::Qualified")),
        [id("P::Qualified::last"), left]
    );
    assert!(index
        .implicit_redefinition_targets(id("P::Qualified::last"))
        .is_empty());
    assert_eq!(
        index.effective_ends_of(id("P::SameNameFeature")),
        [id("P::SameNameFeature::local"), right]
    );
    assert_eq!(
        index.implicit_redefinition_targets(id("P::SameNameFeature::local")),
        [left]
    );
    assert_eq!(
        index.effective_ends_of(id("P::UnrelatedEnd")),
        [id("P::UnrelatedEnd::local"), right]
    );
    assert_eq!(
        index.implicit_redefinition_targets(id("P::UnrelatedEnd::local")),
        [left]
    );
    assert_eq!(
        index.effective_ends_of(id("P::Reordered")),
        [id("P::Reordered::right"), id("P::Reordered::left")]
    );
    assert_eq!(
        index.implicit_redefinition_targets(id("P::Reordered::right")),
        [right]
    );
    for _ in 0..2 {
        assert_eq!(
            index.effective_ends_of(id("P::Diamond")),
            [replacement, right]
        );
        assert_eq!(
            index.effective_ends_of(id("P::Multibranch")),
            [replacement, other]
        );
    }
}

#[test]
fn implicit_relation_targets_follow_direct_types_and_interface_end_count() {
    let loaded = sysml_syntax::load_project(
        &[r#"
package P {
    datatype D;
    class C;
    struct S;
    part def A { part nested : A; }
    attribute fromData : D;
    attribute fromClass : C;
    attribute fromStructure : S;
    attribute fromFeature : A::nested;
    attribute untyped;
    struct Holder { composite attribute structural : S; composite attribute occurrence : C; }
    class ClassHolder { composite attribute occurrence : C; }
    interface def Definition { end a : P; end b : P; }
    interface inherited : Definition;
    interface twoEnds { end a : P; end b : P; }
    interface oneEnd { end a : P; }
    interface threeEnds { end a : P; end b : P; end c : P; }
}
"#],
        true,
    )
    .expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let by_qualified = |q: &str| {
        index
            .qualified
            .iter()
            .position(|name| name == q)
            .unwrap_or_else(|| panic!("{q}"))
    };
    let target = |id| {
        index
            .implicit_subsetting_targets(id)
            .into_iter()
            .map(|target| index.qualified[target].as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(target(by_qualified("P::fromData")), ["Base::dataValues"]);
    assert_eq!(
        target(by_qualified("P::fromClass")),
        ["Occurrences::occurrences"]
    );
    assert_eq!(
        target(by_qualified("P::fromStructure")),
        ["Objects::objects"]
    );
    assert_eq!(target(by_qualified("P::fromFeature")), ["Base::dataValues"]);
    assert_eq!(target(by_qualified("P::untyped")), ["Base::dataValues"]);
    assert_eq!(
        target(by_qualified("P::Holder::structural")),
        ["Objects::Object::subobjects"]
    );
    assert_eq!(
        target(by_qualified("P::Holder::occurrence")),
        ["Occurrences::Occurrence::suboccurrences"]
    );
    assert_eq!(
        target(by_qualified("P::ClassHolder::occurrence")),
        ["Occurrences::Occurrence::suboccurrences"]
    );
    assert_eq!(
        index.types_of(by_qualified("P::fromFeature")),
        ["A::nested"]
    );
    assert_eq!(
        index.resolve(by_qualified("P::fromFeature"), "A::nested"),
        Resolution::Found(by_qualified("P::A::nested"))
    );
    let definition = by_qualified("P::Definition");
    let inherited = by_qualified("P::inherited");
    assert!(index.is_definition[definition]);
    assert!(!index.is_definition[inherited]);
    assert!(target(definition).is_empty());
    assert!(index
        .supertypes_of_public(definition)
        .contains(&by_qualified("Interfaces::BinaryInterface")));
    assert!(index.owned_ends_of(inherited).is_empty());
    assert_eq!(index.effective_ends_of(inherited).len(), 2);
    assert_eq!(target(inherited), ["Interfaces::interfaces"]);
    assert_eq!(
        target(by_qualified("P::twoEnds")),
        ["Interfaces::binaryInterfaces"]
    );
    assert_eq!(
        target(by_qualified("P::oneEnd")),
        ["Interfaces::interfaces"]
    );
    assert_eq!(
        target(by_qualified("P::threeEnds")),
        ["Interfaces::interfaces"]
    );
    index.enable_supertype_cache();
    for _ in 0..2 {
        assert_eq!(target(by_qualified("P::fromFeature")), ["Base::dataValues"]);
        assert_eq!(
            target(by_qualified("P::fromStructure")),
            ["Objects::objects"]
        );
        assert_eq!(
            target(by_qualified("P::twoEnds")),
            ["Interfaces::binaryInterfaces"]
        );
    }
}

#[test]
fn shorthand_ends_replace_each_inherited_branch_by_identity() {
    let loaded = sysml_syntax::load_project(
        &[r#"
package P {
    port def Signal;
    interface def First {
        end source : Signal;
        end target : Signal;
        end extra : Signal;
    }
    interface def Second {
        end source : Signal;
        end target : Signal;
        end extra : Signal;
    }
    part def Assembly {
        port left : Signal;
        port right : Signal;
        interface inherited : First, Second;
        interface joined : First, Second connect left to right;
    }
}
"#],
        false,
    )
    .expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots);
    let id = |q: &str| {
        index
            .qualified
            .iter()
            .position(|name| name == q)
            .unwrap_or_else(|| panic!("{q}"))
    };
    let first = index.owned_ends_of(id("P::First"));
    let second = index.owned_ends_of(id("P::Second"));
    assert_eq!(first.len(), 3);
    assert_eq!(second.len(), 3);
    assert!(first.iter().all(|end| !second.contains(end)));
    let inherited = id("P::Assembly::inherited");
    let all: Vec<_> = first.iter().chain(&second).copied().collect();
    assert_eq!(index.effective_ends_of(inherited), all);

    let joined = id("P::Assembly::joined");
    let owned = index.owned_ends_of(joined);
    assert_eq!(owned.len(), 2);
    assert_ne!(owned[0], owned[1]);
    let assembly = loaded.roots[0]
        .children
        .iter()
        .find(|el| el.name == "Assembly")
        .unwrap();
    let usage = assembly.children.iter().find(|el| el.id == joined).unwrap();
    for (position, endpoint) in ["left", "right"].into_iter().enumerate() {
        let end = &usage.children[position];
        assert_eq!(end.id, owned[position]);
        assert_eq!(end.kind, sysml_model::Kind::PortUsage);
        assert!(end.is_end);
        assert_eq!(end.membership, sysml_model::Membership::EndFeature);
        assert!(!index.has_authored_name(end.id));
        assert_eq!(end.references, [endpoint]);
        assert!(end.type_ref.is_none());
        assert!(end.type_refs.is_empty());
        let endpoint_id = id(&format!("P::Assembly::{endpoint}"));
        assert_ne!(end.id, endpoint_id);
        assert!(!index.is_end_of(endpoint_id));
        assert_eq!(
            index.resolve(end.id, endpoint),
            Resolution::Found(endpoint_id)
        );
        assert_eq!(
            index.implicit_redefinition_targets(end.id),
            [first[position], second[position]]
        );
    }
    let effective = vec![owned[0], owned[1], first[2], second[2]];
    assert_eq!(index.effective_ends_of(joined), effective);
    index.enable_supertype_cache();
    for _ in 0..2 {
        assert_eq!(index.effective_ends_of(inherited), all);
        assert_eq!(index.effective_ends_of(joined), effective);
        assert_eq!(
            index.implicit_redefinition_targets(owned[0]),
            [first[0], second[0]]
        );
    }
}

#[test]
fn interface_usage_shorthand_projects_one_end_per_authored_endpoint() {
    // Two independently declared multi-end interfaces, then a usage that joins two parts with the
    // `connect … to …` shorthand. The projection gives each authored endpoint its own end child
    // carrying the endpoint text in `references` (`lower.rs::project_connector_ends`), so the
    // identity assertions here are what the SEM096 count stands on: two distinct ids in authored
    // order, each pointing at the part it names, with no authored `: T` typing on either.
    let loaded = sysml_syntax::load_project(
        &[r#"
package P {
    port def Fuel;
    port def Signal;

    interface def FuelPorts {
        end primary : Fuel;
        end secondary : Fuel;
    }
    interface def SignalPorts {
        end clock : Signal;
        end reset : Signal;
    }

    part def Assembly {
        part left;
        part right;
        interface joined : FuelPorts connect left to right;
    }
}
"#],
        false,
    )
    .expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots);
    let by_qualified = |q: &str| {
        index
            .qualified
            .iter()
            .position(|name| name == q)
            .unwrap_or_else(|| panic!("{q}"))
    };

    // The two definitions stay independent: neither contributes an end to the other, and each
    // keeps exactly the two ends it authored.
    let fuel_ends = index.effective_ends_of(by_qualified("P::FuelPorts"));
    let signal_ends = index.effective_ends_of(by_qualified("P::SignalPorts"));
    assert_eq!(fuel_ends.len(), 2, "got {fuel_ends:?}");
    assert_eq!(signal_ends.len(), 2, "got {signal_ends:?}");
    assert!(
        fuel_ends.iter().all(|end| !signal_ends.contains(end)),
        "the two definitions must not share end identities: {fuel_ends:?} vs {signal_ends:?}"
    );

    let joined = by_qualified("P::Assembly::joined");
    let owned = index.owned_ends_of(joined);
    assert_eq!(
        owned.len(),
        2,
        "one projected end per authored endpoint: {owned:?}"
    );
    assert_ne!(
        owned[0], owned[1],
        "the projected ends are distinct identities"
    );

    // Authored order: the `connect` clause names its source first and its target second, and each
    // end keeps the endpoint text it was projected from.
    assert_eq!(
        index.references_of(owned[0]),
        ["left"],
        "first end is the `from` endpoint"
    );
    assert_eq!(
        index.references_of(owned[1]),
        ["right"],
        "second end is the `to` endpoint"
    );

    // Each endpoint reference resolves to the part it names, not to the end itself.
    let left = by_qualified("P::Assembly::left");
    let right = by_qualified("P::Assembly::right");
    assert_eq!(index.resolve(owned[0], "left"), Resolution::Found(left));
    assert_eq!(index.resolve(owned[1], "right"), Resolution::Found(right));
    assert!(
        !index.is_end_of(left),
        "an endpoint reference is not itself an end"
    );
    assert!(
        !index.is_end_of(right),
        "an endpoint reference is not itself an end"
    );

    // No authored `FeatureTyping`: the projected end is typed by ReferenceSubsetting alone, so its
    // text stays empty and the direct type of whatever it references is not copied onto it.
    assert!(
        index.type_of(owned[0]).is_none(),
        "projected end must author no FeatureTyping"
    );
    assert!(
        index.type_of(owned[1]).is_none(),
        "projected end must author no FeatureTyping"
    );

    // Both projected ends positionally replace the ends inherited from `FuelPorts`, so the usage
    // exposes exactly two effective identities — three would be the double count SEM096 reports.
    assert_eq!(
        index.effective_ends_of(joined),
        owned,
        "the two local ends replace the two inherited ones"
    );
    assert!(
        index
            .effective_ends_of(joined)
            .iter()
            .all(|end| !fuel_ends.contains(end)),
        "no inherited FuelPorts end survives positional replacement"
    );
    assert_eq!(
        index.implicit_redefinition_targets(owned[0]),
        [fuel_ends[0]]
    );
    assert_eq!(
        index.implicit_redefinition_targets(owned[1]),
        [fuel_ends[1]]
    );

    // Both endpoint fields are authored as well — `left` and `right`. They are the reading the old
    // count added to the projected ends, so asserting both exist is what makes the "two, not four"
    // claim below meaningful.
    assert_eq!(
        index.end_text(joined, true),
        Some("left"),
        "the `from` endpoint is authored"
    );
    assert_eq!(
        index.end_text(joined, false),
        Some("right"),
        "the `to` endpoint is authored"
    );

    // The cached path answers the same identities, so a rule pass that enables the cache does not
    // see a different count from the one asserted above.
    index.enable_supertype_cache();
    for _ in 0..2 {
        assert_eq!(index.effective_ends_of(joined), owned);
        assert_eq!(
            index.implicit_redefinition_targets(owned[0]),
            [fuel_ends[0]]
        );
    }
}

#[test]
fn from_to_shorthand_ends_keep_identity_and_leave_unreplaced_inherited_ends() {
    // The `from ... to ...` spelling of the same shorthand (`parser.rs`, the `ConnectorPart`
    // alternative after the declaration head). It projects one end child per endpoint exactly as
    // `connect ... to ...` does, so the count SEM096 reads is the effective end set: a valid usage
    // of a two-end interface answers two, and a usage of a three-end interface answers three,
    // because position 2 was never written and its inherited end therefore survives.
    let loaded = sysml_syntax::load_project(
        &[r#"
package P {
    port def Hot;
    port def Cold;

    interface def TwoWay {
        end feed : Hot;
        end drain : Cold;
    }
    interface def ThreeWay {
        end feed : Hot;
        end drain : Cold;
        end spare : Hot;
    }

    part def Vehicle {
        part tank;
        part engine;
        interface paired : TwoWay from tank to engine;
        interface delivery : TwoWay, ThreeWay from tank to engine;
    }
}
"#],
        true,
    )
    .expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = |q: &str| {
        index
            .qualified
            .iter()
            .position(|name| name == q)
            .unwrap_or_else(|| panic!("{q}"))
    };

    // The two definitions are independent: neither contributes an end to the other, so the
    // third end below belongs to `ThreeWay` alone.
    let two_way = index.owned_ends_of(id("P::TwoWay"));
    let three_way = index.owned_ends_of(id("P::ThreeWay"));
    assert_eq!(two_way.len(), 2, "got {two_way:?}");
    assert_eq!(three_way.len(), 3, "got {three_way:?}");
    assert!(
        two_way.iter().all(|end| !three_way.contains(end)),
        "the definitions must not share end identities"
    );

    let vehicle = loaded.roots[0]
        .children
        .iter()
        .find(|el| el.name == "Vehicle")
        .expect("Vehicle");
    let endpoints = ["tank", "engine"];

    // Valid arity: two projected ends replace the two inherited ones positionally, so adding
    // `from`/`to` on top would read four and report a valid interface.
    let paired = id("P::Vehicle::paired");
    let paired_ends = index.owned_ends_of(paired);
    assert_eq!(
        paired_ends.len(),
        2,
        "one projected end per authored endpoint"
    );
    assert_eq!(index.effective_ends_of(paired), paired_ends);
    assert_ne!(
        paired_ends[0], paired_ends[1],
        "the two endpoints keep distinct identities"
    );

    // Malformed arity: the same replacement leaves `ThreeWay::spare` in place.
    let delivery = id("P::Vehicle::delivery");
    let owned = index.owned_ends_of(delivery);
    assert_eq!(
        owned.len(),
        2,
        "the shorthand itself projects two ends: {owned:?}"
    );
    assert_ne!(owned[0], owned[1]);
    assert_eq!(
        index.effective_ends_of(delivery),
        [owned[0], owned[1], three_way[2]],
        "only the replaced identities are removed"
    );
    assert!(
        !index.effective_ends_of(delivery).contains(&two_way[0]),
        "the inherited ends that were replaced must not survive"
    );

    // Authored order and identity of each projected child: `from` first, `to` second.
    let usage = vehicle
        .children
        .iter()
        .find(|el| el.id == delivery)
        .expect("the usage");
    for (position, endpoint) in endpoints.into_iter().enumerate() {
        let end = &usage.children[position];
        assert_eq!(end.id, owned[position], "authored order");
        assert_eq!(end.kind, sysml_model::Kind::PortUsage);
        assert!(end.is_end);
        assert_eq!(end.membership, sysml_model::Membership::EndFeature);
        assert!(
            !index.has_authored_name(end.id),
            "the projected end is anonymous"
        );
        assert_eq!(end.references, [endpoint]);
        // No authored `FeatureTyping`: the endpoint text is a reference, and the typing the
        // reference implies is derived rather than written onto the end.
        assert!(end.type_ref.is_none());
        assert!(end.type_refs.is_empty());
        assert!(index.types_of(end.id).is_empty());
        assert!(index.type_of(end.id).is_none());
        let endpoint_id = id(&format!("P::Vehicle::{endpoint}"));
        assert_ne!(end.id, endpoint_id, "the end is not the endpoint feature");
        assert_eq!(
            index.resolve(end.id, endpoint),
            Resolution::Found(endpoint_id)
        );
        assert!(!index.is_end_of(endpoint_id));
        let replaced = index.implicit_redefinition_targets(end.id);
        assert!(replaced.contains(&two_way[position]), "got {replaced:?}");
        assert!(replaced.contains(&three_way[position]), "got {replaced:?}");
    }

    // The cached path answers the same identities and the same count.
    index.enable_supertype_cache();
    for _ in 0..2 {
        assert_eq!(index.effective_ends_of(paired), paired_ends);
        assert_eq!(
            index.effective_ends_of(delivery),
            [owned[0], owned[1], three_way[2]]
        );
        assert!(index
            .implicit_redefinition_targets(owned[1])
            .contains(&two_way[1]));
    }
}

#[test]
fn interface_usage_shorthand_keeps_the_arity_and_end_kind_sem096_reports() {
    // The count in `property_type.rs` is the effective end set, so the diagnostics that hang off it
    // must still be reachable through a shorthand usage. Two shapes are covered: a third inherited
    // end that survives positional replacement (the `AtMost(2)` arity report), and an end written
    // as `part` inside an interface definition (the `Each("PortUsage")` type report). Both are the
    // inputs those checks read, asserted where the resolver can see them.
    let loaded = sysml_syntax::load_project(
        &[r#"
package P {
    port def Fuel;
    interface def ThreeEnds {
        end first : Fuel;
        end second : Fuel;
        end third : Fuel;
    }
    interface def BadEnds {
        end part notPort : Fuel;
    }
    part def Assembly {
        part left;
        part right;
        interface joined : ThreeEnds connect left to right;
    }
}
"#],
        false,
    )
    .expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots);
    let by_qualified = |q: &str| {
        index
            .qualified
            .iter()
            .position(|name| name == q)
            .unwrap_or_else(|| panic!("{q}"))
    };

    // Malformed arity: the shorthand replaces the first two inherited ends positionally and the
    // third one survives, so the usage holds three effective ends and the third is the one past
    // the limit. It is inherited, not written here — that is what decides which element the report
    // names, and it must not be a projected end.
    let three = by_qualified("P::ThreeEnds");
    let three_ends = index.effective_ends_of(three);
    assert_eq!(three_ends.len(), 3, "got {three_ends:?}");
    let joined = by_qualified("P::Assembly::joined");
    let owned = index.owned_ends_of(joined);
    assert_eq!(
        owned.len(),
        2,
        "the shorthand itself still projects two ends: {owned:?}"
    );
    let effective = index.effective_ends_of(joined);
    assert_eq!(
        effective.len(),
        3,
        "the third inherited end must survive: {effective:?}"
    );
    assert_eq!(
        effective[..2],
        owned[..],
        "local ends keep their positional slots"
    );
    assert_eq!(
        effective[2], three_ends[2],
        "the inherited third end is the one past the limit"
    );
    assert_ne!(
        index.parent[effective[2]],
        Some(joined),
        "the end past the limit is inherited, not projected by the shorthand"
    );

    // Malformed type: `end part notPort : Fuel` stays a PartUsage, so the `Each("PortUsage")`
    // check has a wrong-typed value to reject. The authored `: Fuel` typing is preserved as the
    // direct type identity of that end.
    let bad = index.owned_ends_of(by_qualified("P::BadEnds"));
    assert_eq!(bad.len(), 1, "got {bad:?}");
    assert_eq!(index.kind[bad[0]], sysml_model::Kind::PartUsage);
    assert!(index.is_end_of(bad[0]));
    assert_eq!(index.type_of(bad[0]), Some("Fuel"));
    assert_eq!(index.types_of(bad[0]), ["Fuel"]);
}

#[test]
fn association_structure_library_identity_does_not_add_attribute_defaults() {
    let loaded = sysml_syntax::load_project(
        &["package P { part def T; connection def AB { end : T; end : T; } attribute n : AB; }"],
        true,
    )
    .expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = |q: &str| index.qualified.iter().position(|name| name == q).unwrap();
    let link_object = id("Objects::LinkObject");
    assert_eq!(
        index.kind[link_object],
        sysml_model::Kind::AssocStructureDef
    );
    assert!(index.is_kind_of(link_object, "AssociationStructure"));
    assert!(index.is_kind_of(link_object, "Association"));
    assert!(index.is_kind_of(link_object, "Structure"));
    assert!(index.implicit_subsetting_targets(link_object).is_empty());
    let n = id("P::n");
    let data_values = id("Base::dataValues");
    let data_value = id("Base::DataValue");
    assert_eq!(
        index.implicit_subsetting_targets(n),
        [id("Objects::objects")]
    );
    for end in index.owned_ends_of(id("P::AB")) {
        assert_ne!(index.kind[end], sysml_model::Kind::PortUsage);
    }
    index.enable_supertype_cache();
    for _ in 0..2 {
        let _ = index.effective_ends_of(id("P::AB"));
        assert!(!index.is_subtype(n, data_values));
        assert!(!index.is_subtype(n, data_value));
        assert_eq!(
            index.implicit_subsetting_targets(n),
            [id("Objects::objects")]
        );
        assert_eq!(index.resolve(n, "AB"), Resolution::Found(id("P::AB")));
    }
}

#[test]
fn every_usage_kind_sees_its_library_base() {
    // Every definition and usage keyword gives its declaration an implicit supertype in the
    // standard library, and the specification writes no clause for it. The pilot's "Duplicate of
    // inherited member name 'self' from Action, Part" cannot be produced without those edges: the
    // warning names the two library types that each declare a `self`.
    //
    // One line per family, spelled the way the pilot spells them. `attribute` is deliberately not
    // here — see `an_attribute_does_not_inherit_a_data_value`.
    let cases = [
        (
            "part p { ref ref x :> performedActions; }",
            "performedActions",
        ),
        ("item i { ref ref x :> self; }", "self"),
        ("action a { ref ref x :> self; }", "self"),
        ("state s { ref ref x :> self; }", "self"),
        ("calc c { ref ref x :> self; }", "self"),
        ("occurrence o { ref ref x :> self; }", "self"),
        ("case k { ref ref x :> self; }", "self"),
        ("use case u { ref ref x :> self; }", "self"),
        ("requirement r { ref ref x :> self; }", "self"),
        ("constraint c { ref ref x :> self; }", "self"),
    ];
    for (body, member) in cases {
        let src = format!("package P {{\n\t{body}\n}}\n");
        let loaded = sysml_syntax::load_project(&[src.as_str()], true).expect("parses");
        let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
        let id = index.name.iter().position(|n| n == "x").expect("x");
        // `Ambiguous` counts: `Occurrences::Occurrence` declares `self` several times over (one
        // per specialization block), so the name resolves to more than one element. That is the
        // whole point of the edge — the name is reachable — and the duplicate warning reports it.
        assert!(
            !matches!(index.resolve(id, member), Resolution::Unresolved),
            "`{body}` should inherit `{member}` from its library base, got {:?}",
            index.resolve(id, member)
        );
    }
}

#[test]
fn an_attribute_definition_specializes_data_value() {
    // `Attributes::AttributeValue` is an alias for `Base::DataValue`, so an attribute definition
    // carries a `self` from it. That is why `part p2 : att;` inherits `self` twice, and the pilot
    // states it as one warning naming both providers ("from DataValue, Part"), not as two.
    let src = "package P {
	attribute def AttDef;
}
";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = index
        .name
        .iter()
        .position(|n| n == "AttDef")
        .expect("AttDef");
    let supers: Vec<&str> = index
        .supertypes_of_public(id)
        .iter()
        .map(|&s| index.qualified[s].as_str())
        .collect();
    assert!(
        supers.iter().any(|s| s.ends_with("DataValue")),
        "an attribute definition specializes DataValue, got {supers:?}"
    );
}

#[test]
fn an_attribute_usage_has_no_implicit_data_value_base() {
    // An attribute *definition* specializes `DataValue`; an attribute *usage* does not. The
    // pilot states both facts: `part p2 : AttDef;` inherits `self` from `DataValue` and
    // `Parts::Part` (`PartUsage_invalid.sysml.xt:45`), while `attribute a : A;` with `A` a part
    // definition is reported only for its typing (`AttributeUsage_invalid.sysml.xt:45`).
    //
    // The question is about the element's own supertypes, not what a name resolves to through
    // its type: `attribute a : V` reaches `self` through `V`, and that is not this test.
    let src = "package P {
	attribute def V { attribute a : V; }
}
";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = index.name.iter().position(|n| n == "a").expect("a");
    let supers: Vec<&str> = index
        .supertypes_of_public(id)
        .iter()
        .map(|&s| index.qualified[s].as_str())
        .collect();
    assert!(
        !supers.iter().any(|s| s.ends_with("DataValue")),
        "an attribute usage must not specialize DataValue directly, got {supers:?}"
    );
}

#[test]
fn a_part_sees_what_the_library_part_provides() {
    // SysML gives every part an implicit `Parts::Part` supertype. Without it a part does not see
    // `performedActions` or `exhibitedStates`, and every reference to one dangles.
    let src = "package P {\n\tpart def Holder { ref action pp :> performedActions; }\n}\n";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = index.name.iter().position(|n| n == "pp").expect("pp");
    assert!(
        matches!(index.resolve(id, "performedActions"), Resolution::Found(_)),
        "a part inherits Part's members, got {:?}",
        index.resolve(id, "performedActions")
    );
}

#[test]
fn a_dotted_crosses_target_resolves_through_the_feature() {
    // `end x crosses y.b` names a member of the opposite end, not a name in the association. The
    // single-name lookup asked whether `y.b` is declared here, which it never is, so every dotted
    // crossing reported unresolved and `validateFeatureCrossFeatureType` judged it by raw text.
    let src = "package P {
	class C1;
	class C2 { feature b : C1; }
	assoc A1 {
		end y : C2;
		end x : C1 crosses y.b;
	}
}
";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = index.name.iter().position(|n| n == "x").expect("x");
    assert!(
        matches!(
            index.resolve_path(id, &sysml_model::NameRef::new("y.b", 1)),
            Resolution::Found(_)
        ),
        "`crosses y.b` must reach the feature, got {:?}",
        index.resolve_path(id, &sysml_model::NameRef::new("y.b", 1))
    );
}

#[test]
fn a_dotted_clause_through_the_element_itself_reaches_the_named_feature() {
    // `perform b.a` is a reference, so the performance itself declares no name and the lowering
    // nests a host called `a` under it; the host's clause names `b.a`, the path it sits on.
    // Resolving from the owner finds the host first, so the clause came back as a self-reference
    // and the host inherited nothing. Each enclosing scope is tried until one answers with
    // something that is neither the element nor an ancestor of it.
    let src = "package P {
	part def B { action a; }
	ref b : B;
	action outer { perform b.a; }
}
";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let host = (0..index.len())
        .find(|&i| index.qualified[i] == "P::outer::a")
        .expect("the host element");
    let supertypes: Vec<&str> = index
        .supertypes_of_public(host)
        .iter()
        .map(|&s| index.qualified[s].as_str())
        .collect();
    assert_eq!(supertypes, ["P::B::a"], "got {supertypes:?}");
}

#[test]
fn a_performance_whose_path_runs_through_itself_terminates() {
    // `perform p.a.b` lowers to a host `a` under the anonymous performance inside `p`, and the
    // host's clause names `p.a.b` — a path that starts at the part it sits in and runs through the
    // action beside it. Asking what the host specializes asks what it performs, which is the same
    // walk again. The recursion has no visited set and no depth bound that helps: each re-entry
    // starts from a fresh depth, so `check` on this three-line model never returned and the
    // resolver alone entered one element 9,711 times. The walk is bounded by the element being
    // marked while its own question is open, so this resolves — to anything — rather than
    // recursing.
    let src = "package P {
	part p {
		perform p.a.b;
		action a { action b; }
	}
}
";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let host = (0..index.len())
        .find(|&i| index.qualified[i] == "P::p::a")
        .expect("the lowered host");

    // This is the call that did not return: the host's own clause, resolved from the host.
    let _ = index.resolve(host, "p.a.b");

    // And the supertype walk on its own, which is the entry `check` reaches through the
    // lowered host before any rule runs.
    let _ = index.supertypes_of_public(host);
}

#[test]
fn a_usage_that_redefines_something_still_inherits_from_its_type() {
    // A usage may write both a redefinition and a type: `:>> transformation : Frame`. The walk
    // that answers "what do I inherit" used to return the redefinition target alone, so the type
    // was not an ancestor and a member declared on it was unreachable from the body:
    // `15_13-Discretely Sampled Function Value` reported `origin` unresolved for exactly this.
    let src = "package P {
	attribute def Base {
		attribute origin;
	}
	attribute def Frame :> Base;
	attribute def Holder {
		attribute transformation : Frame;
	}
	attribute def Scale :> Holder {
		attribute :>> transformation : Frame {
			:>> origin = 1;
		}
	}
}
";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let refs = index.references(&loaded.roots);
    let unresolved: Vec<&str> = refs
        .iter()
        .filter(|r| matches!(r.target, Resolution::Unresolved))
        .map(|r| r.text.as_str())
        .collect();
    assert!(
        !unresolved.contains(&"origin"),
        "`origin` is declared on `Base`, which `Frame` specializes; got {unresolved:?}"
    );
}

#[test]
fn a_redefinition_reaches_a_member_declared_two_levels_up() {
    // `member_of` answers with a scope's direct members, so the inherited lookup has to walk the
    // whole ancestry: `x` is declared on `Base`, reached through `Mid`. One step found nothing and
    // the reference was reported unresolved (`11a-View-Viewpoint.sysml:50` was that shape, with
    // `columnView` declared on a library type two steps from the rendering).
    let src = "package P {
	part def Base {
		part x;
	}
	part def Mid :> Base;
	part def Holder {
		part h : Mid;
	}
	part def Top :> Holder {
		part :>> h : Mid {
			part :>> x;
		}
	}
}
";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let refs = index.references(&loaded.roots);
    let unresolved: Vec<&str> = refs
        .iter()
        .filter(|r| matches!(r.target, Resolution::Unresolved))
        .map(|r| r.text.as_str())
        .collect();
    assert!(
        !unresolved.contains(&"x"),
        "`x` is declared on `Base`, two steps up from `h`; got {unresolved:?}"
    );
}

#[test]
fn a_redefinition_inside_a_performance_sees_what_the_performance_performs() {
    // `resolve_bare` includes the performed feature's members in scope; the walk the redefinition
    // branch uses did not, so `perform massTest { in part :>> testVehicle; }` could not see the
    // `testVehicle` that `massTest` declares, while a plain reference of the same name resolved
    // (`9-Verification-simplified.sysml:80`).
    let src = "package P {
	part def Vehicle;
	verification def MassTest {
		subject testVehicle : Vehicle;
	}
	verification massTest : MassTest;
	part system {
		perform massTest {
			in part :>> testVehicle;
		}
	}
}
";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let refs = index.references(&loaded.roots);
    let unresolved: Vec<&str> = refs
        .iter()
        .filter(|r| matches!(r.target, Resolution::Unresolved))
        .map(|r| r.text.as_str())
        .collect();
    assert!(
        !unresolved.contains(&"testVehicle"),
        "`testVehicle` belongs to what the performance performs; got {unresolved:?}"
    );
}

#[test]
fn a_root_level_import_does_not_shadow_its_own_target() {
    // An import element is stored under the text of its target, so `private import Views::*;` at
    // the top of a file is a top-level element *named* `Views`. Resolving `Views` answered that
    // statement instead of the package, nothing was bound, and every rendering name after it was
    // reported unresolved — measured on `11b-Safety and Security Feature Views.sysml`, whose first
    // line is exactly that import.
    let src = "private import Views::*;
package P {
	view v {
		render asElementTable;
	}
}
";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let import = (0..index.len())
        .find(|&i| index.kind[i] == sysml_model::Kind::Import)
        .expect("the import");
    let resolved = index.resolve(import, "Views");
    let target = match resolved {
        Resolution::Found(id) => id,
        other => panic!("`Views` must resolve to the package, got {other:?}"),
    };
    assert_ne!(
        index.kind[target],
        sysml_model::Kind::Import,
        "`Views` resolved to the import statement itself"
    );
    assert!(
        matches!(
            index.resolve(import, "asElementTable"),
            Resolution::Found(_)
        ),
        "the import must bind the package's members"
    );
}
