//! What a box says it is.
//!
//! The stereotype is the first thing a reader scans, and it carries the modifiers as well as the
//! kind: `«ref part»` and `«part»` are not the same claim, and a diagram that drew both as `«part»`
//! was erasing the distinction the declaration was written to make. The values here are the ones
//! the reference renderings use.

use sysml_view::graph;

fn stereotypes(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load(src).expect("parses");
    let g = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    g.nodes.iter().map(|n| n.stereotype.clone()).collect()
}

fn of(src: &str, name: &str) -> String {
    let loaded = sysml_syntax::load(src).expect("parses");
    let g = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    g.nodes
        .iter()
        .find(|n| n.label() == name)
        .unwrap_or_else(|| panic!("no box called {name}"))
        .stereotype
        .clone()
}

#[test]
fn a_plain_part_is_drawn_without_modifiers() {
    assert_eq!(of("package P { part def A; }", "A"), "part def");
    assert_eq!(of("package P { part def A { part b : A; } }", "b"), "part");
}

#[test]
fn a_reference_and_an_end_are_named() {
    let src = "package P { part def A { ref part borrowed : A; end part ender; } }";
    assert_eq!(of(src, "borrowed"), "ref part");
    assert_eq!(of(src, "ender"), "end part");
}

#[test]
fn an_abstract_definition_is_named() {
    assert_eq!(
        of("package P { abstract part def Base; }", "Base"),
        "abstract part def"
    );
}

#[test]
fn a_variation_and_its_alternatives_are_named() {
    // The picture exists to show which alternatives a variation offers, which a list of identical
    // `«part»` boxes cannot.
    let src = "package P { part def Base; variation part def Pack :> Base { variant part a; } }";
    assert!(of(src, "Pack").contains("variation"), "{}", of(src, "Pack"));
    assert_eq!(of(src, "a"), "variant part");
}

#[test]
fn a_timeslice_and_a_snapshot_are_named_by_the_restriction() {
    // The reference writes `«timeslice»` alone; `«timeslice occurrence»` says the occurrence part
    // twice and reads as a different kind.
    let src = "package P { part def A { timeslice T; snapshot S; } }";
    assert_eq!(of(src, "T"), "timeslice");
    assert_eq!(of(src, "S"), "snapshot");
}

#[test]
fn an_ordinary_part_keeps_its_short_name() {
    // The regression this guards: a modifier table that fires on the default value turns every
    // box into `«composite part»` and the picture stops distinguishing anything.
    let all = stereotypes("package P { part def A { part b : A; } }");
    assert!(
        all.iter().all(|s| !s.contains("composite")),
        "no box carries a modifier nobody wrote: {all:?}"
    );
}

#[test]
fn a_state_and_an_enumeration_keep_their_kinds() {
    let src = "package P { enum def Color { red, green } state def M { state idle; } }";
    assert_eq!(of(src, "Color"), "enum def");
    assert_eq!(of(src, "red"), "enum");
    assert_eq!(of(src, "M"), "state def");
    assert_eq!(of(src, "idle"), "state");
}
