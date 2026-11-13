//! `validateConnectionDefinitionIsSufficient`, and the two metamodel defaults around it.
//!
//! The rule's OCL is the bare expression `isSufficient`, and the specification's metaclass redefines
//! the flag to a constant `true` for `ConnectionDefinition`. So the constraint is satisfied by the
//! *metaclass*, not by the notation: a model writes `connection def C;` and never writes `all`, and
//! the flag is still true. The kernel fills it in `derive_modifiers` (`lower.rs`).
//!
//! That makes this rule the mirror image of most others here: it can only fail if the fill is missing.
//! The tests below therefore check the fill rather than trying to construct a violating model — there
//! is no notation that violates it, which is exactly why it was filed "not applicable" before.
//!
//! # The enumeration default, which is the same shape and did not work
//!
//! `EnumerationDefinition::isVariation` is redefined to `true` the same way, and filling it looked like
//! the identical change. It broke seven models: `variation_members` reads `is_variation` to mean
//! "every member must be a `Variant`", so setting it on every enumeration definition made every
//! ordinary literal a violation. The last test here pins that regression, because the tempting
//! "symmetry" fix is to fill it too.

use sysml_model::Element;
use sysml_semantics::{check, Severity};

fn load(source: &str) -> (Vec<Element>, Vec<Element>) {
    let loaded = sysml_syntax::load_project(&[source], true).expect("the model parses");
    (loaded.roots, loaded.library)
}

fn id_of(roots: &[Element], name: &str) -> Option<usize> {
    fn go(el: &Element, name: &str) -> Option<usize> {
        if el.name == name {
            return Some(el.id);
        }
        el.children.iter().find_map(|child| go(child, name))
    }
    roots.iter().find_map(|root| go(root, name))
}

fn modifiers(source: &str, name: &str) -> sysml_model::Modifiers {
    let (roots, _library) = load(source);
    fn go(el: &Element, name: &str) -> Option<sysml_model::Modifiers> {
        if el.name == name {
            return Some(el.modifiers.clone());
        }
        el.children.iter().find_map(|child| go(child, name))
    }
    roots
        .iter()
        .find_map(|root| go(root, name))
        .unwrap_or_else(|| panic!("no element named `{name}`"))
}

fn errors(source: &str) -> Vec<String> {
    let (roots, library) = load(source);
    check(&roots, &library)
        .issues
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| format!("{} {}", issue.code, issue.message))
        .collect()
}

#[test]
fn a_connection_definition_is_sufficient_without_writing_all() {
    // The notation has no way to say "sufficient" for a connection definition; the metaclass does.
    let source = "\
package P {
    part def A;
    part def B;
    connection def C {
        end a : A;
        end b : B;
    }
}
";
    let mods = modifiers(source, "C");
    assert!(
        mods.is_sufficient,
        "a connection definition is sufficient by the metamodel, not by the notation"
    );
}

#[test]
fn a_connection_definition_is_not_reported() {
    let source = "\
package P {
    part def A;
    connection def C {
        end a : A;
        end b : A;
    }
}
";
    let found = errors(source);
    assert!(
        !found
            .iter()
            .any(|m| m.contains("validateConnectionDefinitionIsSufficient")),
        "the constraint holds by construction, got: {found:#?}"
    );
}

#[test]
fn a_plain_part_definition_is_not_made_sufficient() {
    // The fill must be keyed on the kind, or every definition would claim sufficiency.
    let source = "\
package P {
    part def A;
}
";
    let mods = modifiers(source, "A");
    assert!(
        !mods.is_sufficient,
        "only a connection definition gets the default"
    );
}

#[test]
fn the_element_is_reachable_at_all() {
    // Guards the three tests above: if the lookup silently failed they would all pass on a panic-free
    // path only because `modifiers` panics, and this states the precondition plainly.
    let source = "package P { part def A; connection def C { end a : A; end b : A; } }";
    let (roots, _library) = load(source);
    assert!(
        id_of(&roots, "C").is_some(),
        "the connection definition must be in the tree"
    );
}

#[test]
fn an_enumeration_definition_is_not_treated_as_a_variation() {
    // The regression guard. `EnumerationDefinition::isVariation` is `true` in the metamodel too, and
    // filling it made `variation_members` report every ordinary literal — seven spurious diagnostics
    // and one officially-clean model rejected. Until that rule's reading of the flag is reconciled,
    // the flag must stay as the model wrote it.
    let source = "\
package P {
    enum def E {
        a;
        b;
    }
}
";
    let mods = modifiers(source, "E");
    assert!(
        !mods.is_variation,
        "filling `is_variation` on an enumeration definition breaks `variation_members`"
    );
    let found = errors(source);
    assert!(
        found.is_empty(),
        "an ordinary enumeration must be accepted, got: {found:#?}"
    );
}
