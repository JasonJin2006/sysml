//! The plain-text outline, including how an import statement spells itself.

use sysml_syntax::load;
use sysml_view::outline;

#[test]
fn an_import_prints_its_suffix_not_its_target_twice() {
    let loaded = load(
        r#"
package P {
    private import ISQ::*;
    public import A::*::**;
    private import ScalarValues::Real;
    protected import B [ 1 ];
}
"#,
    )
    .expect("parses");
    let text = outline(&loaded.roots);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[1..],
        [
            "  import ISQ::* (private)",
            "  import A::*::** (public)",
            "  import ScalarValues::Real (private)",
            "  import B::[filter] (protected)",
        ]
    );
}

#[test]
fn ordinary_elements_still_print_their_type() {
    let loaded = load(
        r#"
package P {
    part def W {
        attribute mass : MassValue;
    }
}
"#,
    )
    .expect("parses");
    let text = outline(&loaded.roots);
    assert!(text.contains("attribute mass : MassValue"), "{text}");
}

#[test]
fn a_visibility_keyword_belongs_to_the_member_it_is_written_on() {
    // `private import ScalarValues::*;` used to leave its `private` pending, so the declaration
    // after the import became private too — a model whose next member was a public library type
    // printed it as private and answered visibility questions from that.
    let loaded = load(
        r#"
package P {
    private import ScalarValues::*;
    part def A;
    part def B;
}
"#,
    )
    .expect("parses");
    let text = outline(&loaded.roots);
    assert!(text.contains("part def A\n"), "{text}");
    assert!(text.contains("part def B\n"), "{text}");
    assert!(
        !text.contains("part def A (private)"),
        "the import's visibility is not A's: {text}"
    );
}

#[test]
fn an_annotation_keeps_the_visibility_the_declaration_carries() {
    // The one case the keyword is handed on: the annotation is parsed first and the declaration
    // arrives in the next member.
    let loaded = load(
        r#"
package P {
    private #Safety part def B2;
}
"#,
    )
    .expect("parses");
    let text = outline(&loaded.roots);
    assert!(text.contains("part def B2 (private)"), "{text}");
}
