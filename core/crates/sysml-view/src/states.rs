//! The state transition view: states as boxes, transitions as directed edges.
//!
//! The general view draws a state machine as three sibling boxes hanging off the machine, with no
//! edge between them: `state off`, `state on` and `transition off then on` all become children of
//! `Power`, so the one fact the diagram is for — `off` goes to `on` — is the one fact it does not
//! show. This view keeps the machine as a frame, puts its states inside, and draws each transition
//! as an edge from its source to its target.
//!
//! What is drawn is what the model states, and nothing else:
//!
//!   * the states directly inside the machine (`state off;`), plus pseudo-states (`first`);
//!   * transitions whose two ends name those states. A transition whose ends name something else —
//!     a state one level down, or a name that did not resolve — is left out rather than drawn to
//!     a box that does not exist;
//!   * a transition's guard (or trigger, when there is no guard) as the edge label.
//!
//! A state that itself contains states is drawn as a box here, not as a nested frame. The nested
//! machine is a machine of its own and gets its own frame when the walk reaches it; drawing both
//! would put a frame inside a circle of boxes, which is the layout problem this view is not
//! solving yet.

use std::collections::BTreeMap;

use sysml_model::{Element, Kind};

use crate::graph::bare_end;

use crate::graph::{Arrow, Edge, Graph, View};

/// One group: the element drawn as a frame, and the behaviour nodes inside it.
pub struct Group {
    /// Index into `Graph::nodes`, the frame itself.
    pub node: usize,
    /// Nodes drawn inside this frame, in model order.
    pub members: Vec<usize>,
}

pub struct Behaviour {
    pub graph: Graph,
    pub groups: Vec<Group>,
}

/// A state *usage* is a state; a `state def` is a definition of one.
///
/// The distinction decides what a machine is. A package that contains `state def Power` is not a
/// state machine — it contains one — so counting a StateDef as a state made every enclosing
/// package a frame with the machine's name inside it.
fn is_state_node(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::StateUsage | Kind::PseudoState | Kind::ControlNode
    )
}

fn is_action_node(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::ActionUsage | Kind::ControlNode | Kind::PseudoState
    )
}

/// State machines: states inside frames, transitions as edges.
pub fn states(roots: &[Element]) -> Behaviour {
    project(roots, View::StateTransition, is_state_node)
}

/// Action flows: actions inside frames, successions as edges.
pub fn actions(roots: &[Element]) -> Behaviour {
    project(roots, View::ActionFlow, is_action_node)
}

fn project(roots: &[Element], view: View, is_node: fn(&Kind) -> bool) -> Behaviour {
    let mut g = Graph::new(view);

    fn walk(el: &Element, g: &mut Graph, is_node: fn(&Kind) -> bool) {
        if el.children.iter().any(|c| is_node(&c.kind)) {
            let frame = g.push_element(el, &el.name, &el.name, None);
            g.nodes[frame].is_frame = true;

            // Names, so a succession's ends can be matched against what is actually drawn.
            let members: Vec<&Element> = el.children.iter().filter(|c| is_node(&c.kind)).collect();
            let mut node_of: BTreeMap<&str, usize> = BTreeMap::new();
            for member in &members {
                let index = g.push_element(member, &member.name, &member.name, Some(frame));
                node_of.insert(member.name.as_str(), index);
            }

            for child in &el.children {
                if child.kind != Kind::Transition {
                    continue;
                }
                let (Some(from_text), Some(to_text)) = (&child.from, &child.to) else {
                    continue;
                };
                let (Some(&from), Some(&to)) = (
                    node_of.get(bare_end(from_text)),
                    node_of.get(bare_end(to_text)),
                ) else {
                    continue;
                };
                g.edges.push(Edge {
                    from,
                    to,
                    arrow: Arrow::Transition,
                    // The guard is the condition the transition is taken on; a trigger without a
                    // guard is the next most useful thing to read on the edge.
                    label: child
                        .constraint
                        .clone()
                        .or_else(|| child.trigger_text.clone()),
                    from_port: None,
                    to_port: None,
                });
            }
            // A behaviour node that is itself a group is drawn as a box above and walked into
            // below, so a nested machine or action definition gets its own frame.
            for child in &el.children {
                if is_node(&child.kind) && child.children.iter().any(|c| is_node(&c.kind)) {
                    walk(child, g, is_node);
                }
            }
            return;
        }
        if matches!(
            el.kind,
            Kind::Import | Kind::Satisfy | Kind::Connection | Kind::Unsupported
        ) {
            return;
        }
        for child in &el.children {
            walk(child, g, is_node);
        }
    }

    for root in roots {
        walk(root, &mut g, is_node);
    }

    Behaviour {
        groups: groups_of(&g),
        graph: g,
    }
}

/// Rebuild the group frames from a projected graph.
///
/// The canvas and the export receive a `Graph`, not the projection's own structs, so the one
/// derivation serves both and they cannot describe different trees.
pub fn groups_of(g: &Graph) -> Vec<Group> {
    let mut groups = Vec::new();
    for (index, node) in g.nodes.iter().enumerate() {
        if !node.is_frame {
            continue;
        }
        let members = (0..g.nodes.len())
            .filter(|&child| g.nodes[child].parent == Some(index))
            .collect();
        groups.push(Group {
            node: index,
            members,
        });
    }
    groups
}
