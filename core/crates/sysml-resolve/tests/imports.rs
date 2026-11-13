//! Import binding: what `import` pulls into a scope, how far it reaches, and where the
//! bundled standard library fits.
//!
//! The visibility question is settled by KerML 8.2.3.3, not by taste: a private import is
//! visible inside the namespace that declares it and inside everything nested in it. The
//! reference implementation reads it the other way (semantic.py:433-435 hides private imports
//! from child scopes), which would leave the examples' unit typings unresolved; the tests
//! below pin the spec reading.

use sysml_model::ImportShape;
use sysml_resolve::{Index, Resolution};
use sysml_syntax::load_project;

struct Project {
    roots: Vec<sysml_model::Element>,
    library: Vec<sysml_model::Element>,
}

fn project(sources: &[&str], stdlib: bool) -> Project {
    let loaded = load_project(sources, stdlib).expect("parses");
    Project {
        roots: loaded.roots,
        library: loaded.library,
    }
}

fn index(p: &Project) -> Index {
    Index::build(&p.roots).with_library(&p.library)
}

fn id_of(index: &Index, qualified: &str) -> usize {
    index
        .qualified
        .iter()
        .position(|q| q == qualified)
        .unwrap_or_else(|| panic!("no element {qualified}"))
}

/// Resolve the `type_ref` written on the element with this qualified name.
fn typing_of(index: &Index, roots: &[sysml_model::Element], qualified: &str) -> Resolution {
    let id = id_of(index, qualified);
    fn find(el: &sysml_model::Element, id: usize) -> Option<&sysml_model::Element> {
        if el.id == id {
            return Some(el);
        }
        el.children.iter().find_map(|c| find(c, id))
    }
    let el = roots
        .iter()
        .find_map(|r| find(r, id))
        .unwrap_or_else(|| panic!("no element {qualified} in the tree"));
    let text = el
        .type_ref
        .clone()
        .unwrap_or_else(|| panic!("{qualified} has no type_ref"));
    index.resolve(el.id, &text)
}

