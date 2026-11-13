//! The derived properties: each one both ways, on models where the answer is known.
//!
//! A derived property that answers "nothing" for everything looks identical to one that works, so
//! every test here has a model that must produce a value and a model that must not.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;
use sysml_semantics::derived;

fn load(src: &str) -> (Index, Vec<Element>) {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots);
    (index, loaded.roots)
}

/// The same, with the standard library attached: the inherited parameters only exist there.
fn load_with_library(src: &str) -> (Index, Vec<Element>) {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    (index, loaded.roots)
}

fn id_of(roots: &[Element], name: &str) -> usize {
    fn go(el: &Element, name: &str) -> Option<usize> {
        if el.name == name {
            return Some(el.id);
        }
        el.children.iter().find_map(|c| go(c, name))
    }
    roots
        .iter()
        .find_map(|r| go(r, name))
        .unwrap_or_else(|| panic!("no element named `{name}`"))
}

fn names(index: &Index, ids: &[usize]) -> Vec<String> {
    ids.iter().map(|&id| index.name[id].clone()).collect()
}

// -- inputParameters ---------------------------------------------------------

const ACTION: &str = r#"
package P {
	action def Move {
		in distance : Real;
		inout ok : Boolean;
		out verdict : Boolean;
		attribute plain : Real;
	}
}
"#;

#[test]
fn input_parameters_are_the_in_and_inout_features() {
    let (index, roots) = load(ACTION);
    let move_def = id_of(&roots, "Move");
    let mut got = names(&index, &derived::input_parameters(&index, move_def));
    got.sort();
    assert_eq!(
        got,
        ["distance", "ok"],
        "`in` and `inout` are inputs; `out` is not"
    );
}

#[test]
fn a_declaration_with_no_direction_has_no_input_parameters() {
    let src = r#"
package P {
	action def Move {
		attribute plain : Real;
	}
}
"#;
    let (index, roots) = load(src);
    let move_def = id_of(&roots, "Move");
    assert!(derived::input_parameters(&index, move_def).is_empty());
}

#[test]
fn a_subject_is_an_input_parameter() {
    // `SubjectMembership` is a `ParameterMembership` whose `parameterDirection()` is `_'in'`, and
    // the tree does not derive that direction onto the subject.
    let src = r#"
package P {
	requirement def R {
		subject s : P;
	}
	part def P;
}
"#;
    let (index, roots) = load(src);
    let r = id_of(&roots, "R");
    let params = derived::input_parameters(&index, r);
    assert_eq!(names(&index, &params), ["s"]);
}

// -- resultParameter --------------------------------------------------------

#[test]
fn a_return_clause_is_the_result_parameter() {
    let src = r#"
package P {
	calc def Double {
		in x : Real;
		return : Real = x * 2;
	}
}
"#;
    let (index, roots) = load(src);
    let calc = id_of(&roots, "Double");
    let result = derived::result_parameter(&index, calc).expect("a return clause");
    assert_eq!(index.clause_of(result), Some("return"));
    assert!(!derived::input_parameters(&index, calc).contains(&result));
}

#[test]
fn a_calculation_without_a_return_has_no_result() {
    let src = r#"
package P {
	calc def Nothing {
		in x : Real;
	}
}
"#;
    let (index, roots) = load(src);
    let calc = id_of(&roots, "Nothing");
    assert_eq!(derived::result_parameter(&index, calc), None);
}

// -- chainingFeature / featureTarget ---------------------------------------

#[test]
fn a_chain_clause_names_the_chaining_features() {
    let src = r#"
package P {
	part def A {
		part b : A;
	}
	part def Holder {
		part c : A;
		part f chains c.b;
	}
}
"#;
    let (index, roots) = load(src);
    let f = id_of(&roots, "f");
    let chain = derived::chaining_features(&index, f);
    // `chainingFeature` is the sequence of features the chain passes through, so `chains c.b`
    // names both `c` and the `b` inside `c`'s type. Reading only the last one made
    // `validateFeatureChainingFeatureConformance` compare a feature with itself.
    assert_eq!(
        names(&index, &chain),
        ["c", "b"],
        "`chains c.b` names `c` and then the `b` inside its type"
    );
    assert_eq!(
        index.qualified[derived::feature_target(&index, f)],
        "P::A::b"
    );
}

