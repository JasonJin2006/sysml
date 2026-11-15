//! Inferred literal types as hints; ownership details belong in hover.

use sysml_model::{Element, Kind};

/// The protocol's `InlayHintKind`: a type annotation.
const KIND_TYPE: u8 = 1;

/// One hint, placed at a byte offset in one buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    /// Index of the buffer the position is in.
    pub file: usize,
    /// Byte offset, which the caller turns into a protocol position.
    pub offset: usize,
    /// The text drawn.
    pub label: String,
    pub kind: u8,
    /// Padding, so the hint does not touch the word beside it.
    pub padding_left: bool,
}

/// Every hint for one project, in no particular order.
pub fn hints(roots: &[Element]) -> Vec<Hint> {
    let mut out = Vec::new();
    for element in walk(roots) {
        if element.kind == Kind::Unsupported {
            // A construct the grammar rejected has no computed state to show; a hint beside it
            // would describe something that is not there.
            continue;
        }
        implied_type(element, &mut out);
    }
    out
}

/// The type a feature takes from its name, when it wrote none.
///
/// `attribute mass = 12` has no `: T`, and the language does not require one: an attribute whose
/// value is a number is a `Real`, and the checker decides rules about that. The hint names the type
/// the kernel computed, which is the one thing a reader cannot recover from the line.
fn implied_type(element: &Element, out: &mut Vec<Hint>) {
    // A declaration that wrote a type has said it; only silence is hinted.
    if element.type_ref.is_some() || element.value_expr.is_none() {
        return;
    }
    // Only features. A definition's type is itself, and an import's "type" is its target.
    if !matches!(
        element.kind,
        Kind::AttributeUsage | Kind::PartUsage | Kind::ItemUsage
    ) {
        return;
    }
    let Some(span) = element.name_span else {
        return;
    };
    let Some(value) = &element.value_text else {
        return;
    };
    // The value's own spelling tells the type for the literal cases, and that is the reading a
    // person makes too: `= 12` is a number, `= 12.5` a real, `= true` a boolean, `= "x"` a string.
    // Anything else is an expression whose type the evaluator would have to answer, and it is not
    // asked here: a wrong hint is worse than none, and an expression's type is a claim.
    let trimmed = value.trim();
    let inferred = if trimmed == "true" || trimmed == "false" {
        Some("Boolean")
    } else if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        Some("String")
    } else if trimmed.parse::<i64>().is_ok() {
        Some("Integer")
    } else if trimmed.parse::<f64>().is_ok() {
        Some("Real")
    } else {
        None
    };
    let Some(name) = inferred else {
        return;
    };
    // The hint goes after the name, before anything the declaration wrote next.
    out.push(Hint {
        file: span.file,
        offset: span.end,
        label: format!(": {name}"),
        kind: KIND_TYPE,
        padding_left: false,
    });
}

/// Every element of a forest, pre-order.
fn walk(roots: &[Element]) -> Vec<&Element> {
    fn one<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
        out.push(el);
        for child in &el.children {
            one(child, out);
        }
    }
    let mut out = Vec::new();
    for root in roots {
        one(root, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints_of(src: &str) -> Vec<Hint> {
        let loaded = sysml_syntax::load(src).expect("parses");
        hints(&loaded.roots)
    }

    fn labels(hints: &[Hint]) -> Vec<&str> {
        hints.iter().map(|h| h.label.as_str()).collect()
    }

    #[test]
    fn a_composite_feature_does_not_get_an_inline_ownership_label() {
        let hints = hints_of("package P {\n  part def Car {\n    part engine;\n  }\n}\n");
        assert!(hints.is_empty(), "{:?}", labels(&hints));
    }

    #[test]
    fn a_reference_is_not_called_composite() {
        // `ref part` is a reference, not a part of its owner, and saying otherwise would invert
        // the one thing the word means.
        let hints = hints_of("package P {\n  part def Car {\n    ref part engine;\n  }\n}\n");
        assert!(
            !labels(&hints).iter().any(|l| l.trim() == "composite"),
            "{:?}",
            labels(&hints)
        );
    }

    #[test]
    fn a_declaration_that_wrote_composite_gets_no_hint() {
        let hints = hints_of("package P {\n  part def Car {\n    composite part engine;\n  }\n}\n");
        assert!(
            !labels(&hints).iter().any(|l| l.trim() == "composite"),
            "the text already says it: {:?}",
            labels(&hints)
        );
    }

    #[test]
    fn a_literal_value_supplies_a_type() {
        let hints = hints_of("package P {\n  part def A {\n    attribute mass = 12;\n  }\n}\n");
        assert!(
            labels(&hints).contains(&": Integer"),
            "{:?}",
            labels(&hints)
        );
    }

    #[test]
    fn a_real_boolean_and_string_are_told_apart() {
        let src = "package P {\n  part def A {\n    attribute a = 1.5;\n    attribute b = true;\n    attribute c = \"text\";\n  }\n}\n";
        let hints = hints_of(src);
        let all = labels(&hints);
        assert!(all.contains(&": Real"), "{all:?}");
        assert!(all.contains(&": Boolean"), "{all:?}");
        assert!(all.contains(&": String"), "{all:?}");
    }

    #[test]
    fn a_declaration_that_wrote_its_type_gets_no_hint() {
        let hints =
            hints_of("package P {\n  part def A {\n    attribute mass : Real = 12;\n  }\n}\n");
        assert!(
            !labels(&hints).iter().any(|l| l.starts_with(": ")),
            "the type is written: {:?}",
            labels(&hints)
        );
    }

    #[test]
    fn an_expression_is_not_given_a_type() {
        // The type of `a + b` is a claim the evaluator would have to make. A hint that guessed
        // would be a second opinion about the model, and the wrong one costs more than silence.
        let hints = hints_of("package P {\n  part def A {\n    attribute a = 1 + 2;\n  }\n}\n");
        assert!(
            !labels(&hints).iter().any(|l| l.starts_with(": ")),
            "{:?}",
            labels(&hints)
        );
    }

    #[test]
    fn a_parse_failure_gets_no_hint() {
        let hints = hints_of("package P {\n  this is not sysml\n}\n");
        assert!(hints.is_empty(), "{:?}", labels(&hints));
    }
}
