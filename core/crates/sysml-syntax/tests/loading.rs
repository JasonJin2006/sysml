//! Front end behaviour that the resolver and the views both lean on: what gets lowered, and
//! what the bundled standard library contains.

use sysml_model::{ImportShape, Kind, Visibility};
use sysml_syntax::load_project;

/// Serialises the tests that set `SYSML_LIBRARY`.
///
/// The environment is process-wide and `cargo test` runs a file's tests on several threads, so a
/// test that sets the variable, loads, then removes it can have its window straddled by another
/// test doing the same. The symptom is not a wrong assertion but a flaky one: one run reported
/// `902 passed, 1 failed` where the same code reported `937 passed, 0 failed` a minute later.
/// Holding this lock across set-load-restore removes the interleaving.
///
/// A poisoned lock is recovered rather than propagated: one panicking test must not turn every
/// later test into a failure with an unrelated message.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn load(src: &str) -> sysml_syntax::Loaded {
    load_project(&[src], false).expect("parses")
}
#[test]
fn association_structures_keep_their_kind_specializations_and_body() {
    let loaded = load(
        "package Objects { abstract assoc struct LinkObject specializes Link, Object { \
         end participant : Object; } struct Following; }",
    );
    let members = &loaded.roots[0].children;
    assert_eq!(members.len(), 2);
    let link = &members[0];
    assert_eq!(link.name, "LinkObject");
    assert_eq!(link.kind, Kind::AssocStructureDef);
    assert!(link.modifiers.is_abstract);
    assert_eq!(link.specializes, ["Link", "Object"]);
    assert!(link.type_refs.is_empty());
    assert_eq!(link.children.len(), 1);
    assert_eq!(link.children[0].kind, Kind::KermlFeature);
    assert!(link.children[0].is_end);
    assert_eq!(members[1].name, "Following");
}

#[test]
fn bare_end_kinds_follow_the_owner_without_rewriting_explicit_kinds() {
    let loaded = load(
        "package P { connection def C { end bare : T; end port explicitPort : T; } \
         assoc A { end bare : T; } \
         interface def I { end bare : T; end part explicitPart : T; end feature explicitFeature : T; } \
         interface usage { end bare : T; } }",
    );
    let members = &loaded.roots[0].children;
    let member = |name: &str| members.iter().find(|el| el.name == name).unwrap();
    // A connection's end is a `Usage` — `ConnectionDefinition::connectionEnd` is declared `Usage`
    // and the library writes `end source: Anything` for every connection below `Connections` — and
    // an association's end is a KerML `Feature`. That difference is what
    // `connection def B :> Links::BinaryLink` turns on: the ends it inherits are features, so the
    // property check rejects them.
    assert_eq!(member("C").children[0].kind, Kind::OccurrenceUsage);
    assert_eq!(member("C").children[1].kind, Kind::PortUsage);
    assert_eq!(member("A").children[0].kind, Kind::KermlFeature);
    assert_eq!(member("I").children[0].kind, Kind::PortUsage);
    assert_eq!(member("I").children[1].kind, Kind::PartUsage);
    assert_eq!(member("I").children[2].kind, Kind::KermlFeature);
    assert_eq!(member("usage").children[0].kind, Kind::PortUsage);
    for owner in members {
        for end in &owner.children {
            assert!(end.is_end);
            assert_eq!(end.membership, sysml_model::Membership::EndFeature);
            assert!(!end.is_composite);
        }
    }
}

