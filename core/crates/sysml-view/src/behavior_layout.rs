use std::collections::{BTreeMap, HashMap};

use crate::graph::{Arrow, Graph, Node, NodeSymbol};
use crate::layered::{self, Input};
use crate::states::{self, Group};

use super::{
    boundary_slot, facing_sides, node_size, reroute_general, Attachment, Rect, StateGeometry,
};

const PAD: f32 = 28.0;
const GAP: f32 = 48.0;

fn group_layout(g: &Graph, groups: &[Group], frame: usize) -> StateGeometry {
    let group = groups
        .iter()
        .find(|group| group.node == frame)
        .expect("group frame");
    let nested: HashMap<usize, StateGeometry> = group
        .members
        .iter()
        .filter(|&&i| g.nodes[i].is_frame)
        .map(|&i| (i, group_layout(g, groups, i)))
        .collect();
    let sizes: Vec<_> = group
        .members
        .iter()
        .map(|&i| {
            nested
                .get(&i)
                .map(|p| (p.width, p.height))
                .unwrap_or_else(|| node_size(&g.nodes[i]))
        })
        .collect();
    let local: HashMap<_, _> = group
        .members
        .iter()
        .enumerate()
        .map(|(i, &node)| (node, i))
        .collect();
    let inputs: Vec<_> = g
        .edges
        .iter()
        .enumerate()
        .filter_map(|(index, edge)| {
            Some(Input {
                index,
                from: *local.get(&edge.from)?,
                to: *local.get(&edge.to)?,
                kind: Arrow::Transition,
                same_rank: false,
            })
        })
        .collect();
    let order: Vec<_> = (0..sizes.len()).collect();
    let layout = layered::build(&sizes, &inputs, &order, &vec![None; sizes.len()]);
    let top = layout
        .nodes
        .iter()
        .filter(|n| n.real.is_some())
        .map(|n| n.y)
        .fold(f32::MAX, f32::min);
    let header = states::frame_title(&g.nodes[frame]);
    let mut piece = StateGeometry {
        boxes: Vec::new(),
        edges: BTreeMap::new(),
        frames: Vec::new(),
        width: node_size(&g.nodes[frame]).0 + 2.0 * PAD,
        height: header + 2.0 * PAD,
    };
    for node in layout.nodes.iter().filter(|n| n.real.is_some()) {
        let i = group.members[node.real.expect("real node")];
        let x = node.x + PAD;
        let y = node.y - top + PAD + header;
        piece.width = piece.width.max(x + node.w + PAD);
        piece.height = piece.height.max(y + node.h + PAD);
        if let Some(child) = nested.get(&i) {
            piece.boxes.extend(
                child
                    .boxes
                    .iter()
                    .map(|&(i, cx, cy, w, h)| (i, x + cx, y + cy, w, h)),
            );
            piece.frames.extend(
                child
                    .frames
                    .iter()
                    .map(|&(i, cx, cy, w, h)| (i, x + cx, y + cy, w, h)),
            );
        } else {
            piece.boxes.push((i, x, y, node.w, node.h));
        }
    }
    piece
        .frames
        .insert(0, (frame, 0.0, 0.0, piece.width, piece.height));
    piece
}

fn ancestor(g: &Graph, frame: usize, mut node: usize) -> bool {
    while let Some(parent) = g.nodes[node].parent {
        if parent == frame {
            return true;
        }
        node = parent;
    }
    false
}

fn slot(node: &Node, rect: Rect, side: u8, ratio: f32) -> ((f32, f32), (f32, f32)) {
    let (mut point, _) = boundary_slot(rect, side, ratio);
    let cx = (rect.x0 + rect.x1) / 2.0;
    let cy = (rect.y0 + rect.y1) / 2.0;
    let dx = point.0 - cx;
    let dy = point.1 - cy;
    let sx = (rect.x1 - rect.x0) / 2.0;
    let sy = (rect.y1 - rect.y0) / 2.0;
    let scale = match node.symbol {
        NodeSymbol::Initial | NodeSymbol::Done => {
            1.0 / ((dx / sx).powi(2) + (dy / sy).powi(2)).sqrt()
        }
        NodeSymbol::Decision | NodeSymbol::Merge => 1.0 / (dx.abs() / sx + dy.abs() / sy),
        _ => 1.0,
    };
    point = (cx + dx * scale, cy + dy * scale);
    let lead = match side {
        0 => (point.0, point.1 - 20.0),
        1 => (point.0, point.1 + 20.0),
        2 => (point.0 - 20.0, point.1),
        _ => (point.0 + 20.0, point.1),
    };
    (point, lead)
}

