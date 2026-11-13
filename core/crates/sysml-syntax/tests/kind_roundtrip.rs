//! Every kind this kernel prints can be read back as the same kind.
//!
//! `Kind::keyword` is what `sysml outline`, `diff` and the JSON output print, and it is also what
//! a round-trip test can feed back into the parser. If a keyword has no parse path, the printed
//! form is a dead end: a reader who copies it into a model gets a syntax error, and a tool that
//! rewrites a model from its own output loses the element.

use sysml_model::Kind;
use sysml_syntax::load;

/// The definition kinds whose keyword is a plain declaration word.
///
/// Not every `Kind` has one: relationship words (`import`, `alias`, `satisfy`) and statement words
/// (`connect`, `bind`) are not declarations, and `Kind::DefaultReference` names the absence of a
/// keyword. Those are exercised by the grammar tests.
fn definition_kinds() -> &'static [Kind] {
    use Kind::*;
    &[
        Package,
        PartDef,
        AttributeDef,
        PortDef,
        ItemDef,
        ActionDef,
        StateDef,
        RequirementDef,
        ConstraintDef,
        InterfaceDef,
        ConnectionDef,
        VerificationDef,
        RenderingDef,
    ]
}

#[test]
fn a_definition_keyword_parses_back_to_the_same_kind() {
    for kind in definition_kinds() {
        let word = kind.keyword_as(true);
        let text = format!("package P {{\n\t{word} X;\n}}\n");
        let loaded = match load(&text) {
            Ok(loaded) => loaded,
            Err(e) => panic!("`{word}` does not parse: {e}"),
        };
        let found = loaded
            .roots
            .iter()
            .flat_map(|r| r.children.iter())
            .find(|e| e.name == "X")
            .unwrap_or_else(|| panic!("`{word} X;` produced no element named X"));
        assert_eq!(
            &found.kind, kind,
            "`{word}` reads back as {:?}, not {kind:?}",
            found.kind
        );
    }
}