#[test]
fn imports_lower_with_their_shape_and_visibility() {
    // The visibility indicator is mandatory in the textual notation (OMG SYSML2_-207, resolved
    // in SysML 2.0b4); a bare `import X;` is a parse error, not a default.
    let loaded = load(
        r#"
package P {
    private import ISQ::*;
    private import ScalarValues::Real;
    public import A::*::**;
    protected import B [ 1 ] [ 2 + 3 ];
}
"#,
    );
    let imports: Vec<&sysml_model::Element> = loaded.roots[0]
        .children
        .iter()
        .filter(|c| c.kind == Kind::Import)
        .collect();
    assert_eq!(imports.len(), 4);

    assert_eq!(imports[0].name, "ISQ");
    assert_eq!(imports[0].type_ref.as_deref(), Some("ISQ"));
    assert_eq!(
        imports[0].import_shape,
        Some(ImportShape::Namespace { recursive: false })
    );
    assert_eq!(imports[0].visibility, Some(Visibility::Private));

    // The element's name is the authored target path, suffix excluded; the resolver takes the
    // last segment as the bound simple name.
    assert_eq!(imports[1].name, "ScalarValues::Real");
    assert_eq!(imports[1].type_ref.as_deref(), Some("ScalarValues::Real"));
    assert_eq!(
        imports[1].import_shape,
        Some(ImportShape::Membership { recursive: false })
    );
    assert_eq!(imports[1].visibility, Some(Visibility::Private));

    assert_eq!(
        imports[2].import_shape,
        Some(ImportShape::Namespace { recursive: true })
    );
    assert_eq!(imports[2].visibility, Some(Visibility::Public));

    // The filter form is `A [ expr ] [ expr ]`, not `A::[members]` (parser validation test
    // `test_import_filter_package`: "KerML: import with FilterPackage form
    // `QualifiedName [ expr ] [ expr ]+ ;`"). The filter expressions themselves are dropped;
    // only the target survives.
    assert_eq!(imports[3].name, "B");
    assert_eq!(imports[3].type_ref.as_deref(), Some("B"));
    assert_eq!(imports[3].import_shape, Some(ImportShape::Filter));
    assert_eq!(imports[3].visibility, Some(Visibility::Protected));
}

#[test]
fn imports_inside_definitions_are_children_of_that_scope() {
    let loaded = load(
        r#"
package P {
    part def W {
        private import ISQ::*;
        attribute mass : MassValue;
    }
}
"#,
    );
    let w = loaded.roots[0]
        .children
        .iter()
        .find(|c| c.name == "W")
        .expect("part def W");
    assert!(w
        .children
        .iter()
        .any(|c| c.kind == Kind::Import && c.name == "ISQ"));
}

#[test]
fn the_standard_library_stays_out_of_roots() {
    // Whatever library was found — the bundled stub, or the real one when it is installed —
    // it must never become part of the project. `roots` is what every view draws and what the
    // ledger audits, so a library element appearing there would put `Real` and `ScalarValue`
    // boxes in every diagram and demand a decision record for each.
    let loaded = load_project(&["package User { part x; }"], true).expect("parses");
    assert_eq!(
        loaded.roots.len(),
        1,
        "library elements must not become roots"
    );
    assert!(!loaded.library.is_empty(), "a library is attached");

    // Ids continue after the project's so one index can hold both trees.
    let project_size: usize = loaded.roots.iter().map(sysml_model::Element::size).sum();
    assert_eq!(loaded.library[0].id, project_size);
    assert_eq!(loaded.element_count, project_size);

    // The names a model actually imports have to be there. `ScalarValues` and `Real` are the
    // two the official examples rely on in nearly every file.
    let names: Vec<&str> = loaded.library.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"ScalarValues"),
        "library must define ScalarValues, got {names:?}"
    );
    let scalars = loaded
        .library
        .iter()
        .find(|e| e.name == "ScalarValues")
        .expect("ScalarValues package");
    let members: Vec<&str> = scalars.children.iter().map(|c| c.name.as_str()).collect();
    assert!(
        members.contains(&"Real"),
        "ScalarValues must define Real, got {members:?}"
    );
}

#[test]
fn the_bundled_stdlib_is_used_when_no_library_is_installed() {
    // The fallback has to keep working on its own: it is what a copied binary or a sandbox
    // without the library on disk runs against, and a silently empty library would make every
    // type reference in every model unresolved.
    let _env = env_lock();
    std::env::set_var("SYSML_LIBRARY", "off");
    let loaded = load_project(&["package User { part x; }"], true).expect("parses");
    // Restored before any assertion can fail, so one failure cannot leak into other tests.
    std::env::remove_var("SYSML_LIBRARY");
    let names: Vec<&str> = loaded.library.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "ScalarValues",
            "ISQBase",
            "ISQMechanics",
            "ISQThermodynamics",
            "ISQ"
        ],
        "the bundled stub defines exactly these packages"
    );
}

