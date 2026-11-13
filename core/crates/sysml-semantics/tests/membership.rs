//! The membership axis: which membership introduced a member, filled from what the notation wrote.
//!
//! The point of the field is that the metamodel states member constraints *through* the membership,
//! and the projection used to keep that fact in three unrelated places. These tests pin the fill,
//! including the one distinction that is easy to get wrong: a directed feature is a **parameter by
//! its direction**, not a member of a `ParameterMembership`.

use sysml_model::{Element, Membership};
use sysml_resolve::Index;

fn load(src: &str) -> (Index, Vec<Element>) {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    (Index::build(&loaded.roots), loaded.roots)
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

#[test]
fn a_directed_feature_is_a_feature_member_not_a_parameter_membership() {
    // `Step::parameter redefines directedFeature`: `in x` is a parameter because of its direction,
    // and it is owned through an ordinary `FeatureMembership`. The `ParameterMembership`s the
    // notation writes are message payloads and call arguments, which this projection does not
    // build, so mapping a direction to one would let a rule judge a membership nobody wrote.
    let (index, roots) = load(
        r#"
package P {
	action def Move {
		in distance : Real;
		out verdict : Boolean;
	}
}
"#,
    );
    let distance = id_of(&roots, "distance");
    let verdict = id_of(&roots, "verdict");
    assert_eq!(index.membership_of(distance), Membership::Feature);
    assert_eq!(index.membership_of(verdict), Membership::Feature);
}

#[test]
fn a_return_clause_is_a_return_parameter_membership() {
    let (index, roots) = load(
        r#"
package P {
	calc def Double {
		in x : Real;
		return : Real = x * 2;
	}
}
"#,
    );
    let ret = id_of(&roots, "return");
    assert_eq!(index.membership_of(ret), Membership::ReturnParameter);
}

#[test]
fn a_require_clause_is_a_requirement_constraint_membership() {
    let (index, roots) = load(
        r#"
package P {
	requirement def R {
		require constraint { true }
		assume constraint { true }
	}
}
"#,
    );
    let constraints: Vec<Membership> = roots[0].children[0]
        .children
        .iter()
        .map(|c| c.membership)
        .collect();
    assert!(
        constraints
            .iter()
            .all(|m| *m == Membership::RequirementConstraint),
        "both `require` and `assume` are that membership: {constraints:?}"
    );
}

#[test]
fn an_end_is_an_end_feature_membership() {
    let (index, roots) = load(
        r#"
package P {
	part def Wheel;
	part def Cart {
		end wheel : Wheel;
	}
}
"#,
    );
    let wheel = id_of(&roots, "wheel");
    assert_eq!(index.membership_of(wheel), Membership::EndFeature);
}

#[test]
fn an_objective_and_a_frame_are_their_own_memberships() {
    let (index, roots) = load(
        r#"
package P {
	concern def Safety;
	requirement def R {
		frame k : Safety;
	}
	requirement def WithObjective {
		objective { require constraint { true } }
	}
}
"#,
    );
    let frame = id_of(&roots, "k");
    let objective = id_of(&roots, "objective");
    assert_eq!(index.membership_of(frame), Membership::FramedConcern);
    assert_eq!(index.membership_of(objective), Membership::Objective);
}

#[test]
fn an_ordinary_member_is_a_feature_membership() {
    let (index, roots) = load(
        r#"
package P {
	part def Wheel;
	part def Cart {
		part wheel : Wheel;
	}
}
"#,
    );
    let wheel = id_of(&roots, "wheel");
    assert_eq!(index.membership_of(wheel), Membership::Feature);
}
