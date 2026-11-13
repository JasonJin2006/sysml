//! What colour a box is drawn in.
//!
//! Every colour here encodes something: the kind of element the box is. A diagram is scanned by
//! colour before any label is read — that is what makes a two-hundred-element picture legible —
//! so a colour that encodes nothing is not added.
//!
//! The palette is taken from the reference renderings collected under `research/syside-diagrams/`,
//! which is not published; the values are copied here so a picture can be compared against them
//! without carrying the third-party files.

/// The colours of one box: its header band, its body, and the rule under the title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxStyle {
    /// The band behind the stereotype and name.
    pub header: &'static str,
    /// The body, where attributes and compartments are drawn.
    pub body: &'static str,
    /// The border, and the rule between the title and the body.
    pub border: &'static str,
}

impl BoxStyle {
    /// A box whose kind has no colour of its own: a name and a border, nothing more.
    pub const PLAIN: BoxStyle = BoxStyle {
        header: "#FFFFFF",
        body: "#FFFFFF",
        border: "#666666",
    };
}

/// The style for an element, from the stereotype the projection already carries.
///
/// The stereotype is `«part def»`, `«requirement»`, `«state»` and so on — the kind as the notation
/// writes it, which is exactly the distinction a reader scans for. Grouping happens here rather
/// than at each call site so a new kind gets a colour in one place.
pub fn for_stereotype(stereotype: &str) -> BoxStyle {
    let kind = stereotype
        .trim()
        .trim_start_matches('«')
        .trim_end_matches('»')
        .trim();

    const PART_DEF: BoxStyle = BoxStyle {
        header: "#88B7E3",
        body: "#F7F9FF",
        border: "#5D9DDB",
    };
    const PART_USE: BoxStyle = BoxStyle {
        header: "#BDDCFF",
        body: "#FFFFFF",
        border: "#5D9DDB",
    };
    const ENUM_DEF: BoxStyle = BoxStyle {
        header: "#FFE066",
        body: "#FFFDF0",
        border: "#F5C518",
    };
    const REQUIREMENT: BoxStyle = BoxStyle {
        header: "#F5B272",
        body: "#FFF5EB",
        border: "#E08A3C",
    };
    const CONSTRAINT: BoxStyle = BoxStyle {
        header: "#F4969D",
        body: "#FFF7F9",
        border: "#E06874",
    };
    const STATE: BoxStyle = BoxStyle {
        header: "#80D6D6",
        body: "#F4FCFC",
        border: "#38C4C4",
    };
    const ACTION: BoxStyle = BoxStyle {
        header: "#80D6A7",
        body: "#F6FCF8",
        border: "#43C479",
    };
    const METADATA: BoxStyle = BoxStyle {
        header: "#FFFDE7",
        body: "#FFFDE7",
        border: "#C9A227",
    };
    const COMMENT: BoxStyle = BoxStyle {
        header: "#FFF4E7",
        body: "#FFF4E7",
        border: "#C08A4A",
    };
    const PACKAGE: BoxStyle = BoxStyle {
        header: "#E8EAF6",
        body: "#F9FAFF",
        border: "#C5CAE9",
    };
    const ITEM: BoxStyle = BoxStyle {
        header: "#F3CC7A",
        body: "#FDFAEE",
        border: "#F0AA30",
    };

    match kind {
        "part def" | "part definition" => PART_DEF,
        "part" => PART_USE,
        "port def" | "port definition" => PART_DEF,
        "interface def" | "connection def" | "allocation def" => PART_DEF,
        "interface" | "connection" | "allocation" => PART_USE,
        "enum def" | "enumeration def" => ENUM_DEF,
        "enum" | "enumeration" => ENUM_DEF,
        "requirement def" => REQUIREMENT,
        "requirement" => REQUIREMENT,
        "constraint def" | "require constraint" | "assume constraint" => CONSTRAINT,
        "state def" => STATE,
        "state" => STATE,
        "action def" => ACTION,
        "action" | "accept action" | "send action" | "perform" => ACTION,
        "metadata def" | "metadata" => METADATA,
        "comment" | "doc" | "rep" => COMMENT,
        "package" | "namespace" | "library package" => PACKAGE,
        "item def" => ITEM,
        "item" => ITEM,
        // Everything else keeps the plain box: an attribute, a subject, a satisfy. Colouring them
        // would say they are more alike each other than they are unlike the groups above.
        _ => BoxStyle::PLAIN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_definition_and_a_usage_of_the_same_construct_differ() {
        let def = for_stereotype("«part def»");
        let usage = for_stereotype("«part»");
        assert_ne!(def.header, usage.header);
        assert_eq!(def.border, usage.border, "one family, one border");
    }

    #[test]
    fn different_constructs_get_different_colours() {
        let fills = [
            for_stereotype("«part def»").header,
            for_stereotype("«enum def»").header,
            for_stereotype("«requirement def»").header,
            for_stereotype("«state»").header,
            for_stereotype("«action»").header,
            for_stereotype("«metadata»").header,
        ];
        let unique: std::collections::BTreeSet<_> = fills.iter().collect();
        assert_eq!(
            unique.len(),
            fills.len(),
            "each construct has its own: {fills:?}"
        );
    }

    #[test]
    fn a_kind_outside_the_table_keeps_the_plain_box() {
        assert_eq!(for_stereotype("«attribute»"), BoxStyle::PLAIN);
        assert_eq!(for_stereotype("«subject»"), BoxStyle::PLAIN);
        assert_eq!(for_stereotype(""), BoxStyle::PLAIN);
    }
}