#[test]
fn a_membership_import_binds_the_simple_name() {
    let p = project(
        &[r#"
package ScalarValues {
    attribute def Real;
}
package P {
    private import ScalarValues::Real;
    part def W {
        attribute v : Real;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::v"),
        Resolution::Found(id_of(&index, "ScalarValues::Real"))
    );
}

#[test]
fn a_namespace_import_binds_every_member() {
    let p = project(
        &[r#"
package Q {
    attribute def A;
    attribute def B;
}
package P {
    private import Q::*;
    part def W {
        attribute x : A;
        attribute y : B;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::x"),
        Resolution::Found(id_of(&index, "Q::A"))
    );
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::y"),
        Resolution::Found(id_of(&index, "Q::B"))
    );
}

#[test]
fn a_private_import_reaches_nested_scopes() {
    // KerML 8.2.3.3: private is visible inside the declaring namespace and everything nested
    // in it. The import sits on the package; the reference is two scopes deeper.
    let p = project(
        &[r#"
package Q {
    attribute def T;
}
package P {
    private import Q::*;
    part def W {
        part inner : W2 {
            attribute m : T;
        }
    }
    part def W2;
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::inner::m"),
        Resolution::Found(id_of(&index, "Q::T"))
    );
}

#[test]
fn a_private_import_does_not_leak_to_a_sibling_package() {
    let p = project(
        &[r#"
package Q {
    attribute def T;
}
package P {
    private import Q::*;
}
package R {
    part def W {
        attribute m : T;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "R::W::m"),
        Resolution::Unresolved
    );
}

#[test]
fn a_public_import_reexports_to_an_importing_package() {
    let p = project(
        &[r#"
package Q {
    attribute def T;
}
package P {
    public import Q::*;
}
package R {
    private import P::*;
    part def W {
        attribute m : T;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "R::W::m"),
        Resolution::Found(id_of(&index, "Q::T")),
        "P publicly imported Q, so importing P must surface Q::T"
    );
}

#[test]
fn a_private_import_is_not_reexported() {
    let p = project(
        &[r#"
package Q {
    attribute def T;
}
package P {
    private import Q::*;
}
package R {
    private import P::*;
    part def W {
        attribute m : T;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "R::W::m"),
        Resolution::Unresolved
    );
}

#[test]
fn a_local_member_shadows_an_imported_name() {
    let p = project(
        &[r#"
package Q {
    attribute def T;
}
package P {
    private import Q::*;
    part def T;
    part def W {
        attribute m : T;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::m"),
        Resolution::Found(id_of(&index, "P::T"))
    );
}

#[test]
fn an_inner_import_shadows_an_outer_one() {
    let p = project(
        &[r#"
package Q1 {
    attribute def T;
}
package Q2 {
    attribute def T;
}
package Outer {
    private import Q1::*;
    part def Inner {
        private import Q2::*;
        attribute m : T;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "Outer::Inner::m"),
        Resolution::Found(id_of(&index, "Q2::T"))
    );
}

#[test]
fn a_later_import_of_the_same_name_wins() {
    // The reference implementation's dict assignment does the same: last write wins
    // (semantic.py:_resolve_namespace_import). Reported as found rather than ambiguous,
    // matching that.
    let p = project(
        &[r#"
package Q1 {
    attribute def T;
}
package Q2 {
    attribute def T;
}
package P {
    private import Q1::*;
    private import Q2::*;
    part def W {
        attribute m : T;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::m"),
        Resolution::Found(id_of(&index, "Q2::T"))
    );
}

#[test]
fn a_recursive_namespace_import_binds_nested_members() {
    let p = project(
        &[r#"
package Q {
    attribute def Top;
    part def Sub {
        attribute def Deep;
    }
}
package P {
    private import Q::*::**;
    part def W {
        attribute a : Top;
        attribute b : Deep;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::a"),
        Resolution::Found(id_of(&index, "Q::Top"))
    );
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::b"),
        Resolution::Found(id_of(&index, "Q::Sub::Deep"))
    );
}

#[test]
fn a_bare_double_star_import_is_recursive() {
    // `import Q::**` — the recursive form without the leading `*`. The lexer folds `**` into one
    // token because it is also the exponent operator, and the import parser tested only for `*`,
    // so `Q::**` fell through to the segment loop and bound `Q` itself: `Top` and `Deep` were
    // unreachable. `Q::*::**` worked, which is why the tests above never saw it.
    let p = project(
        &[r#"
package Q {
    attribute def Top;
    part def Sub {
        attribute def Deep;
    }
}
package P {
    private import Q::**;
    part def W {
        attribute a : Top;
        attribute b : Deep;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::a"),
        Resolution::Found(id_of(&index, "Q::Top"))
    );
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::b"),
        Resolution::Found(id_of(&index, "Q::Sub::Deep"))
    );
}

#[test]
fn a_non_recursive_namespace_import_does_not_bind_nested_members() {
    let p = project(
        &[r#"
package Q {
    part def Sub {
        attribute def Deep;
    }
}
package P {
    private import Q::*;
    part def W {
        attribute b : Deep;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::b"),
        Resolution::Unresolved
    );
}

#[test]
fn the_bundled_library_binds_through_an_import() {
    // `MassValue` lives in ISQBase and reaches ISQ only through ISQ's own
    // `public import ISQBase::*`, which is the re-export path.
    let p = project(
        &[r#"
package SurveyDrone {
    private import ISQ::*;
    part def Battery {
        attribute mass : MassValue;
    }
}
"#],
        true,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "SurveyDrone::Battery::mass"),
        Resolution::Found(id_of(&index, "ISQBase::MassValue"))
    );
}

#[test]
fn a_library_name_without_an_import_stays_unresolved() {
    let p = project(
        &[r#"
package P {
    part def W {
        attribute mass : MassValue;
    }
}
"#],
        true,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::mass"),
        Resolution::Unresolved,
        "the library is reachable through imports only, never through a global fallback"
    );
}

#[test]
fn the_library_is_reachable_through_a_membership_import() {
    let p = project(
        &[r#"
package P {
    private import ScalarValues::Real;
    part def W {
        attribute v : Real;
    }
}
"#],
        true,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::v"),
        Resolution::Found(id_of(&index, "ScalarValues::Real"))
    );
}

#[test]
fn a_project_package_shadows_the_bundled_library() {
    let p = project(
        &[r#"
package ISQBase {
    attribute def MassValue;
}
package P {
    private import ISQ::*;
    part def W {
        attribute mass : MassValue;
    }
}
"#],
        true,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::mass"),
        Resolution::Found(id_of(&index, "ISQBase::MassValue"))
    );
}

#[test]
fn a_reexport_reaches_an_earlier_file() {
    // The importing file comes first in the project and the re-exporting package second, so
    // the bindings only line up on a later pass of `bind_imports`.
    let p = project(
        &[
            r#"
package R {
    private import P::*;
    part def W {
        attribute m : T;
    }
}
"#,
            r#"
package P {
    public import Q::*;
}
package Q {
    attribute def T;
}
"#,
        ],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "R::W::m"),
        Resolution::Found(id_of(&index, "Q::T"))
    );
}

#[test]
fn imports_cross_files() {
    let p = project(
        &[
            r#"
package A {
    part def Thing;
}
"#,
            r#"
package B {
    private import A::*;
    part x : Thing;
}
"#,
        ],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "B::x"),
        Resolution::Found(id_of(&index, "A::Thing"))
    );
}

#[test]
fn a_filter_import_binds_nothing() {
    let p = project(
        &[r#"
package Q {
    attribute def T;
}
package P {
    private import Q [ 1 ];
    part def W {
        attribute m : T;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::m"),
        Resolution::Unresolved
    );
    let entry = index
        .imports()
        .into_iter()
        .find(|e| e.element == id_of(&index, "P::Q"))
        .expect("the filter import is indexed");
    assert_eq!(entry.shape, ImportShape::Filter);
    assert!(entry.bound.is_empty());
}

#[test]
fn an_unresolved_import_target_is_reported() {
    let p = project(
        &[r#"
package P {
    private import NoSuchPackage::*;
}
"#],
        true,
    );
    let index = index(&p);
    let entry = index
        .imports()
        .into_iter()
        .find(|e| e.element == id_of(&index, "P::NoSuchPackage"))
        .expect("the import is indexed");
    assert_eq!(entry.target, Resolution::Unresolved);
    assert!(entry.bound.is_empty());
}

#[test]
fn an_import_written_in_a_definition_body_scopes_to_that_definition() {
    let p = project(
        &[r#"
package Q {
    attribute def T;
}
package P {
    part def W {
        private import Q::*;
        attribute m : T;
    }
    part def Other {
        attribute n : T;
    }
}
"#],
        false,
    );
    let index = index(&p);
    assert_eq!(
        typing_of(&index, &p.roots, "P::W::m"),
        Resolution::Found(id_of(&index, "Q::T"))
    );
    assert_eq!(
        typing_of(&index, &p.roots, "P::Other::n"),
        Resolution::Unresolved,
        "W's import must not reach W's sibling"
    );
}

#[test]
fn import_bindings_are_listed_for_the_report() {
    let p = project(
        &[r#"
package P {
    private import ISQ::*;
}
"#],
        true,
    );
    let index = index(&p);
    let entry = index
        .imports()
        .into_iter()
        .find(|e| e.element == id_of(&index, "P::ISQ"))
        .expect("the import is indexed");
    assert_eq!(entry.shape, ImportShape::Namespace { recursive: false });
    assert_eq!(entry.owner, Some(id_of(&index, "P")));
    assert_eq!(entry.target, Resolution::Found(id_of(&index, "ISQ")));
    let bound: Vec<&str> = entry.bound.iter().map(|(n, _)| n.as_str()).collect();
    // The library grows as later layers need more quantity types, so this asserts what a
    // namespace import must do -- bind the target's own members plus its public re-exports --
    // rather than pinning the whole list, which would fail on every addition while telling
    // nobody anything.
    for expected in [
        "TemperatureDifferenceValue",
        "LengthValue",
        "MassValue",
        "PowerValue",
        "EnergyValue",
    ] {
        assert!(
            bound.contains(&expected),
            "a namespace import should bind {expected}, got {bound:?}"
        );
    }
}
