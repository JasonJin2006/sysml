//! The sequence view: parts as lifelines, messages in the order they are written.
//!
//! A send names what it carries and where it goes; nothing else in the model states an exchange
//! between two parts, and no view showed one. This one draws each part that takes part in a
//! message as a lifeline, and each `send` as an arrow from the part that writes it to the part it
//! names, ordered by the line it is written on. The spec's own definition is that the time axis is
//! vertical and increases downwards, so the order is the source order.
//!
//! What decides the sender: the nearest enclosing `part` usage. A `send` written in a package or in
//! a bare action definition has no lifeline to leave from, so it is skipped rather than attached to
//! whatever element happens to be above it — the same rule the state view applies to a transition
//! whose end names nothing drawn.
//!
//! `accept` is not drawn. Pairing a `send` with an `accept` is a relationship the model does not
//! state: they are two independent statements, and choosing which accept answers which send would
//! be the view inventing the very fact it exists to show.

use std::collections::BTreeMap;

use sysml_model::{Element, Kind};

use crate::graph::{bare_end, Arrow, Edge, Graph, View};

pub struct Sequence {
    pub graph: Graph,
}

/// Parts, and the messages that run between them.
pub fn sequence(roots: &[Element]) -> Sequence {
    let mut g = Graph::new(View::Sequence);

    // Every part usage in the model, by name. A part usage is a lifeline candidate; a part *def*
    // is a type, and two usages of one type are two lifelines.
    let mut parts: BTreeMap<&str, &Element> = BTreeMap::new();
    fn collect<'a>(el: &'a Element, parts: &mut BTreeMap<&'a str, &'a Element>) {
        if el.kind == Kind::PartUsage && !el.name.is_empty() {
            parts.entry(el.name.as_str()).or_insert(el);
        }
        for child in &el.children {
            collect(child, parts);
        }
    }
    for root in roots {
        collect(root, &mut parts);
    }

    // Sends, with the part each is written inside.
    struct Sent<'a> {
        from: &'a str,
        to: &'a str,
        label: String,
        line: u32,
    }
    let mut sent: Vec<Sent> = Vec::new();
    fn walk<'a>(el: &'a Element, inside: Option<&'a str>, out: &mut Vec<Sent<'a>>) {
        let here = if el.kind == Kind::PartUsage && !el.name.is_empty() {
            Some(el.name.as_str())
        } else {
            inside
        };
        if el.kind == Kind::SendActionUsage {
            if let (Some(from), Some(addressed)) = (here, el.addressed.as_deref()) {
                let to = bare_end(addressed);
                if !to.is_empty() {
                    out.push(Sent {
                        from,
                        to,
                        label: el.to.clone().unwrap_or_else(|| el.name.clone()),
                        line: el.line,
                    });
                }
            }
        }
        for child in &el.children {
            walk(child, here, out);
        }
    }
    for root in roots {
        walk(root, None, &mut sent);
    }

    // Both ends have to be parts before either becomes a lifeline. A send whose receiver is not a
    // part is not drawable, and drawing the sender alone would put a lifeline in the picture that
    // nothing speaks to or from.
    sent.retain(|message| parts.contains_key(message.from) && parts.contains_key(message.to));

    // Only parts that take part in a message are drawn: a model with forty parts and one exchange
    // should show two lifelines, not forty.
    let mut drawn: Vec<&Element> = Vec::new();
    for message in &sent {
        for name in [message.from, message.to] {
            if let Some(part) = parts.get(name) {
                if !drawn.iter().any(|p| p.id == part.id) {
                    drawn.push(part);
                }
            }
        }
    }
    drawn.sort_by_key(|p| p.id);

    let mut node_of: BTreeMap<&str, usize> = BTreeMap::new();
    for part in &drawn {
        let index = g.push_element(part, &part.name, &part.name, None);
        node_of.insert(part.name.as_str(), index);
    }

    // Source order is time order, and the layout reads the arrows in the order they were pushed,
    // so the sort happens here rather than in the geometry.
    sent.sort_by_key(|message| message.line);

    for message in &sent {
        let (Some(&from), Some(&to)) = (node_of.get(message.from), node_of.get(message.to)) else {
            continue;
        };
        g.edges.push(Edge {
            from,
            to,
            arrow: Arrow::Transition,
            label: Some(message.label.clone()),
            from_port: None,
            to_port: None,
        });
    }

    Sequence { graph: g }
}