#[test]
fn several_files_share_one_tree() {
    let loaded = load_project(
        &[
            "package A { part def Thing; }",
            "package B { part x : A::Thing; }",
        ],
        false,
    )
    .expect("parses");
    assert_eq!(loaded.roots.len(), 2);
    assert_eq!(loaded.roots[1].name, "B");
}

/// The load has to say which library it read.
///
/// Before this, a run against the 68-line stub and a run against the 28612-line library were
/// indistinguishable in every report: both resolved `MassValue` to *something*, both said the
/// model was clean. The difference is the quantity and unit chain, which only the full library
/// carries, so a "clean" verdict against the stub is a weaker claim than it looks.
#[test]
fn the_load_names_the_library_it_used() {
    let _env = env_lock();
    std::env::set_var("SYSML_LIBRARY", "off");
    let stubbed = load_project(&["package User { part x; }"], true).expect("parses");
    std::env::remove_var("SYSML_LIBRARY");
    assert!(
        stubbed.library_source.is_stub(),
        "the bundled stub must report itself: {:?}",
        stubbed.library_source
    );
    let note = stubbed.library_source.describe();
    assert!(
        note.contains("stub"),
        "the note names the case rather than leaving it to be inferred: {note}"
    );

    // And the real library, when one is installed, must not be described as a stub.
    if let Some(dir) = sysml_syntax::library::discover(None) {
        if let Ok(lib) = sysml_syntax::library::LibrarySources::read(&dir) {
            std::env::set_var("SYSML_LIBRARY", &dir);
            let full = load_project(&["package User { part x; }"], true).expect("parses");
            std::env::remove_var("SYSML_LIBRARY");
            assert!(
                !full.library_source.is_stub(),
                "a full library is not a stub: {:?}",
                full.library_source
            );
            assert!(
                full.library.len() > stubbed.library.len(),
                "the full library carries more than the stub: {} vs {}",
                full.library.len(),
                stubbed.library.len()
            );
            let _ = lib;
        }
    }
}

/// A clone has to get the full library, not the stub.
///
/// The library used to live only under `research/upstream/`, which is gitignored, so a fresh
/// clone silently ran against the 68-line stub: `MassValue` still resolved, `check` still said
/// clean, and the missing unit chain showed up only as unexplained `inconclusive` verdicts. The
/// vendored copy in `core/stdlib` is what closes that hole, so its presence is asserted here
/// rather than left to whoever remembers why the directory exists.
#[test]
fn the_repository_ships_a_usable_standard_library() {
    let vendored = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("stdlib"))
        .expect("a crate directory has a parent");
    assert!(
        vendored.is_dir(),
        "{} is missing; a clone would fall back to the bundled stub",
        vendored.display()
    );
    let lib =
        sysml_syntax::library::LibrarySources::read(&vendored).expect("the vendored library reads");
    assert!(
        lib.files.len() > 50,
        "the stub has one file; a real library has dozens: {}",
        lib.files.len()
    );

    // And the discovery order must prefer it over anything else on this machine.
    let _env = env_lock();
    std::env::remove_var("SYSML_LIBRARY");
    let found = sysml_syntax::library::discover(None).expect("a library is discoverable");
    let found = found.canonicalize().unwrap_or(found);
    let want = vendored.canonicalize().unwrap_or(vendored.clone());
    assert_eq!(
        found, want,
        "discover must return the vendored copy, not a research checkout"
    );
}

/// The quantity chain is the difference between the stub and the real library, and it is what
/// unit-bearing constraints need. A stub `MassValue` has no supertype; the real one is a
/// `ScalarQuantityValue`.
#[test]
fn the_vendored_library_carries_the_quantity_chain() {
    let _env = env_lock();
    std::env::set_var("SYSML_LIBRARY", "off");
    let stubbed = load_project(&["package U { part x; }"], true).expect("parses");
    std::env::remove_var("SYSML_LIBRARY");
    let full = load_project(&["package U { part x; }"], true).expect("parses");
    assert!(
        full.library.len() > stubbed.library.len() * 10,
        "the real library is far larger than the stub: {} vs {}",
        full.library.len(),
        stubbed.library.len()
    );
    assert!(!full.library_source.is_stub(), "{:?}", full.library_source);
}
