//! The expression id space: ids, walking, and the two properties a rule asks of an expression.
//!
//! The point of the space is that expressions do **not** become members: a declaration that writes
//! `= a + b` still owns no child for the `+`. The first test pins that, because it is the property
//! the rest of the rule set depends on.

use sysml_model::Element;
use sysml_resolve::Index;
use sysml_semantics::expr::{self, ExprId, ExprKind, ExprTree};

fn load(src: &str) -> (Index, Vec<Element>, ExprTree) {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots);
    let tree = ExprTree::build(&loaded.roots);
    (index, loaded.roots, tree)
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
fn an_expression_is_not_a_child_of_the_declaration_that_wrote_it() {
    let src = r#"
package P {
	part def Wheel {
		attribute radius : Real = 2 + 3;
	}
}
"#;
    let (_index, roots, tree) = load(src);
    let radius = id_of(&roots, "radius");
    let el = roots[0].children[0].children[0].clone();
    assert_eq!(el.name, "radius");
    assert!(
        el.children.is_empty(),
        "an expression must not become a member: {:?}",
        el.children.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    assert_eq!(
        tree.roots_of(radius).len(),
        1,
        "the value expression is in the id space instead"
    );
}

#[test]
fn ids_are_dense_and_a_node_precedes_its_children() {
    let src = r#"
package P {
	attribute x : Real = a + b * c;
}
"#;
    let (_index, roots, tree) = load(src);
    let x = id_of(&roots, "x");
    let root = tree.roots_of(x)[0];
    let order = tree.preorder(root);
    // `a + (b * c)`: five nodes, the root first and every child after its parent.
    assert_eq!(order.len(), 5, "got {order:?}");
    assert_eq!(order[0], root);
    for (position, id) in order.iter().enumerate() {
        for child in &tree.node(*id).children {
            let at = order
                .iter()
                .position(|x| x == child)
                .expect("child in preorder");
            assert!(at > position, "a child must come after its parent");
        }
    }
    // Dense from zero: the ids of this model are 0..=4.
    assert_eq!(ExprId(0), order[0]);
    assert_eq!(tree.len(), 5);
}

#[test]
fn a_call_denotes_the_return_parameter_of_what_it_calls() {
    let src = r#"
package P {
	calc def Double {
		in x : Real;
		return : Real = x * 2;
	}
	attribute y : Real = Double(3);
}
"#;
    let (index, roots, tree) = load(src);
    let y = id_of(&roots, "y");
    let root = tree.roots_of(y)[0];
    assert_eq!(tree.node(root).kind, ExprKind::Call);
    let result = expr::result_of(&index, &tree, root).expect("a called calculation has a result");
    assert_eq!(index.clause_of(result), Some("return"));
}

#[test]
fn a_reference_denotes_what_it_names() {
    let src = r#"
package P {
	attribute a : Real = 1;
	attribute b : Real = a;
}
"#;
    let (index, roots, tree) = load(src);
    let b = id_of(&roots, "b");
    let result = expr::result_of(&index, &tree, tree.roots_of(b)[0]).expect("a reference names a");
    assert_eq!(index.qualified[result], "P::a");
}

#[test]
fn a_computed_value_names_no_element() {
    let src = r#"
package P {
	attribute a : Real = 1;
	attribute b : Real = a + 1;
}
"#;
    let (index, roots, tree) = load(src);
    let b = id_of(&roots, "b");
    assert_eq!(
        expr::result_of(&index, &tree, tree.roots_of(b)[0]),
        None,
        "a sum is not an element"
    );
}

#[test]
fn a_comparison_has_a_boolean_result() {
    let src = r#"
package P {
	assert constraint { 3 > 2 }
	assert constraint { 3 + 4 }
}
"#;
    let (_index, roots, tree) = load(src);
    let asserts: Vec<usize> = roots[0].children.iter().map(|c| c.id).collect();
    assert_eq!(asserts.len(), 2, "two assertions");
    let comparison = tree.roots_of(asserts[0])[0];
    let sum = tree.roots_of(asserts[1])[0];
    assert_eq!(expr::boolean_result(&_index, &tree, comparison), Some(true));
    assert_eq!(
        expr::boolean_result(&_index, &tree, sum),
        None,
        "an arithmetic result is not a Boolean by its operator"
    );
}

#[test]
fn a_reference_to_a_boolean_typed_feature_is_a_boolean() {
    let src = r#"
package P {
	attribute def Boolean;
	attribute isReady : Boolean = false;
	assert constraint { isReady }
}
"#;
    let (index, roots, tree) = load(src);
    let assert_el = id_of(&roots, "assert");
    let root = tree.roots_of(assert_el)[0];
    assert!(matches!(tree.node(root).kind, ExprKind::Ref(_)));
    assert_eq!(expr::boolean_result(&index, &tree, root), Some(true));
}

#[test]
fn a_reference_to_a_real_typed_feature_is_not_a_boolean() {
    let src = r#"
package P {
	attribute def Real;
	attribute mass : Real = 2;
	assert constraint { mass }
}
"#;
    let (index, roots, tree) = load(src);
    let assert_el = id_of(&roots, "assert");
    let root = tree.roots_of(assert_el)[0];
    assert_eq!(expr::boolean_result(&index, &tree, root), Some(false));
}

#[test]
fn a_constructor_is_not_a_boolean() {
    let src = r#"
package P {
	part def A;
	assert constraint { new A() }
}
"#;
    let (index, roots, tree) = load(src);
    let assert_el = id_of(&roots, "assert");
    let root = tree.roots_of(assert_el)[0];
    assert!(
        matches!(tree.node(root).kind, ExprKind::Constructor { .. }),
        "the value is a construction: {:?}",
        tree.node(root).kind
    );
    assert_eq!(
        expr::boolean_result(&index, &tree, root),
        Some(false),
        "a constructed part is not a predicate, and the type says so"
    );
}

#[test]
fn a_call_has_the_type_of_its_result_parameter() {
    // Before the type layer a call answered `None` here, so a condition written as a call was
    // never judged: `filter new A(null, 1, "", false)` passed while the reference implementation
    // rejects it. The signature is what settles it, and the signature is in the model.
    let src = r#"
package P {
	attribute def Boolean;
	attribute def Real;
	calc def IsReady { return : Boolean = true; }
	calc def Weight { return : Real = 1; }
	assert constraint { IsReady() }
	assert constraint { Weight() }
}
"#;
    let (index, roots, tree) = load(src);
    let asserts: Vec<usize> = roots[0]
        .children
        .iter()
        .filter(|c| c.name == "assert")
        .map(|c| c.id)
        .collect();
    assert_eq!(asserts.len(), 2, "two assertions");
    let boolean_call = tree.roots_of(asserts[0])[0];
    let real_call = tree.roots_of(asserts[1])[0];
    assert!(matches!(tree.node(boolean_call).kind, ExprKind::Call));
    assert_eq!(
        expr::boolean_result(&index, &tree, boolean_call),
        Some(true),
        "a function returning a Boolean is a predicate"
    );
    assert_eq!(
        expr::boolean_result(&index, &tree, real_call),
        Some(false),
        "a function returning a Real is not"
    );
}

#[test]
fn a_braced_chain_keeps_the_name_it_binds() {
    // `items->forAll { in w; w > 0 }` lowers as a chain whose braced body binds `w`; the binding
    // names live on the chain node, which is where a rule that must not report `w` as an
    // unresolved model name reads them.
    let src = r#"
package P {
	attribute all : Boolean = items->forAll { in w; w > 0 };
}
"#;
    let (_index, roots, tree) = load(src);
    let all = id_of(&roots, "all");
    let root = tree.roots_of(all)[0];
    let kinds: Vec<&ExprKind> = tree
        .preorder(root)
        .iter()
        .map(|id| &tree.node(*id).kind)
        .collect();
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, ExprKind::Chain { op, params } if op == "forAll" && params == &vec!["w".to_string()])),
        "the chain keeps the name its body binds: {kinds:?}"
    );
}
