//! `@Safety { level = 3 } part def Brake;` — the annotation belongs to the declaration after it.
//!
//! Lowering both as members of the body put the annotation *beside* Brake: the tree carried a
//! metadata usage in the package and a part def with no annotation, so nothing downstream could ask
//! what Brake's safety level was. These tests pin where an annotation ends up.

use sysml_model::{Element, Kind};

fn roots(src: &str) -> Vec<Element> {
    let loaded = sysml_syntax::load_project(&[src], false).expect("model parses");
    loaded.roots
}

fn find<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
    if el.name == name {
        return Some(el);
    }
    el.children.iter().find_map(|c| find(c, name))
}

fn annotated(src: &str, name: &str) -> Vec<String> {
    let roots = roots(src);
    let target = roots
        .iter()
        .find_map(|r| find(r, name))
        .unwrap_or_else(|| panic!("no element called {name}"));
    target
        .children
        .iter()
        .filter(|c| c.kind == Kind::MetadataUsage)
        .map(|c| c.type_ref.clone().unwrap_or_default())
        .collect()
}

#[test]
fn an_annotation_before_a_declaration_annotates_it() {
    let src = r#"
package P {
    metadata def Safety {
        attribute level : Integer;
    }
    @Safety { level = 3; }
    part def Brake;
}
"#;
    assert_eq!(annotated(src, "Brake"), ["Safety"]);
    // And the package no longer holds a stray metadata usage beside it.
    let package = &roots(src)[0];
    assert_eq!(
        package
            .children
            .iter()
            .filter(|c| c.kind == Kind::MetadataUsage)
            .count(),
        0,
        "the annotation must not stay a member of the package"
    );
}

#[test]
fn two_annotations_before_one_declaration_both_attach_in_order() {
    let src = r#"
package P {
    metadata def Safety {
        attribute level : Integer;
    }
    metadata def Owner {
        attribute team : String;
    }
    @Safety { level = 3; }
    @Owner { team = "brakes"; }
    part def Brake;
}
"#;
    assert_eq!(annotated(src, "Brake"), ["Safety", "Owner"]);
}

#[test]
fn an_annotation_with_about_names_its_target_and_stays_put() {
    // `about` says what is annotated, so there is nothing to infer from position.
    let src = r#"
package P {
    metadata def Safety {
        attribute level : Integer;
    }
    part def Brake;
    @Safety about Brake { level = 3; }
}
"#;
    let package = &roots(src)[0];
    let annotation = package
        .children
        .iter()
        .find(|c| c.kind == Kind::MetadataUsage)
        .expect("the annotation stays a member of the package");
    assert_eq!(annotation.to.as_deref(), Some("Brake"));
    assert!(
        annotated(src, "Brake").is_empty(),
        "`about` does not attach"
    );
}

#[test]
fn an_annotation_with_nothing_after_it_stays_a_member() {
    // A body that ends with an annotation annotates nothing in that body. Dropping it would lose a
    // thing the model wrote; making it annotate the enclosing element would invent one.
    let src = r#"
package P {
    metadata def Safety {
        attribute level : Integer;
    }
    @Safety { level = 3; }
}
"#;
    let package = &roots(src)[0];
    assert_eq!(
        package
            .children
            .iter()
            .filter(|c| c.kind == Kind::MetadataUsage)
            .count(),
        1
    );
}

#[test]
fn an_annotation_is_anonymous_so_it_does_not_shadow_its_own_type() {
    // Naming the usage after its type made `@Bogus` resolve its own `type_ref` to itself, which is
    // why an annotation naming a type that does not exist passed `check`.
    let src = r#"
package P {
    metadata def Safety {
        attribute level : Integer;
    }
    @Safety { level = 3; }
    part def Brake;
}
"#;
    let roots = roots(src);
    let brake = roots.iter().find_map(|r| find(r, "Brake")).expect("Brake");
    let annotation = brake
        .children
        .iter()
        .find(|c| c.kind == Kind::MetadataUsage)
        .expect("attached");
    assert_eq!(annotation.name, "<anonymous>");
    assert_eq!(annotation.type_ref.as_deref(), Some("Safety"));
}

