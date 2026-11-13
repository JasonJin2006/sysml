//! Member access whose base is not a name: `new D().b` and `(x as T).b`.
//!
//! Both spellings read a feature of the base's *type*, and both were wrong in opposite directions
//! before `Expr::Member` existed:
//!
//!   * `new D().b` was flattened into the text `new D().b`, which is not a name, so it resolved to
//!     nothing and a legal model was rejected with "value expression reads `new D().b`, which names
//!     nothing";
//!   * `(x as T).b` was lowered to the cast alone with the member **dropped**, so
//!     `(x as T).zzzNoSuchMember` was accepted. A false negative is the worse of the two: the tool
//!     reports success on something it never checked.
//!
//! The last two tests pin the boundary: a member read from a *name* stays a dotted path, because a
//! member of a name is resolved in that name's scope — which is a different question, answered by a
//! different walk.

use sysml_semantics::{check, Severity};

/// Error diagnostics for a model, as `(code, message)`.
fn errors(src: &str) -> Vec<(String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| (i.code.to_string(), i.message.clone()))
        .collect()
}

const HEAD: &str = r#"
package P {
    part def A { attribute a; }
    part def D :> A { part b :>> a; }
    part instance1 : D;
"#;

#[test]
fn a_member_of_a_constructed_value_is_found() {
    // `new D()` is a `D`, and `D` declares `b`. Before the change this was "names nothing".
    let src = format!("{HEAD}    part p = new D().b;\n}}\n");
    let found = errors(&src);
    assert!(
        found.is_empty(),
        "the member of a constructed value was rejected: {found:?}"
    );
}

#[test]
fn a_member_inherited_through_a_supertype_is_found() {
    // `a` is declared on `A`, not on `D`, so the lookup has to follow `D :> A`.
    let src = format!("{HEAD}    part p = new D().a;\n}}\n");
    let found = errors(&src);
    assert!(
        found.is_empty(),
        "an inherited member was rejected: {found:?}"
    );
}

#[test]
fn a_member_a_constructed_type_does_not_declare_is_reported() {
    let src = format!("{HEAD}    part p = new D().zzzNoSuchMember;\n}}\n");
    let found = errors(&src);
    assert_eq!(found.len(), 1, "expected one diagnostic, got {found:?}");
    assert_eq!(found[0].0, "SEM118");
    assert!(
        found[0].1.contains("zzzNoSuchMember"),
        "the message must name the member: {}",
        found[0].1
    );
}

#[test]
fn a_member_of_a_cast_is_found() {
    let src = format!("{HEAD}    part p = (instance1 as D).b;\n}}\n");
    let found = errors(&src);
    assert!(
        found.is_empty(),
        "the member of a cast was rejected: {found:?}"
    );
}

#[test]
fn a_member_a_cast_type_does_not_declare_is_reported() {
    // This is the false negative the change fixes: the member used to be dropped from the
    // expression, so no name was left to check and anything at all was accepted.
    let src = format!("{HEAD}    part p = (instance1 as D).zzzNoSuchMember;\n}}\n");
    let found = errors(&src);
    assert_eq!(
        found.len(),
        1,
        "an invented member after a cast must be reported, got {found:?}"
    );
    assert_eq!(found[0].0, "SEM118");
}

#[test]
fn a_dotted_name_on_a_declared_feature_resolves() {
    // The counter-case that keeps the fix narrow: `instance1.b` is a member access on a *name*, so it
    // stays a dotted `Ref` and is resolved by the path walk.
    let src = format!("{HEAD}    part p = instance1.b;\n}}\n");
    let found = errors(&src);
    assert!(
        found.is_empty(),
        "a dotted name on a declared feature was rejected: {found:?}"
    );
}

#[test]
fn a_scope_sibling_after_a_dot_is_still_reported() {
    // `S` is a struct in the enclosing scope, not a feature of `instance1`'s type. The pilot reports
    // this ("Must be a valid feature", `FeatureChain_invalid.kerml.xt:61`), and it only stays
    // reported while the dotted form keeps going through the path walk rather than through the type.
    let src = r#"
package P {
    struct S;
    part def D;
    part instance1 : D;
    part p = instance1.S;
}
"#;
    let found = errors(src);
    assert_eq!(
        found.len(),
        1,
        "a scope sibling read through a feature must be reported, got {found:?}"
    );
}
