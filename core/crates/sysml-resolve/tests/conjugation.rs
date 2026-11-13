//! A conjugated type provides what it conjugates, and the walk knows the edge turned.
//!
//! `class C1 conjugates C` means `C1` has `C`'s features with every direction flipped. Member
//! lookup therefore has to reach through the conjugation — `C1` declares nothing of its own — and
//! a rule that asks which direction a redefinition must match has to know the path crossed one.

use sysml_resolve::{Index, Resolution};

fn index(src: &str) -> Index {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    Index::build(&loaded.roots).with_library(&loaded.library)
}

fn id_of(index: &Index, qualified: &str) -> usize {
    index
        .qualified
        .iter()
        .position(|q| q == qualified)
        .unwrap_or_else(|| panic!("no element `{qualified}`; have {:?}", index.qualified))
}

const MODEL: &str = r#"
package P {
	class C {
		in z;
	}
	class C1 conjugates C;
	class D specializes C1 {
		in :>> z;
	}
}
"#;

#[test]
fn a_conjugated_type_is_an_edge_of_the_member_walk() {
    let index = index(MODEL);
    let c = id_of(&index, "P::C");
    let c1 = id_of(&index, "P::C1");
    assert_eq!(index.conjugate_edges(c1), vec![c]);
}

#[test]
fn a_member_of_the_conjugated_type_is_inherited_through_it() {
    // `C1` declares no `z`; the `z` a redefinition inside `D` names is `C`'s.
    let index = index(MODEL);
    let c = id_of(&index, "P::C");
    let z = id_of(&index, "P::C::z");
    let d = id_of(&index, "P::D");
    assert_eq!(index.inherited_member_of(d, "z"), Some(z));
    assert_ne!(c, d);
}

#[test]
fn a_conjugation_two_steps_down_still_supplies_the_member() {
    // `D :> C1 conjugates C` — the conjugation is one step above the redefinition's owner.
    let index = index(MODEL);
    let z = id_of(&index, "P::C::z");
    let d = id_of(&index, "P::D");
    assert_eq!(index.inherited_member_of(d, "z"), Some(z));
}
