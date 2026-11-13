//! SysML §7.27.3 — the specializations a semantic-metadata annotation derives.
//!
//! "If the metadata definition of a metadata usage is a direct or indirect specialization of KerML
//! metaclass SemanticMetadata …, each annotated element is then considered to implicitly specialize
//! a definition or usage determined from the baseType value as follows: if the annotated type is a
//! definition and the baseType is a usage …, then the annotated definition implicitly subclassifies
//! each definition … of the baseType [value]; if the annotated type is a usage and the baseType is a
//! usage …, then the annotated usage implicitly subsets the baseType" — §7.27.3, PDF p.192-193.
//!
//! The rule matters because it is the only way a redefinition inside an annotated element can find
//! its target. Without it `class C2 { @B; feature :>> x; }` reports `unresolved: x` even though the
//! reference implementation accepts it — measured on
//! `MetadataTests_SemanticMetadata_valid.kerml.xt`, a `noErrors` model.
//!
//! Every test asks the question the CLI asks when it prints `unresolved: <text>`: does this clause
//! resolve? Reading through [`Index::references`] rather than an internal helper keeps a unit
//! assertion from passing while the tool still reports the diagnostic.
//!
//! The negative cases are the load-bearing half. A reading that gave the annotated element every
//! member of the baseType *value* would also make `C2::y` legal, and
//! `MetadataTests_SemanticMetadata_invalid.kerml.xt:58` demands that `y` be reported. Turning that
//! diagnostic into a silence would be worse than the false positive it replaces.

use sysml_resolve::Index;

/// Whether the `:>>` clause named `clause` resolves, for every element in the model.
///
/// Returns `(clause, resolved)` pairs, so a caller can assert both "this one resolves" and "that one
/// does not" without the second assertion accidentally passing because nothing was found at all.
fn redefinitions(src: &str) -> Vec<(String, bool)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    // `with_library` is what the CLI calls (`sysml-cli/src/main.rs:286`). Without it the standard
    // library is loaded but not reachable from the index, `Metaobjects::SemanticMetadata` resolves
    // to nothing, and every derivation under test silently does nothing — the tests would then
    // assert on a code path the tool never runs.
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    index
        .references(&loaded.roots)
        .into_iter()
        .filter(|r| !r.in_predicate)
        // The clauses under test are redefinitions; a typing reference (`: C`) would also appear
        // here, and matching only the names the model redefines keeps the assertion readable.
        .filter(|r| ["x", "y", "annotatedElement"].contains(&r.text.as_str()))
        .map(|r| (r.text, r.target.is_resolved()))
        .collect()
}

/// The `(clause, resolved)` pair for `clause`, panicking when the model never wrote it.
fn clause(pairs: &[(String, bool)], name: &str) -> bool {
    let found: Vec<&(String, bool)> = pairs.iter().filter(|(c, _)| c == name).collect();
    assert!(
        !found.is_empty(),
        "the model never redefines `{name}`, so the assertion would be vacuous: {pairs:?}"
    );
    // A model may write the same clause more than once; report whether all of them resolve, which
    // is what the caller means by "this name is reachable".
    found.iter().all(|(_, ok)| *ok)
}

const MODEL: &str = r#"
package SemanticMetadata_valid {
    class C {
        feature x;
    }
    feature f : C {
        feature y;
    }
    abstract metaclass A :> Metaobjects::SemanticMetadata {
        feature :>> baseType;
    }
    metaclass B :> A {
        feature :>> baseType = f meta KerML::Type;
    }
    class C1 {
        @B;
        feature :>> x;
    }
    feature f1 {
        @B;
        feature :>> x;
        feature :>> y;
    }
}
"#;

#[test]
fn an_annotated_definition_subclassifies_the_base_type_values_type() {
    // `C1` is a definition and `f` is a usage, so §7.27.3 gives `C1` the *type* of `f` — `C` — and
    // not `f` itself. `x` is a member of `C`, so the redefinition resolves.
    let pairs = redefinitions(MODEL);
    assert!(
        clause(&pairs, "x"),
        "`C1`'s `:>> x` must resolve through the implicit subclassification, got {pairs:?}"
    );
}

#[test]
fn an_annotated_usage_subsets_the_base_type_value_itself() {
    // `f1` is a usage, so it subsets `f`, which declares both `x` and `y`. This is the case that
    // distinguishes "subsets the value" from "subclassifies the value's type": `y` is reachable
    // here and unreachable in the definition case below.
    let pairs = redefinitions(MODEL);
    assert!(
        clause(&pairs, "y"),
        "`f1`'s `:>> y` must resolve through the implicit subsetting, got {pairs:?}"
    );
}

