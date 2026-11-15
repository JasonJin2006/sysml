//! Behavior projections preserve containment, node symbols and transition clauses.

use std::collections::BTreeMap;

use sysml_model::{Element, Kind};

use crate::graph::{bare_end, Arrow, Edge, Graph, View};

/// One behavior frame and its direct members, including nested frames.
pub struct Group {
    pub node: usize,
    pub members: Vec<usize>,
}

pub struct Behaviour {
    pub graph: Graph,
    pub groups: Vec<Group>,
}

fn is_state_node(kind: &Kind) -> bool {
    matches!(kind, Kind::StateUsage | Kind::ControlNode)
}

fn is_action_node(kind: &Kind) -> bool {
    matches!(kind, Kind::ActionUsage | Kind::ControlNode)
}

pub fn states(roots: &[Element]) -> Behaviour {
    project(roots, View::StateTransition, is_state_node)
}

pub fn actions(roots: &[Element]) -> Behaviour {
    project(roots, View::ActionFlow, is_action_node)
}

pub fn frame_title(node: &crate::graph::Node) -> f32 {
    22.0 + if node.attributes.is_empty() {
        0.0
    } else {
        14.0
    } + node.attributes.len() as f32 * 16.0
}

fn state_rows(el: &Element) -> Vec<String> {
    el.children
        .iter()
        .filter(|child| child.kind == Kind::StateAction)
        .filter_map(|child| {
            let role = child.clause.as_deref()?;
            let behavior = child
                .to
                .as_deref()
                .or(child.performs.as_deref())
                .unwrap_or(&child.name);
            let behavior = if behavior == role || behavior == "<anonymous>" {
                ""
            } else {
                behavior
            };
            if behavior.is_empty() && child.type_ref.is_none() && child.children.is_empty() {
                return None;
            }
            let mut row = role.to_string();
            if !behavior.is_empty() {
                row.push(' ');
                row.push_str(behavior);
            }
            if let Some(ty) = &child.type_ref {
                row.push_str(" : ");
                row.push_str(ty);
            }
            Some(row)
        })
        .collect()
}

fn transition_label(el: &Element) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(trigger) = el.trigger_text.as_deref().filter(|text| !text.is_empty()) {
        let mut text = trigger.to_string();
        if let Some(via) = el.addressed.as_deref().filter(|text| !text.is_empty()) {
            text.push_str(" via ");
            text.push_str(via);
        }
        parts.push(text);
    }
    if let Some(guard) = el.constraint.as_deref().filter(|text| !text.is_empty()) {
        parts.push(format!("[{guard}]"));
    }
    if let Some(effect) = el.effect_text.as_deref().filter(|text| !text.is_empty()) {
        parts.push(format!("/ {effect}"));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn project(roots: &[Element], view: View, is_node: fn(&Kind) -> bool) -> Behaviour {
    fn add_edge(g: &mut Graph, from: usize, to: usize, label: Option<String>) {
        g.edges.push(Edge {
            from,
            to,
            arrow: Arrow::Transition,
            label,
            from_port: None,
            to_port: None,
        });
    }

    fn fill_group(el: &Element, frame: usize, g: &mut Graph, is_node: fn(&Kind) -> bool) {
        g.nodes[frame].is_frame = true;
        g.nodes[frame].attributes = state_rows(el);
        let mut node_of: BTreeMap<&str, usize> = BTreeMap::new();
        for member in el.children.iter().filter(|child| is_node(&child.kind)) {
            let index = g.push_element(member, &member.name, &member.name, Some(frame));
            g.nodes[index].attributes = state_rows(member);
            g.nodes[index].constraint = member.constraint.clone();
            node_of.insert(member.name.as_str(), index);
        }

        for member in el.children.iter().filter(|child| is_node(&child.kind)) {
            if member.children.iter().any(|child| is_node(&child.kind)) {
                fill_group(member, node_of[member.name.as_str()], g, is_node);
            }
        }

        if g.view == View::StateTransition
            && !node_of.contains_key("done")
            && el
                .children
                .iter()
                .any(|child| child.kind == Kind::Transition && child.to.as_deref() == Some("done"))
        {
            let terminal = g.push_terminal(frame);
            node_of.insert("done", terminal);
        }

        // Empty entry followed immediately by a succession denotes the state machine's entry.
        let entry = if g.view == View::StateTransition {
            el.children.windows(2).find_map(|pair| {
                let (entry, succession) = (&pair[0], &pair[1]);
                (entry.kind == Kind::StateAction
                    && entry.name == "entry"
                    && matches!(entry.clause.as_deref(), None | Some("entry"))
                    && entry.to.is_none()
                    && entry.type_ref.is_none()
                    && entry.children.is_empty()
                    && succession.kind == Kind::Transition
                    && succession.name == "succession"
                    && succession
                        .from
                        .as_deref()
                        .is_some_and(|source| source == "entry" || source == el.name))
                .then_some((entry, succession.id))
            })
        } else {
            None
        };
        let initial = entry.map(|(entry, succession)| {
            let node = g.push_element(entry, "entry", "entry", Some(frame));
            g.nodes[node].symbol = crate::graph::NodeSymbol::Initial;
            (succession, node)
        });

        for child in &el.children {
            if child.kind == Kind::Transition {
                let Some(to_text) = &child.to else {
                    continue;
                };
                let from = match initial {
                    Some((succession, node)) if succession == child.id => Some(node),
                    _ => child
                        .from
                        .as_deref()
                        .and_then(|text| node_of.get(bare_end(text)).copied()),
                };
                if let (Some(from), Some(&to)) = (from, node_of.get(bare_end(to_text))) {
                    add_edge(g, from, to, transition_label(child));
                }
            }
        }
    }

    fn walk(el: &Element, g: &mut Graph, is_node: fn(&Kind) -> bool) {
        let behavior_owner = match g.view {
            View::StateTransition => matches!(el.kind, Kind::StateDef | Kind::StateUsage),
            View::ActionFlow => matches!(el.kind, Kind::ActionDef | Kind::ActionUsage),
            _ => false,
        };
        if el.children.iter().any(|child| {
            is_node(&child.kind) && (child.kind != Kind::ControlNode || behavior_owner)
        }) {
            let frame = g.push_element(el, &el.name, &el.name, None);
            fill_group(el, frame, g, is_node);
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

    let mut graph = Graph::new(view);
    for root in roots {
        walk(root, &mut graph, is_node);
    }
    Behaviour {
        groups: groups_of(&graph),
        graph,
    }
}

/// Rebuild groups from the same containment relation consumed by every renderer.
pub fn groups_of(g: &Graph) -> Vec<Group> {
    g.nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.is_frame)
        .map(|(index, _)| Group {
            node: index,
            members: (0..g.nodes.len())
                .filter(|&child| g.nodes[child].parent == Some(index))
                .collect(),
        })
        .collect()
}