#[test]
fn a_cross_feature_does_not_fall_back_to_the_enclosing_scope() {
    // `crosses a.b` names `b` inside what `a` is. When `a`'s type declares no `b` but the package
    // does, a scope walk binds the package's `b` — a feature the crossed end says nothing about —
    // while the reference pass reports the same path unresolved (`resolve_path` walks a dotted
    // path member by member). The derivation has to agree with the reference pass.
    let src = r#"
package P {
	part def W;
	part a : W;
	attribute b;
	part def Q {
		end e : W crosses a.b;
	}
}
"#;
    let (index, roots) = load(src);
    let e = id_of(&roots, "e");
    assert_eq!(derived::cross_feature(&index, e), None);
}

#[test]
fn a_feature_with_no_chain_is_its_own_target() {
    let (index, roots) = load(ACTION);
    let plain = id_of(&roots, "plain");
    assert!(derived::chaining_features(&index, plain).is_empty());
    assert_eq!(derived::feature_target(&index, plain), plain);
}

// -- crossFeature -----------------------------------------------------------

#[test]
fn a_cross_feature_is_read_from_the_crossed_feature_chain() {
    // `deriveFeatureCrossFeature`: the second chaining feature of the crossed feature. The tree
    // keeps `crosses` as authored text, so the crossed feature has to resolve first.
    let src = r#"
package P {
	part def A {
		part b : A;
	}
	part def Holder {
		part c : A {
			part b : A;
		}
		end e : A crosses c;
	}
}
"#;
    let (index, roots) = load(src);
    let e = id_of(&roots, "e");
    // `c` writes no `chains`, so the crossed feature has fewer than two chaining features: the
    // derivation returns null, and so does this.
    assert_eq!(derived::cross_feature(&index, e), None);
}

#[test]
fn a_declaration_with_no_crosses_has_no_cross_feature() {
    let (index, roots) = load(ACTION);
    let plain = id_of(&roots, "plain");
    assert_eq!(derived::cross_feature(&index, plain), None);
}

// -- referencedFeatureTarget -----------------------------------------------

#[test]
fn a_satisfy_references_the_requirement_it_names() {
    let src = r#"
package P {
	requirement def R1;
	requirement r2 : R1;
	part p {
		satisfy r2 by p;
	}
}
"#;
    let (index, roots) = load(src);
    // `r2` names two elements: the requirement and the satisfy inside `p`. The qualified name is
    // what tells them apart, and the satisfy is the one with a reference.
    let satisfy = (0..index.len())
        .find(|&id| index.qualified[id] == "P::p::r2")
        .expect("the satisfy");
    let target = derived::referenced_feature_target(&index, satisfy).expect("a reference");
    assert_eq!(index.qualified[target], "P::r2");
    assert!(index.is_kind_of(target, "RequirementUsage"));
    // `satisfy req : R1` names its type rather than a feature: the source end is empty and the
    // rule's own `referencedFeatureTarget() <> null` antecedent excludes it.
    let typed = r#"
package P {
	requirement def R1;
	requirement r3 {
		satisfy req : R1;
	}
}
"#;
    let (index, _roots) = load(typed);
    let satisfy = (0..index.len())
        .find(|&id| index.qualified[id] == "P::r3::req")
        .expect("the typed satisfy");
    assert_eq!(derived::referenced_feature_target(&index, satisfy), None);
}

#[test]
fn a_declaration_with_no_reference_has_no_target() {
    let src = r#"
package P {
	assert constraint { 1 > 0 }
}
"#;
    let (index, roots) = load(src);
    let assert_el = id_of(&roots, "assert");
    assert_eq!(derived::referenced_feature_target(&index, assert_el), None);
}

// -- type ------------------------------------------------------------------

#[test]
fn a_redundant_supertype_is_not_a_second_type() {
    // `deriveFeatureType`: "the union of the types of its typings and the types of the Features it
    // subsets, with all redundant supertypes removed". `B :> A`, so `p : A, B` is of type `{B}`.
    let src = r#"
package P {
	part def A;
	part def B :> A;
	part p : A, B;
}
"#;
    let (index, roots) = load(src);
    let p = id_of(&roots, "p");
    assert_eq!(names(&index, &derived::types(&index, p)), ["B"]);
}

#[test]
fn a_subsetting_feature_inherits_the_type_of_what_it_subsets() {
    // The written clause is empty; the type is not. `typingFeatures()` includes the subsetted
    // feature, and `type` takes the closure, so `q` is typed like `p`.
    let src = r#"
package P {
	part def A;
	part p : A;
	part q subsets p;
}
"#;
    let (index, roots) = load(src);
    let q = id_of(&roots, "q");
    assert_eq!(names(&index, &derived::types(&index, q)), ["A"]);
}

