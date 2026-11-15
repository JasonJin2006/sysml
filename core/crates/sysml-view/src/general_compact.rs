use std::collections::BTreeMap;

use crate::layered::{Input, Layered, Pin, SEP};

const WIDTH: f32 = 1100.0;
const ROW_GAP: f32 = 64.0;

pub(super) fn compact(layout: &mut Layered, edges: &[Input]) {
    if layout.width <= WIDTH {
        return;
    }
    if pack_components(layout, edges) {
        dimensions(layout);
        return;
    }
    let mut ranks: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, node) in layout.nodes.iter().enumerate() {
        if node.real.is_some() {
            ranks.entry(node.rank).or_default().push(index);
        }
    }
    let mut shift = 0.0;
    for members in ranks.values_mut() {
        members.sort_by(|&a, &b| {
            layout.nodes[a]
                .x
                .total_cmp(&layout.nodes[b].x)
                .then(a.cmp(&b))
        });
        let top = members
            .iter()
            .map(|&i| layout.nodes[i].y)
            .fold(f32::MAX, f32::min);
        let bottom = members
            .iter()
            .map(|&i| layout.nodes[i].y + layout.nodes[i].h)
            .fold(0.0, f32::max);
        let right = members
            .iter()
            .map(|&i| layout.nodes[i].x + layout.nodes[i].w)
            .fold(0.0, f32::max);
        if right <= WIDTH {
            for &index in members.iter() {
                layout.nodes[index].y += shift;
            }
            continue;
        }

        let mut group_of: BTreeMap<usize, usize> = members.iter().map(|&i| (i, i)).collect();
        for edge in edges.iter().filter(|edge| edge.same_rank) {
            let Some((&a, &b)) = group_of.get(&edge.from).zip(group_of.get(&edge.to)) else {
                continue;
            };
            if a != b {
                for group in group_of.values_mut() {
                    if *group == b {
                        *group = a;
                    }
                }
            }
        }
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut positions = BTreeMap::new();
        for &index in members.iter() {
            let group = group_of[&index];
            let next = groups.len();
            let position = *positions.entry(group).or_insert_with(|| {
                groups.push(Vec::new());
                next
            });
            groups[position].push(index);
        }
        let mut rows: Vec<Vec<usize>> = Vec::new();
        let mut row = Vec::new();
        let mut row_width = 0.0;
        for group in groups {
            let width = group.iter().map(|&i| layout.nodes[i].w).sum::<f32>()
                + SEP * group.len().saturating_sub(1) as f32;
            let separator = if row.is_empty() { 0.0 } else { SEP };
            if !row.is_empty() && row_width + separator + width > WIDTH {
                rows.push(std::mem::take(&mut row));
                row_width = 0.0;
            }
            if !row.is_empty() {
                row_width += SEP;
            }
            row_width += width;
            row.extend(group);
        }
        if !row.is_empty() {
            rows.push(row);
        }
        let mut y = top + shift;
        let mut new_bottom = y;
        for row in rows {
            let height = row.iter().map(|&i| layout.nodes[i].h).fold(0.0, f32::max);
            let mut x = 0.0;
            for index in row {
                let node = &mut layout.nodes[index];
                node.x = x;
                node.y = y + (height - node.h) / 2.0;
                x += node.w + SEP;
            }
            new_bottom = y + height;
            y = new_bottom + ROW_GAP;
        }
        shift = (new_bottom - bottom).max(shift);
    }
    dimensions(layout);
}

// Keep unrelated subgraphs intact instead of folding each of their ranks independently.
fn pack_components(layout: &mut Layered, edges: &[Input]) -> bool {
    let real: BTreeMap<_, _> = layout
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| node.real.map(|id| (id, i)))
        .collect();
    let mut parent: Vec<_> = (0..layout.nodes.len()).collect();
    fn root(parent: &[usize], mut i: usize) -> usize {
        while parent[i] != i {
            i = parent[i];
        }
        i
    }
    for edge in edges {
        if let (Some(&a), Some(&b)) = (real.get(&edge.from), real.get(&edge.to)) {
            let a = root(&parent, a);
            let b = root(&parent, b);
            parent[b] = a;
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &i in real.values() {
        groups.entry(root(&parent, i)).or_default().push(i);
    }
    if groups.len() <= 1 {
        return false;
    }
    let pieces: Vec<_> = groups
        .values()
        .map(|nodes| {
            let local: BTreeMap<_, _> = nodes
                .iter()
                .enumerate()
                .map(|(i, &n)| (layout.nodes[n].real.expect("real node"), i))
                .collect();
            let sizes: Vec<_> = nodes
                .iter()
                .map(|&i| (layout.nodes[i].w, layout.nodes[i].h))
                .collect();
            let inputs: Vec<_> = edges
                .iter()
                .filter_map(|e| {
                    Some(Input {
                        index: e.index,
                        from: *local.get(&e.from)?,
                        to: *local.get(&e.to)?,
                        kind: e.kind,
                        same_rank: e.same_rank,
                    })
                })
                .collect();
            let mut parents = vec![None; nodes.len()];
            for e in &inputs {
                if e.kind == crate::graph::Arrow::Composite {
                    parents[e.to] = Some(e.from);
                }
            }
            crate::layered::build(
                &sizes,
                &inputs,
                &(0..nodes.len()).collect::<Vec<_>>(),
                &parents,
            )
        })
        .collect();
    if pieces.iter().any(|p| p.width > WIDTH) {
        return false;
    }
    let mut x = 0.0;
    let mut y = 0.0;
    let mut row_height: f32 = 0.0;
    for (nodes, piece) in groups.values().zip(&pieces) {
        if x > 0.0 && x + piece.width > WIDTH {
            x = 0.0;
            y += row_height + ROW_GAP;
            row_height = 0.0;
        }
        for node in piece.nodes.iter().filter(|n| n.real.is_some()) {
            let target = &mut layout.nodes[nodes[node.real.expect("real node")]];
            target.x = x + node.x;
            target.y = y + node.y;
            target.rank = node.rank;
        }
        x += piece.width + SEP;
        row_height = row_height.max(piece.height);
    }
    true
}

pub(super) fn pin(layout: &mut Layered, pins: &[Pin]) {
    for pin in pins {
        let Some(node) = layout
            .nodes
            .iter_mut()
            .find(|node| node.real == Some(pin.node))
        else {
            continue;
        };
        node.x = pin.x;
        node.y = pin.y;
        if let Some(width) = pin.w {
            node.w = width;
        }
        if let Some(height) = pin.h {
            node.h = height;
        }
    }
    dimensions(layout);
}

fn dimensions(layout: &mut Layered) {
    layout.width = layout
        .nodes
        .iter()
        .filter(|node| node.real.is_some())
        .map(|node| node.x + node.w)
        .fold(0.0, f32::max);
    layout.height = layout
        .nodes
        .iter()
        .filter(|node| node.real.is_some())
        .map(|node| node.y + node.h)
        .fold(0.0, f32::max);
}
