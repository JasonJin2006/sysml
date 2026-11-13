//! `validateRedefinitionDirectionConformance`: a redefinition keeps its target's direction.
//!
//! Four of the five cases the OMG pilot implementation states are covered
//! (`Redefinition_DirectionConformance_invalid.kerml`): two direct inherited members, the nested
//! one, and the one reached through a conjugation. The fifth is the other end of the nested pair,
//! which the OCL gives no subject — the module note records why.

use sysml_semantics::{check, Severity};

fn sem081(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM081")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn redefining_an_in_feature_as_out_is_rejected() {
    // The pilot implementation's `class D specializes C { out :>> z; }` against `class C { in z; }`.
    let src = r#"
package P {
	class C { in z; }
	class D specializes C {
		out :>> z;
	}
}
"#;
    assert_eq!(sem081(src).len(), 1, "got {:?}", sem081(src));
}

#[test]
fn redefining_an_out_feature_as_in_is_rejected() {
    let src = r#"
package P {
	class C { out z; }
	class D specializes C {
		in :>> z;
	}
}
"#;
    assert_eq!(sem081(src).len(), 1, "got {:?}", sem081(src));
}

#[test]
fn keeping_the_direction_is_accepted() {
    let src = r#"
package P {
	class C { in z; }
	class D specializes C {
		in :>> z;
	}
}
"#;
    assert!(sem081(src).is_empty(), "got {:?}", sem081(src));
}

#[test]
fn inout_conforms_to_either() {
    let src = r#"
package P {
	class C { inout z; }
	class D specializes C {
		in :>> z;
	}
}
"#;
    assert!(
        sem081(src).is_empty(),
        "`inout` is the union of the other two, got {:?}",
        sem081(src)
    );
}

#[test]
fn a_target_with_no_direction_is_not_judged() {
    let src = r#"
package P {
	class C { z; }
	class D specializes C {
		in :>> z;
	}
}
"#;
    assert!(
        sem081(src).is_empty(),
        "nothing to conform to, got {:?}",
        sem081(src)
    );
}

#[test]
fn a_member_of_a_nested_usage_is_judged_against_its_own_type() {
    // `A` owns `b : B`, and `B` declares `out x1`; so `in x1` inside `b` redefines it. The pilot
    // implementation asserts exactly this (`Redefinition_DirectionConformance_invalid.kerml.xt:30`,
    // "Redefining feature must have a compatible direction" at `in x1;`), and this test used to
    // pin the opposite because the same-name redefinition was not read.
    let src = r#"
package P {
	behavior A { in x; composite step b : B { in x1; } }
	behavior B specializes A { out x1; }
}
"#;
    let found = sem081(src);
    assert!(
        found.iter().any(|f| f.1.ends_with("A::b::x1")),
        "reported at the member: {found:?}"
    );
    // And `out x1` in `B` redefines `A::x` by position, which is `in`: the pilot asserts that too
    // (`:35`). Without positional parameter redefinition it was not judged at all.
    assert!(
        found
            .iter()
            .any(|f| f.1.ends_with("B::x1") && f.2.contains("redefines `x` from `A`")),
        "the positional redefinition is judged: {found:?}"
    );
    assert_eq!(found.len(), 2, "got {found:?}");
}

#[test]
fn a_parameter_redefines_the_parameter_at_its_position() {
    // `out y1` takes `inout y`'s place, which conforms; only the first pair conflicts.
    let src = r#"
package P {
	behavior A { in x; inout y; }
	behavior B specializes A { in x1; out y1; }
	behavior C specializes A { out x1; out y1; }
	function F { in x; }
}
"#;
    let found = sem081(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].1.ends_with("C::x1"), "got {found:?}");
}

#[test]
fn a_redefinition_inherited_through_a_conjugation_is_judged_flipped() {
    // `class C1 conjugates C` gives `C1` the members of `C` with every direction turned around, so
    // the `in` this redefinition writes is the wrong side of the flip. The pilot implementation
    // asserts this at `Redefinition_DirectionConformance_invalid.kerml.xt:52`. Before the member
    // walk followed conjugation edges, `C1` looked like an empty type and the redefinition was
    // skipped rather than judged.
    let src = r#"
package P {
	class C { in z; }
	class C1 conjugates C;
	class D specializes C1 {
		in :>> z;
	}
}
"#;
    let found = sem081(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("through a conjugation"),
        "the report says why the expectation is flipped: {found:?}"
    );
}

#[test]
fn two_conjugations_cancel() {
    // Parity, not a flag: a path that crosses two conjugations arrives the right way round.
    let src = r#"
package P {
	class C { in z; }
	class C1 conjugates C;
	class C2 conjugates C1;
	class D specializes C2 {
		in :>> z;
	}
}
"#;
    assert!(
        sem081(src).is_empty(),
        "two flips are no flip, got {:?}",
        sem081(src)
    );
}