#[test]
fn two_unrelated_types_are_both_kept() {
    // The other direction: removing "redundant" supertypes must not remove types that are merely
    // several.
    let src = r#"
package P {
	part def A;
	part def B;
	part p : A, B;
}
"#;
    let (index, roots) = load(src);
    let p = id_of(&roots, "p");
    let mut got = names(&index, &derived::types(&index, p));
    got.sort();
    assert_eq!(got, ["A", "B"]);
}

#[test]
fn an_owned_cross_feature_inherits_its_owning_ends_type() {
    let src = r#"
package P {
	part def A;
	part def B;
	part def Base {
		end inherited : A;
	}
	part def Holder :> Base {
		end cross [0..1] feature prefix : A;
		end [0..1] part anonymousOwner : A;
		end inherited {
			member feature fromInherited;
		}
		end typedEnd : B {
			member feature body [0..1] featured by B;
			part ordinary;
		}
		end base : A;
		end subsetted subsets base {
			member feature fromSubset;
		}
		end redefined :>> base {
			member feature fromRedefinition;
		}
	}
}
"#;
    let (index, roots) = load(src);
    let cases = [
        ("fromSubset", "A"),
        ("fromRedefinition", "A"),
        ("fromInherited", "A"),
    ];
    for (name, expected) in cases {
        let child = id_of(&roots, name);
        assert!(
            index.types_of(child).is_empty(),
            "{name} has no authored type clause"
        );
        assert_eq!(
            names(&index, &derived::types(&index, child)),
            [expected],
            "{name}"
        );
    }

    for (end_name, expected) in [("prefix", "A"), ("anonymousOwner", "A"), ("typedEnd", "B")] {
        let end = id_of(&roots, end_name);
        let child = derived::owned_cross_feature(&index, end).expect("the owned cross child");
        assert!(
            index.types_of(child).is_empty(),
            "{end_name} child has no authored type clause"
        );
        assert_eq!(
            names(&index, &derived::types(&index, child)),
            [expected],
            "{end_name}"
        );
    }

    let ordinary = id_of(&roots, "ordinary");
    assert!(index.types_of(ordinary).is_empty());
    assert!(
        derived::types(&index, ordinary).is_empty(),
        "FeatureMembership children do not inherit the end type"
    );
}

#[test]
fn an_owned_cross_feature_inherits_types_through_a_cyclic_subsetting_closure() {
    let src = r#"
package P {
	part def A;
	part def Holder {
		end around [0..1] feature left : A subsets right;
		end right subsets left;
	}
}
"#;
    let (index, roots) = load(src);
    let around = id_of(&roots, "around");
    assert_eq!(names(&index, &derived::types(&index, around)), ["A"]);
}

#[test]
fn a_feature_with_no_type_and_no_subsetting_has_none() {
    let src = r#"
package P {
	part q;
}
"#;
    let (index, roots) = load(src);
    let q = id_of(&roots, "q");
    assert!(
        derived::types(&index, q).is_empty(),
        "an untyped, unsubsetting feature has no type"
    );
}

// -- inputParameters, counting what the library supplies ----------------

#[test]
fn an_accept_and_a_send_inherit_their_parameters() {
    // `validateAcceptActionUsageParameters` wants `inputParameters()->size() >= 2` and the send
    // rule wants `>= 3`. Neither number is written by the model: `AcceptPerformance` declares
    // `inout payload` and `in receiver` (`Transfers.kerml:254-265`), and `SendPerformance` the
    // three. Counting only the local members made both rules unsatisfiable.
    let cases: [(&str, &[&str]); 2] = [
        ("accept;", &["payload", "receiver"]),
        ("send;", &["payload", "sender", "receiver"]),
    ];
    for (body, wanted) in cases {
        let src = format!(
            "package P {{
	action def A {{
		{body}
	}}
}}
"
        );
        let (index, roots) = load_with_library(&src);
        // The accept/send action is the member of `A`, not `A` itself: the parameters belong to
        // the action usage, and `A` is the definition that contains it.
        fn action(el: &Element) -> Option<usize> {
            if matches!(el.kind, Kind::AcceptActionUsage | Kind::SendActionUsage) {
                return Some(el.id);
            }
            el.children.iter().find_map(action)
        }
        let action = roots
            .iter()
            .find_map(action)
            .expect("an accept or send action");
        let found: Vec<&str> = derived::input_parameters_with_inherited(&index, action)
            .iter()
            .map(|&p| index.name[p].as_str())
            .collect();
        for name in wanted {
            assert!(
                found.contains(&name),
                "`{body}` must inherit `{name}`, got {found:?}"
            );
        }
    }
}