#[test]
fn an_annotated_definition_does_not_reach_the_base_type_values_own_members() {
    // The negative half, and the reason the derivation reads the *type* of the value. In
    // `MetadataTests_SemanticMetadata_invalid.kerml.xt:55-59` the same annotation sits on a class
    // where `:>> x` is legal and `:>> y` must be reported, because `y` belongs to `f` and not to
    // `C`. A derivation that handed over `f`'s members would make `y` legal and lose that
    // diagnostic.
    let src = r#"
package p {
    class C {
        feature x;
    }
    feature f : C {
        feature y;
    }
    abstract metaclass A :> Metaobjects::SemanticMetadata {
        feature :>> baseType;
    }
    metaclass B :> A {
        feature :>> baseType = f meta KerML::Type;
    }
    class C2 {
        @B;
        feature :>> x;
        feature :>> y;
    }
}
"#;
    let pairs = redefinitions(src);
    assert!(
        clause(&pairs, "x"),
        "`x` is a member of `C` and must resolve, got {pairs:?}"
    );
    assert!(
        !clause(&pairs, "y"),
        "`y` is a member of `f` only, so `C2` (a definition) must not resolve it, got {pairs:?}"
    );
}

#[test]
fn an_unbound_base_type_derives_nothing() {
    // `abstract metaclass A` leaves `baseType` unbound. In
    // `MetadataTests_SemanticMetadata_invalid.kerml.xt:49-52` the reference implementation reports
    // the annotated class's `:>> x` as unresolved for exactly this reason, so an annotation whose
    // definition binds no base must not add an edge.
    let src = r#"
package p {
    abstract metaclass A :> Metaobjects::SemanticMetadata {
        feature :>> baseType;
    }
    class C1 {
        @A;
        feature :>> x;
    }
}
"#;
    let pairs = redefinitions(src);
    assert!(
        !clause(&pairs, "x"),
        "with no baseType value there is nothing to specialize, got {pairs:?}"
    );
}

#[test]
fn a_plain_annotation_without_semantic_metadata_derives_nothing() {
    // The derivation is gated on reaching `Metaobjects::SemanticMetadata`. A metadata definition
    // that does not specialize it is ordinary metadata: it annotates without changing the
    // hierarchy, so the redefinition inside the annotated class still has no target.
    let src = r#"
package p {
    metadata def Plain;
    class C1 {
        @Plain;
        feature :>> x;
    }
}
"#;
    let pairs = redefinitions(src);
    assert!(
        !clause(&pairs, "x"),
        "ordinary metadata derives no specialization, got {pairs:?}"
    );
}

#[test]
fn a_written_specialization_still_resolves_without_any_annotation() {
    // Positive control for `redefinitions` and `clause`. Without it, a helper that found nothing
    // would make the two negative tests above pass for the wrong reason.
    let src = r#"
package p {
    class C {
        feature x;
    }
    class C1 :> C {
        feature :>> x;
    }
}
"#;
    let pairs = redefinitions(src);
    assert!(
        clause(&pairs, "x"),
        "a written `:>` must resolve `x` with no metadata involved, got {pairs:?}"
    );
}

#[test]
fn a_metaclass_sees_the_annotated_element_it_inherits() {
    // "It is the most general Metaclass. All other Metaclasses must subclassify it directly or
    // indirectly" — `Metaobjects.kerml:16`. `Metaobject` declares `annotatedElement`
    // (`Metaobjects.kerml:21`), so a metaclass that writes no `:>` clause must still reach it:
    // `MetadataTests_MetadataFeature_invalid.kerml.xt:42` writes exactly
    // `feature :>> annotatedElement : KerML::Feature;` inside a bare `metaclass A` and the
    // reference implementation accepts it.
    let src = r#"
package p {
    metaclass A {
        feature :>> annotatedElement;
    }
}
"#;
    let pairs = redefinitions(src);
    assert!(
        clause(&pairs, "annotatedElement"),
        "a metaclass implicitly specializes Metaobject, which declares annotatedElement, got {pairs:?}"
    );
}

#[test]
fn a_plain_class_does_not_see_annotated_element() {
    // The negative half of the test above: the implicit edge belongs to `metaclass`, and a `class`
    // is not one. If the derivation were written as "any element may reach any library member" this
    // would start passing and a real error would become a silence.
    let src = r#"
package p {
    class A {
        feature :>> annotatedElement;
    }
}
"#;
    let pairs = redefinitions(src);
    assert!(
        !clause(&pairs, "annotatedElement"),
        "only a metaclass specializes Metaobject, got {pairs:?}"
    );
}
