//! Scope resolution: shadowing, qualified references, and the cases that must fail loudly.

use sysml_resolve::{Index, Resolution};

fn load(src: &str) -> (Vec<sysml_model::Element>, Index) {
    let loaded = sysml_syntax::load(src).expect("parses");
    let index = Index::build(&loaded.roots);
    (loaded.roots, index)
}

fn id_of(index: &Index, qualified: &str) -> usize {
    index
        .qualified
        .iter()
        .position(|q| q == qualified)
        .unwrap_or_else(|| panic!("no element {qualified}"))
}

#[test]
fn same_name_in_two_packages_binds_to_the_one_in_scope() {
    let (_roots, index) = load(
        r#"
package Left {
    part def Wheel;
    part def Bike {
        part front : Wheel;
    }
}
package Right {
    part def Wheel;
    part def Cart {
        part wheel : Wheel;
    }
}
"#,
    );

    let left_wheel = id_of(&index, "Left::Wheel");
    let right_wheel = id_of(&index, "Right::Wheel");

    assert_eq!(
        index.resolve(id_of(&index, "Left::Bike::front"), "Wheel"),
        Resolution::Found(left_wheel)
    );
    assert_eq!(
        index.resolve(id_of(&index, "Right::Cart::wheel"), "Wheel"),
        Resolution::Found(right_wheel),
        "a leaf-name match would return Left::Wheel here"
    );
}

#[test]
fn an_inner_definition_shadows_the_outer_one() {
    let (_roots, index) = load(
        r#"
package P {
    part def Wheel {
        attribute spokes : Real;
    }
    part def Holder {
        part def Wheel {
            attribute diameter : Real;
        }
        part w : Wheel;
    }
}
"#,
    );

    let inner = id_of(&index, "P::Holder::Wheel");
    assert_eq!(
        index.resolve(id_of(&index, "P::Holder::w"), "Wheel"),
        Resolution::Found(inner)
    );
}

#[test]
fn a_qualified_reference_crosses_namespaces() {
    let (_roots, index) = load(
        r#"
package A {
    part def Thing;
}
package B {
    part x : A::Thing;
}
"#,
    );

    assert_eq!(
        index.resolve(id_of(&index, "B::x"), "A::Thing"),
        Resolution::Found(id_of(&index, "A::Thing"))
    );
}

#[test]
fn a_name_the_tree_does_not_contain_is_unresolved() {
    // `Real` lives in the ScalarValues library, which is imported but not part of this tree.
    // Reporting it is the point: silently binding it to something would be worse than saying so.
    let (roots, index) = load(
        r#"
package P {
    part def Widget {
        attribute weight : Real;
    }
}
"#,
    );

    let references = index.references(&roots);
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].text, "Real");
    assert_eq!(references[0].target, Resolution::Unresolved);
}

#[test]
fn ownership_comes_from_the_id_assigned_at_load() {
    let (roots, index) = load(
        r#"
package P {
    part def A {
        part b : B;
    }
    part def B;
}
"#,
    );

    // Ids are pre-order, so every child's id is greater than its parent's.
    assert_eq!(
        index.qualified.len(),
        roots.iter().map(sysml_model::Element::size).sum::<usize>()
    );
    assert_eq!(
        index.parent[id_of(&index, "P::A::b")],
        Some(id_of(&index, "P::A"))
    );
    assert!(index.parent[id_of(&index, "P")].is_none());
}

#[test]
fn a_satisfy_statement_does_not_collide_with_its_requirement() {
    // The satisfy element is named after the requirement it points at, so the two are
    // same-name members of one package. A member walk that counted relationships would call
    // every such reference ambiguous, and the view would drop the edge without a word.
    let (_roots, index) = load(
        r#"
package P {
    requirement def HoverEndurance {
        subject d : Drone;
        require constraint { d.endurance >= 20 [min] }
    }
    part def Drone {
        attribute endurance : Real;
    }
    part drone : Drone;
    satisfy HoverEndurance by drone;
}
"#,
    );

    let satisfy = (0..index.len())
        .find(|&i| index.kind[i] == sysml_model::Kind::Satisfy)
        .expect("a satisfy element is lowered");
    assert_eq!(
        index.resolve(satisfy, "HoverEndurance"),
        Resolution::Found(id_of(&index, "P::HoverEndurance")),
        "the requirement must win over the satisfy element named after it"
    );
    assert_eq!(
        index.resolve(satisfy, "drone"),
        Resolution::Found(id_of(&index, "P::drone"))
    );
}

#[test]
fn an_imported_requirement_is_not_shadowed_by_the_satisfy_naming_it() {
    // Same collision, other direction: when the requirement lives elsewhere and arrives by
    // import, the satisfy element is the only local member with that name — resolving to
    // itself instead of the binding would point the edge at a non-node and drop it.
    let (_roots, index) = load(
        r#"
package R {
    requirement def HoverEndurance {
        subject d : Drone;
    }
}
package P {
    private import R::*;
    part def Drone;
    part drone : Drone;
    satisfy HoverEndurance by drone;
}
"#,
    );

    let satisfy = (0..index.len())
        .find(|&i| index.kind[i] == sysml_model::Kind::Satisfy)
        .expect("a satisfy element is lowered");
    assert_eq!(
        index.resolve(satisfy, "HoverEndurance"),
        Resolution::Found(id_of(&index, "R::HoverEndurance"))
    );
}