pub(super) fn geometry(g: &Graph, groups: &[Group]) -> StateGeometry {
    let mut result = StateGeometry {
        boxes: Vec::new(),
        edges: BTreeMap::new(),
        frames: Vec::new(),
        width: 2.0 * GAP,
        height: GAP,
    };
    for group in groups.iter().filter(|group| {
        !g.nodes[group.node]
            .parent
            .is_some_and(|p| g.nodes[p].is_frame)
    }) {
        let piece = group_layout(g, groups, group.node);
        let y = result.height;
        result.boxes.extend(
            piece
                .boxes
                .into_iter()
                .map(|(i, x, py, w, h)| (i, x + GAP, y + py, w, h)),
        );
        result.frames.extend(
            piece
                .frames
                .into_iter()
                .map(|(i, x, py, w, h)| (i, x + GAP, y + py, w, h)),
        );
        result.width = result.width.max(piece.width + 2.0 * GAP);
        result.height += piece.height + GAP;
    }
    let rects: HashMap<usize, Rect> = result
        .boxes
        .iter()
        .chain(result.frames.iter())
        .map(|&(i, x, y, w, h)| {
            (
                i,
                Rect {
                    x0: x,
                    y0: y,
                    x1: x + w,
                    y1: y + h,
                },
            )
        })
        .collect();
    let mut attachments: HashMap<(usize, u8), Vec<Attachment>> = HashMap::new();
    for (index, edge) in g.edges.iter().enumerate().filter(|(_, e)| e.from != e.to) {
        let (Some(a), Some(b)) = (rects.get(&edge.from), rects.get(&edge.to)) else {
            continue;
        };
        let (from, to) = facing_sides(a, b);
        for (node, side, source, other) in [(edge.from, from, true, b), (edge.to, to, false, a)] {
            attachments
                .entry((node, side))
                .or_default()
                .push(Attachment {
                    edge: index,
                    source,
                    position: if side < 2 {
                        (other.x0 + other.x1) / 2.0
                    } else {
                        (other.y0 + other.y1) / 2.0
                    },
                });
        }
    }
    let mut ends = HashMap::new();
    for ((node, side), mut points) in attachments {
        points.sort_by(|a, b| a.position.total_cmp(&b.position).then(a.edge.cmp(&b.edge)));
        let count = points.len();
        for (i, at) in points.into_iter().enumerate() {
            ends.insert(
                (at.edge, at.source),
                slot(
                    &g.nodes[node],
                    rects[&node],
                    side,
                    (i + 1) as f32 / (count + 1) as f32,
                ),
            );
        }
    }
    let bounds = Rect {
        x0: 0.0,
        y0: 0.0,
        x1: result.width + GAP,
        y1: result.height + GAP,
    };
    for (index, edge) in g.edges.iter().enumerate() {
        let Some(source) = rects.get(&edge.from) else {
            continue;
        };
        let points = if edge.from == edge.to {
            let reach = 24.0;
            let a = source.y0 + (source.y1 - source.y0) * 0.25;
            let b = source.y0 + (source.y1 - source.y0) * 0.75;
            vec![
                (source.x1, a),
                (source.x1 + reach, a),
                (source.x1 + reach, b),
                (source.x1, b),
            ]
        } else {
            let (Some(&(from, lead)), Some(&(to, tail))) =
                (ends.get(&(index, true)), ends.get(&(index, false)))
            else {
                continue;
            };
            let obstacles: Vec<_> = rects
                .iter()
                .filter(|(i, _)| {
                    !g.nodes[**i].is_frame
                        || (**i != edge.from
                            && **i != edge.to
                            && !ancestor(g, **i, edge.from)
                            && !ancestor(g, **i, edge.to))
                })
                .map(|(_, r)| *r)
                .collect();
            reroute_general(from, lead, to, tail, &[], &obstacles, bounds)
        };
        for &(x, y) in &points {
            result.width = result.width.max(x + GAP);
            result.height = result.height.max(y + GAP);
        }
        result.edges.insert(index, points);
    }
    result
}
