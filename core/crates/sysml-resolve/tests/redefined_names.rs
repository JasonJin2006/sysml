//! A feature hides the member it redefines from its own type name.
//!
//! `CalculationUsage_Invalid2.sysml.xt:38-53` states the case: `calc c2 : C { return r1 : T; }`
//! redefines the result parameter `T` of `C`, and the `T` written as `r1`'s type names the
//! `attribute def T` outside, not the parameter it replaces.

use sysml_resolve::{Index, Resolution};

/// Every written reference in the model, as (from, text, what it bound to).
fn bound(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    index
        .references(&loaded.roots)
        .into_iter()
        .map(|reference| {
            let target = match &reference.target {
                Resolution::Found(id) => index.qualified[*id].clone(),
                other => format!("{other:?}"),
            };
            (
                index.qualified[reference.from].clone(),
                reference.text,
                target,
            )
        })
        .collect()
}

#[test]
fn a_type_name_skips_the_member_the_feature_redefines() {
    let found = bound(
        "package Test {\n\tattribute def T;\n\tcalc def C { return T; }\n\tcalc c2 : C {\n\t\treturn r1 : T;\n\t}\n}\n",
    );
    let r1 = found
        .iter()
        .find(|(from, text, _)| from == "Test::c2::r1" && text == "T")
        .expect("r1's type is a reference");
    assert_eq!(r1.2, "Test::T", "{found:?}");
}

#[test]
fn a_name_that_is_not_redefined_still_finds_the_inherited_member() {
    // An input parameter redefines no result, so the inherited `T` is what the name means. Only a
    // redefinition may hide it: an unnamed `return : T` would redefine the result and hide it.
    let found = bound(
        "package Test {\n\tattribute def T;\n\tcalc def C { return T; }\n\tcalc c3 : C {\n\t\tin x : T;\n\t}\n}\n",
    );
    assert!(
        found
            .iter()
            .any(|(from, text, target)| from.starts_with("Test::c3")
                && text == "T"
                && target == "Test::C::T"),
        "{found:?}"
    );
}
