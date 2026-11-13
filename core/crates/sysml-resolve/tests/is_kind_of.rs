//! `Index::is_kind_of` — the `oclIsKindOf` a rule written against the metamodel asks for.

use sysml_model::Kind;
use sysml_resolve::Index;

fn index(src: &str) -> (Index, Vec<sysml_model::Element>) {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    (Index::build(&loaded.roots), loaded.roots)
}

fn id_of(roots: &[sysml_model::Element], name: &str) -> usize {
    fn go(el: &sysml_model::Element, name: &str) -> Option<usize> {
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

const MODEL: &str = r#"
package P {
	part def Wheel;
	part def Vehicle {
		part wheel : Wheel;
		attribute mass : Real;
		port p : Wheel;
	}
	action def Move {
		in distance : Real;
	}
	class C :> Structure;
}
"#;

#[test]
fn a_usage_is_a_feature_a_type_and_a_namespace() {
    let (index, roots) = index(MODEL);
    let wheel = id_of(&roots, "wheel");
    for metaclass in [
        "PartUsage",
        "ItemUsage",
        "OccurrenceUsage",
        "Usage",
        "Feature",
        "Type",
        "Namespace",
        "Element",
    ] {
        assert!(index.is_kind_of(wheel, metaclass), "wheel is a {metaclass}");
    }
    for metaclass in ["Definition", "Behavior", "Class", "DataType"] {
        assert!(
            !index.is_kind_of(wheel, metaclass),
            "wheel is not a {metaclass}"
        );
    }
}

#[test]
fn a_definition_is_not_a_usage_and_an_action_is_a_behavior() {
    let (index, roots) = index(MODEL);
    let vehicle = id_of(&roots, "Vehicle");
    assert!(index.is_kind_of(vehicle, "PartDefinition"));
    assert!(index.is_kind_of(vehicle, "Definition"));
    assert!(!index.is_kind_of(vehicle, "Usage"));
    assert!(!index.is_kind_of(vehicle, "Behavior"));

    let move_def = id_of(&roots, "Move");
    assert!(index.is_kind_of(move_def, "ActionDefinition"));
    assert!(index.is_kind_of(move_def, "Behavior"));
    assert!(!index.is_kind_of(move_def, "Structure"));
}

#[test]
fn a_parameter_is_a_feature_and_not_a_part() {
    let (index, roots) = index(MODEL);
    let distance = id_of(&roots, "distance");
    assert!(index.is_kind_of(distance, "ReferenceUsage"));
    assert!(index.is_kind_of(distance, "Feature"));
    assert!(!index.is_kind_of(distance, "PartUsage"));
}

#[test]
fn a_kerl_class_is_a_class_and_not_a_structure() {
    // `Structure` and `Behavior` are both `Class`es in KerML, and a bare `class` is neither.
    let (index, roots) = index(MODEL);
    let c = id_of(&roots, "C");
    assert!(index.is_kind_of(c, "Class"));
    assert!(index.is_kind_of(c, "Classifier"));
    assert!(!index.is_kind_of(c, "Structure"));
    assert!(!index.is_kind_of(c, "Behavior"));
    assert!(!index.is_kind_of(c, "DataType"));
}

#[test]
fn an_unknown_metaclass_is_false_for_every_element() {
    // A typo in a rule must not pass every model silently.
    let (index, roots) = index(MODEL);
    for name in ["Bogus", "", "partusage"] {
        for root in &roots {
            assert!(
                !index.is_kind_of(root.id, name),
                "`{name}` matched {root:?}"
            );
        }
    }
    // An id past the end of the index is not an element either.
    assert!(!index.is_kind_of(usize::MAX, "Element"));
}

#[test]
fn a_kind_that_stands_for_no_metaclass_matches_nothing() {
    // `Kind::Variant` is the projection's own marker: a bare `variant v;` writes no kind
    // keyword, so there is no metaclass question to answer for it.
    assert!(Kind::Variant.metaclasses().is_empty());
    assert!(!Kind::Variant.is_metaclass("Element"));
}

/// The qualified names of the element at `id` and everything it specializes, transitively.
///
/// `with_library` is what makes the answer meaningful: `load_project` parses the library but the
/// index only knows the types it was handed, and without the library every base name is absent
/// (`Base:: names = 0`, measured) so the closure of a `datatype` is empty and the assertion below
/// would pass or fail for the wrong reason.
fn specialization_closure(src: &str, name: &str) -> (Vec<String>, Vec<String>) {
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots).with_library(&loaded.library);
    let id = index
        .name
        .iter()
        .position(|n| n == name)
        .unwrap_or_else(|| panic!("no element named `{name}`"));
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut stack = vec![id];
    while let Some(next) = stack.pop() {
        for parent in index.supertypes_of_public(next) {
            if seen.insert(parent) {
                out.push(index.qualified[parent].clone());
                stack.push(parent);
            }
        }
    }
    (out, index.qualified.clone())
}

#[test]
fn a_kerml_classifier_specializes_its_library_base() {
    // A KerML model writes `datatype D;` and says nothing about what `D` specializes — the
    // notation decides, exactly as it does for `part p;`. The reference implementation keeps the
    // mapping as a table of its own
    // (`research/upstream/sem/sysmlv2-semantics-0.1.2/src/lib.rs:1526-1536`), and without it the
    // whole KerML half of the validation suite is silent where it must warn: a `datatype`
    // specializing a `class` inherits `self` from `DataValue` and from `Occurrence`, which is what
    // `Specialization_invalid.kerml.xt:39` states as "from DataValue, Occurrence".
    let cases = [
        ("datatype D;", "D", "Base::DataValue"),
        ("class C;", "C", "Occurrences::Occurrence"),
        ("struct S;", "S", "Objects::Object"),
        ("behavior B;", "B", "Performances::Performance"),
        ("function F;", "F", "Performances::Evaluation"),
    ];
    for (decl, name, base) in cases {
        let src = format!("package P {{\n\t{decl}\n}}");
        let (closure, _) = specialization_closure(&src, name);
        assert!(
            closure.iter().any(|q| q == base),
            "`{name}` must specialize `{base}`, got {closure:?}"
        );
    }
}

#[test]
fn an_association_does_not_specialize_a_binary_link() {
    // The reference maps `Association => Links::BinaryLink, Links::Link`, but `BinaryLink`
    // declares the ends `source` and `target`, so adding the base here made every `Interface` in
    // the suite inherit two ends it never wrote and the interface-end rules fired on all of them
    // (measured: thirty diagnostics across `Interface %Example` and its six siblings, spurious 11 → 41).
    // The KerML association tests state no `self` ambiguity either, so the base buys nothing and
    // costs thirty: the mapping is deliberately absent.
    let (closure, _) = specialization_closure("package P {\n\tassoc A;\n}", "A");
    assert!(
        !closure.iter().any(|q| q.contains("BinaryLink")),
        "an association must not inherit BinaryLink's two ends, got {closure:?}"
    );
}