#[test]
fn a_self_terminated_annotation_annotates_its_container() {
    // SysML §7.27.2 (PDF p.192): "if the metadata usage is an owned member of a namespace, then the
    // explicit identification of annotated elements can be omitted, in which case the annotated
    // element shall be implicitly the containing namespace". `@B;` ends at its own `;`, so it is a
    // member of `C1` and annotates `C1` — not the `:>> x` that happens to follow it. Attaching it to
    // the next member made `class C1 { @B; feature :>> x; }` claim that `x` was annotated, which is
    // the opposite of what the model says (measured: `outline` showed `feature x / metadata B`).
    let src = r#"
package P {
    metadata def B;
    class C1 {
        @B;
        feature x;
    }
}
"#;
    let roots = roots(src);
    let c1 = roots.iter().find_map(|r| find(r, "C1")).expect("C1");
    let direct: Vec<&Element> = c1
        .children
        .iter()
        .filter(|c| c.kind == Kind::MetadataUsage)
        .collect();
    assert_eq!(
        direct.len(),
        1,
        "the annotation is a member of C1, not of the feature below it"
    );
    assert_eq!(direct[0].type_ref.as_deref(), Some("B"));
    let x = c1.children.iter().find(|c| c.name == "x").expect("x");
    assert!(
        x.children.iter().all(|c| c.kind != Kind::MetadataUsage),
        "x carries no annotation: the model wrote `@B;` before it, not on it"
    );
}

#[test]
fn an_annotation_never_annotates_another_annotation() {
    // `part y { @Classified { … } @Security; }` writes two annotations and no declaration. Under the
    // "attach to the next member" rule the first was buried inside the second (measured: `outline`
    // showed `metadata Security { metadata Classified { … } }`), and two rules then rejected a model
    // the reference implementation accepts. Neither annotation is a `MemberPrefix`, so §7.27.2 gives
    // both to `y` and neither to the other.
    let src = r#"
package P {
    metadata def Classified { attribute level : Integer; }
    metadata def Security;
    part y {
        @Classified { level = 3; }
        @Security;
    }
}
"#;
    let roots = roots(src);
    let y = roots.iter().find_map(|r| find(r, "y")).expect("y");
    let direct: Vec<&Element> = y
        .children
        .iter()
        .filter(|c| c.kind == Kind::MetadataUsage)
        .collect();
    assert_eq!(
        direct.len(),
        2,
        "both annotations belong to `y`, not one inside the other"
    );
    for annotation in &direct {
        assert!(
            annotation
                .children
                .iter()
                .all(|c| c.kind != Kind::MetadataUsage),
            "an annotation must not nest another annotation"
        );
    }
}

#[test]
fn a_hash_annotation_is_a_metadata_usage() {
    // `#Safety` is the keyword spelling of `@Safety` (`KerML.xtext:1069`, and `:1083` writes the
    // three spellings as `'@' | 'metadata'` in one production). Lowering it to prose dropped the
    // type: `#Classified #Security part z1;` left `part z1` with no annotation at all, so the
    // §7.27.3 derivation had nothing to read and a `noErrors` model reported three unresolved
    // redefinitions.
    let src = r#"
package P {
    metadata def Safety { attribute level : Integer; }
    #Safety part def Brake;
}
"#;
    assert_eq!(annotated(src, "Brake"), ["Safety"]);
}

#[test]
fn an_annotation_after_the_visibility_still_attaches() {
    // Visibility belongs to the membership and the annotation to the element, so `private #Safety
    // part def B;` and `#Safety private part def B;` declare the same thing. Only the second was
    // read; the first dropped the annotation entirely.
    let first = r#"
package P {
    metadata def Safety { attribute level : Integer; }
    private #Safety part def Brake;
}
"#;
    let second = r#"
package P {
    metadata def Safety { attribute level : Integer; }
    #Safety private part def Brake;
}
"#;
    assert_eq!(annotated(first, "Brake"), ["Safety"]);
    assert_eq!(annotated(second, "Brake"), ["Safety"]);
}
