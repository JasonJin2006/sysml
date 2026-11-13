//! `metadata-annotated-element` — an annotation describes only what it says it describes.
//!
//! `validateMetadataFeatureAnnotatedElement`. `metadata def A { ref :>> annotatedElement :
//! SysML::AttributeUsage; }` says "A describes attribute usages". Annotating an attribute with it
//! is legal; annotating a classifier is not.
//!
//! The two models are the pilot's own:
//! `MetadataTests_MetadataFeature_invalid.kerml.xt:80` ("Cannot annotate Classifier") and
//! `MetadataUsage_Invalid.sysml.xt:96` ("Cannot annotate ItemUsage").

use sysml_semantics::{check, Severity};

fn sem094(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.code == "SEM094" && i.severity == Severity::Error)
        .map(|i| i.message.clone())
        .collect()
}

#[test]
fn an_annotation_on_a_classifier_is_rejected() {
    // The KerML spelling: `metaclass A { ... }` describing attribute usages, applied to a
    // classifier.
    let src = r#"
package P {
	metaclass A {
		feature :>> annotatedElement : KerML::Feature;
	}
	classifier B {
		@A;
	}
}
"#;
    let found = sem094(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].contains("cannot annotate"), "got {found:?}");
}

#[test]
fn an_annotation_on_the_element_it_describes_is_accepted() {
    let src = r#"
package P {
	metadata def Safety {
		ref :>> annotatedElement : SysML::AttributeUsage;
		attribute level : Integer;
	}
	attribute mass {
		@Safety { level = 3; }
	}
}
"#;
    assert!(sem094(src).is_empty(), "got {:?}", sem094(src));
}

#[test]
fn an_annotation_that_describes_nothing_is_accepted() {
    // `annotatedElement` is optional. A metadata definition without one constrains nothing, which
    // is how every ordinary `metadata def Safety { attribute level : Integer; }` is written.
    let src = r#"
package P {
	metadata def Safety {
		attribute level : Integer;
	}
	classifier B {
		@Safety { level = 3; }
	}
}
"#;
    assert!(sem094(src).is_empty(), "got {:?}", sem094(src));
}
