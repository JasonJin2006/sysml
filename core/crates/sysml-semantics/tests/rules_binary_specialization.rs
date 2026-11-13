//! A link with more than two ends is not a binary link —
//! `validateAssociationBinarySpecialization` and `validateConnectorBinarySpecialization`.

use sysml_semantics::{check, Severity};

fn sem078(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM078")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_three_ended_association_specializing_binary_link_is_rejected() {
    let src = r#"
package P {
	private import Links::*;
	part def A;
	assoc Ternary :> BinaryLink {
		end x : A;
		end y : A;
		end z : A;
	}
}
"#;
    assert_eq!(sem078(src).len(), 1, "got {:?}", sem078(src));
}

#[test]
fn a_two_ended_association_specializing_binary_link_is_accepted() {
    let src = r#"
package P {
	private import Links::*;
	part def A;
	assoc Towing :> BinaryLink {
		end x : A;
		end y : A;
	}
}
"#;
    assert!(sem078(src).is_empty(), "got {:?}", sem078(src));
}

#[test]
fn a_three_ended_association_structure_specializing_binary_link_is_rejected() {
    // `assoc struct` is `AssociationStructure`, whose supers are `Association, Structure`;
    // `validateAssociationBinarySpecialization` is stated of `Association`, so a ternary combined
    // kind that names `Links::BinaryLink` contradicts its base's two ends exactly as the `assoc`
    // spelling does. Before the kind was added to the rule's subject test, this reported nothing.
    let src = r#"
package P {
	private import Links::*;
	part def A;
	assoc struct Ternary :> BinaryLink {
		end x : A;
		end y : A;
		end z : A;
	}
}
"#;
    let found = sem078(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("has 3 ends"),
        "the message must report the count it judged: {found:?}"
    );
}

#[test]
fn a_two_ended_association_structure_specializing_binary_link_is_accepted() {
    // The nearest conforming model under the combined kind.
    let src = r#"
package P {
	private import Links::*;
	part def A;
	assoc struct Pairing :> BinaryLink {
		end x : A;
		end y : A;
	}
}
"#;
    assert!(sem078(src).is_empty(), "got {:?}", sem078(src));
}

#[test]
fn a_three_ended_association_structure_not_specializing_binary_link_is_accepted() {
    // N-ary associations are legal; only claiming a binary base while having three ends is not.
    let src = r#"
package P {
	private import Links::*;
	part def A;
	assoc struct Hyperedge {
		end x : A;
		end y : A;
		end z : A;
	}
}
"#;
    assert!(sem078(src).is_empty(), "got {:?}", sem078(src));
}

#[test]
fn an_association_structure_that_inherits_its_two_ends_is_accepted() {
    // `Objects::BinaryLinkObject` is exactly this shape: `assoc struct BinaryLinkObject :>
    // BinaryLink, LinkObject` writes no end of its own and has the two `BinaryLink` provides. They
    // are ends of the subtype, not of the base alone, so the relation is binary and there is
    // nothing to report. Counting the authored children saw zero here and still returned "two or
    // fewer", which is why the accepted verdict is the same either way; reading effective ends
    // makes the reason explicit rather than accidental. The local name differs from the library's
    // so that the wildcard import does not collide with the declaration.
    let src = r#"
package P {
	private import Links::*;
	private import Objects::*;
	assoc struct LinkPairObject :> BinaryLink, LinkObject;
}
"#;
    assert!(sem078(src).is_empty(), "got {:?}", sem078(src));
}

#[test]
fn an_association_structure_that_inherits_three_ends_is_rejected() {
    // One written end, five inherited, four in force: positional redefinition replaces position 0
    // of each base (`BinaryLink::source` and `Triple::e`), so `Quad` conducts `x`, `target`, `f` and
    // `g`. The authored children say one end and the rule would stay silent; the base it claims is
    // contradicted by the ends it actually has.
    let src = r#"
package P {
	private import Links::*;
	part def A;
	abstract assoc struct Triple {
		end e : A;
		end f : A;
		end g : A;
	}
	assoc struct Quad :> BinaryLink, Triple {
		end x : A;
	}
}
"#;
    let found = sem078(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("has 4 ends"),
        "inherited ends are ends of the subtype: {found:?}"
    );
}

#[test]
fn a_three_ended_association_not_specializing_binary_link_is_accepted() {
    let src = r#"
package P {
	private import Links::*;
	part def A;
	assoc Ternary {
		end x : A;
		end y : A;
		end z : A;
	}
}
"#;
    assert!(
        sem078(src).is_empty(),
        "only a binary base is contradicted, got {:?}",
        sem078(src)
    );
}
