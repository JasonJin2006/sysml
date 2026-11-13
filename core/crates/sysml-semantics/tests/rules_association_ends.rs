//! A concrete association relates at least two types —
//! `validateAssociationRelatedTypes`.

use sysml_semantics::{check, Severity};

fn sem049(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM049")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_one_ended_association_is_rejected() {
    let src = r#"
package P {
	part def A;
	assoc R {
		end a : A;
	}
}
"#;
    assert_eq!(sem049(src).len(), 1, "got {:?}", sem049(src));
}

#[test]
fn a_two_ended_association_is_accepted() {
    let src = r#"
package P {
	part def A;
	part def B;
	assoc R {
		end a : A;
		end b : B;
	}
}
"#;
    assert!(sem049(src).is_empty());
}

#[test]
fn an_abstract_association_is_exempt() {
    let src = r#"
package P {
	part def A;
	abstract assoc R {
		end a : A;
	}
}
"#;
    assert!(
        sem049(src).is_empty(),
        "an abstract association is the pattern the concrete ones fill in"
    );
}

#[test]
fn an_untyped_end_still_relates_a_type() {
    // Every end has a type: the one it writes, or the implied `Links::Link::participant`
    // subsetting the specification gives an end that writes none. So `end b;` relates a type
    // just as `end a : A;` does, and the pair is binary.
    //
    // This test asserted the opposite until 2026-11-01. The reading it encoded — "relatedType is
    // the type the end names, so an end without one relates nothing" — is not what the OCL says
    // once the implied subsetting is accounted for, and the pilot's
    // `AssociationTest_EndRedefinitionGoodCase.kerml.xt` (a `noErrors` model) has two bare ends.
    let src = r#"
package P {
	part def A;
	assoc R {
		end a : A;
		end b;
	}
}
"#;
    assert!(
        sem049(src).is_empty(),
        "two ends relate two types, got {:?}",
        sem049(src)
    );
}

#[test]
fn an_end_inherited_from_a_supertype_counts() {
    // `assoc A2 specializes A1 { end x : C; }` has one end of its own and one it inherits, so it
    // relates two types. Counting own children reported it as one, which the pilot does not say
    // (`AssociationTest_EndTypes_invalid.kerml.xt` states an arity error only where the count is
    // genuinely short).
    let src = r#"
package P {
	part def A;
	part def B;
	part def C;
	assoc A1 {
		end x : A;
		end y : B;
	}
	assoc A2 specializes A1 {
		end x : C;
	}
}
"#;
    assert!(
        sem049(src).is_empty(),
        "the inherited end counts, got {:?}",
        sem049(src)
    );
}

#[test]
fn a_one_ended_association_structure_is_rejected() {
    // `assoc struct` is `AssociationStructure`, an `Association`, so `validateAssociationRelatedTypes`
    // applies. The kind was missing from the rule's association test, so this reported nothing.
    let src = r#"
package P {
	part def A;
	assoc struct R {
		end a : A;
	}
}
"#;
    let found = sem049(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("1 end(s)"),
        "the count is the message's point: {found:?}"
    );
}

#[test]
fn a_two_ended_association_structure_is_accepted() {
    // The nearest conforming model. Requirement: a valid binary `assoc struct` is accepted.
    let src = r#"
package P {
	part def A;
	part def B;
	assoc struct R {
		end a : A;
		end b : B;
	}
}
"#;
    assert!(sem049(src).is_empty(), "got {:?}", sem049(src));
}

#[test]
fn an_abstract_association_structure_is_exempt() {
    // `Links::LinkObject` and `Links::BinaryLinkObject` are written `abstract assoc struct`; an
    // exemption that did not cover the combined kind would report the library itself.
    let src = r#"
package P {
	part def A;
	abstract assoc struct R {
		end a : A;
	}
}
"#;
    assert!(sem049(src).is_empty(), "got {:?}", sem049(src));
}

#[test]
fn an_association_structure_inherits_both_of_its_ends() {
    // The combined kind's ends can live in a base: `R` writes none and has `Base`'s two in force, so
    // it relates two types. The same shape is how `Objects::BinaryLinkObject` gets the ends of
    // `BinaryLink`, and the rule must count them for an `assoc struct` just as for an `assoc`.
    let src = r#"
package P {
	part def A;
	part def B;
	abstract assoc struct Base {
		end a : A;
		end b : B;
	}
	assoc struct R specializes Base;
}
"#;
    assert!(sem049(src).is_empty(), "got {:?}", sem049(src));
}

#[test]
fn an_end_redeclared_by_name_inside_an_association_structure_replaces_the_inherited_one() {
    // The subtype writes one end and inherits two: the written `a` redefines the inherited `a` by
    // name, and `c` is inherited untouched, so two ends are in force. Counting only the written
    // children reported one; a name-keyed count of the written and inherited ends would report
    // three.
    let src = r#"
package P {
	part def A;
	abstract assoc struct Base {
		end a : A;
		end c : A;
	}
	assoc struct R specializes Base {
		end a : A;
	}
}
"#;
    assert!(sem049(src).is_empty(), "got {:?}", sem049(src));
}

#[test]
fn two_inherited_ends_that_share_a_name_are_still_two_ends() {
    // The direction the name-based count got wrong on real models: `R` inherits `x` from two
    // unrelated associations. Neither end redefines the other — redefinition is positional or
    // explicit, and neither clause says so — but both are named `x`, so keying by name collapsed
    // them into one and reported this conforming binary relation as one-ended.
    //
    // `Left` and `Right` are abstract so that they are exempt from this same rule: each has one end
    // of its own, which is what a pattern that concrete relations fill in looks like.
    let src = r#"
package P {
	part def A;
	part def B;
	abstract assoc struct Left {
		end x : A;
	}
	abstract assoc struct Right {
		end x : B;
	}
	assoc struct R specializes Left, Right;
}
"#;
    assert!(sem049(src).is_empty(), "got {:?}", sem049(src));
}

#[test]
fn a_positional_replacement_does_not_add_an_end() {
    // The direction the name-based count got wrong: `R`'s single end sits at position 0, so it
    // implicitly redefines `Base`'s only end rather than adding a second one. The names differ
    // (`c` against `a`), which is why keying by name counted two and called this conforming.
    // Redefinition is positional or explicit, and position says one end is in force.
    let src = r#"
package P {
	part def A;
	part def C;
	abstract assoc struct Base {
		end a : A;
	}
	assoc struct R specializes Base {
		end c : C;
	}
}
"#;
    let found = sem049(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("1 end(s)"),
        "the replaced end is not a second end: {found:?}"
    );
}

#[test]
fn an_end_with_two_types_is_rejected() {
    // `validateAssociationEndTypes` states `type->size() = 1`, so both zero and more than one are
    // violations. The pilot implementation's AssociationTest_EndTypes_invalid marks the second:
    // `end y : C1, C2;`.
    let src = r#"
package P {
	part def C1;
	part def C2;
	assoc A1 {
		end y : C1, C2;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let report = check(&loaded.roots, &loaded.library);
    let found: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.code == "SEM041" && i.message.contains("names 2 types"))
        .collect();
    assert_eq!(found.len(), 1, "got {}", found.len());
}

#[test]
fn an_end_with_one_type_is_accepted() {
    let src = r#"
package P {
	part def C1;
	assoc A1 {
		end y : C1;
		end z : C1;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let report = check(&loaded.roots, &loaded.library);
    let found: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.code == "SEM041")
        .collect();
    assert!(found.is_empty(), "got {}", found.len());
}
