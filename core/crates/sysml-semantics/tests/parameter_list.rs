//! A declaration's parameter list belongs to the declaration.
//!
//! `ref action 'provide power' (in fuelCmd = ...) :> B` writes its parameters between the name
//! and the specialization clause. The group was not read at all, so every parameter became a
//! member of the enclosing body — `action X (in f = 1)` declared `f` on the package — and the
//! `:> B` clause, having no declaration to attach to, produced an anonymous element.

use sysml_model::Kind;

fn top(src: &str) -> Vec<(String, Kind, Vec<String>)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    loaded
        .roots
        .iter()
        .flat_map(|p| p.children.iter())
        .map(|e| (e.name.clone(), e.kind.clone(), e.supertypes.clone()))
        .collect()
}

#[test]
fn parameters_become_members_of_the_declaration() {
    let found = top(r#"
package P {
	action X (in f = 1, out g);
}
"#);
    let names: Vec<&str> = found.iter().map(|(n, _, _)| n.as_str()).collect();
    assert!(names.contains(&"X"), "the declaration, got {names:?}");
    assert!(
        !names.contains(&"f") && !names.contains(&"g"),
        "parameters are members of X, not of the package: {names:?}"
    );
    let loaded =
        sysml_syntax::load_project(&["package P {\n\taction X (in f = 1, out g);\n}\n"], true)
            .expect("model parses");
    let x = loaded
        .roots
        .iter()
        .flat_map(|p| p.children.iter())
        .find(|e| e.name == "X")
        .expect("X");
    let inner: Vec<&str> = x.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(inner, vec!["f", "g"], "both parameters are X's");
}

#[test]
fn the_specialization_clause_stays_with_the_declaration() {
    let found = top(r#"
package P {
	action target;
	ref action 'provide power' (in f = 1) :> target;
}
"#);
    let names: Vec<&str> = found.iter().map(|(n, _, _)| n.as_str()).collect();
    assert!(
        !names.iter().any(|n| n.starts_with('<')),
        "no anonymous member left over, got {names:?}"
    );
    let provide = found
        .iter()
        .find(|(n, _, _)| n == "provide power")
        .expect("the declaration");
    assert_eq!(provide.2, vec!["target".to_string()]);
}

#[test]
fn a_declaration_without_a_parameter_list_is_unchanged() {
    let found = top(r#"
package P {
	part def Wheel;
	part def Car { part w : Wheel; }
}
"#);
    let car = found.iter().find(|(n, _, _)| n == "Car").expect("Car");
    assert_eq!(car.2, Vec::<String>::new());
}
