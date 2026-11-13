//! The package structure view: the model's namespace tree, with packages drawn as containers.
//!
//! The general view drops packages entirely (`visible` filters `Kind::Package` out) so that a
//! diagram of a system is not buried in namespace boxes. That is right for reading a system, and
//! wrong for the question a person asks while editing one: "where did this land, and what else is
//! in that package?" With the packages gone there is nothing to answer it — a model with
//! `Top::Sub::Deep` draws exactly the same picture as one where every element sits at the root.
//!
//! So this view keeps the namespace tree and draws it as nested frames, one box per package, with
//! its members inside. It is the structural counterpart of the traceability view: that one asks
//! "is this requirement met", this one asks "is this element in the right place".
//!
//! Imports are not drawn. An `import` is a visibility statement, not containment, and drawing it
//! as a line from one package to another would make the picture claim a structural relationship
//! the namespace tree does not have.

use std::collections::BTreeMap;

use sysml_model::{Element, Kind};

use crate::graph::{Graph, Node, View};

/// One frame in the tree: a package, or the root that holds the model's top-level elements.
pub struct Frame {
    /// Index into `Graph::nodes`. `usize::MAX` for the synthetic root, which is not an element.
    pub node: usize,
    /// Nesting depth, 0 for the root.
    pub depth: usize,
    /// The package node this frame sits inside, if any.
    pub parent: Option<usize>,
    /// Nodes drawn directly inside this frame, in model order.
    pub members: Vec<usize>,
    /// Frames nested inside this one, in model order.
    pub children: Vec<usize>,
}

pub struct Packages {
    pub graph: Graph,
    /// Every frame, in pre-order. Index 0 is the synthetic root.
    pub frames: Vec<Frame>,
}

/// The namespace tree, packages as containers.
pub fn packages(roots: &[Element]) -> Packages {
    let mut g = Graph::new(View::Packages);

    /// `frame` is the package node the element sits in; `usize::MAX` is the top level.
    fn walk(el: &Element, frame: usize, g: &mut Graph) {
        // Relationship kinds are claims, not members; `Unsupported` is a front-end marker that
        // nothing should draw. Same filter the general view applies, minus `Package`.
        if matches!(
            el.kind,
            Kind::Import | Kind::Satisfy | Kind::Connection | Kind::Unsupported
        ) {
            return;
        }
        let parent = if frame == usize::MAX {
            None
        } else {
            Some(frame)
        };
        let node = g.push_element(el, &el.name, &el.name, parent);
        // Only a package's children are drawn. A type's features are owned by the type, not by
        // the package, and drawing `energy` next to `Battery` would claim both sit in the same
        // namespace — the question this view answers is "which package", and one level down is
        // what makes that answer readable.
        if el.kind == Kind::Package {
            for child in &el.children {
                walk(child, node, g);
            }
        }
    }

    for root in roots {
        walk(root, usize::MAX, &mut g);
    }

    // Frames are rebuilt from the graph instead of being accumulated during the walk: the canvas
    // only ever receives the graph, and one derivation means the two cannot describe different
    // trees.
    Packages {
        frames: frames_of(&g),
        graph: g,
    }
}

/// Boxes for the tree, in content coordinates.
///
/// Same contract as the other geometry functions: the export and the canvas both call this, so the
/// two draw one picture. A frame is sized by what it contains, so the pass runs bottom-up —
/// children first, then the parent that has to be big enough for them.
pub struct PackageGeometry {
    /// `(node, x, y, w, h)` for every element that is not a package.
    pub boxes: Vec<(usize, f32, f32, f32, f32)>,
    /// `(node, x, y, w, h)` for every package frame, drawn behind its members.
    pub frames: Vec<(usize, f32, f32, f32, f32)>,
    pub width: f32,
    pub height: f32,
}

const PAD: f32 = 14.0;
const GAP: f32 = 18.0;
/// Height of the title band across the top of a frame. The renderer draws the separator line at
/// this offset and the studio sends it to the canvas, so the band is not guessed at twice.
pub const TITLE: f32 = 22.0;
const MARGIN: f32 = 48.0;
const MIN_W: f32 = 96.0;
const BOX_H: f32 = 34.0;
/// Left inset of a frame's title.
pub const TITLE_INSET: f32 = 8.0;

pub fn packages_geometry(g: &Graph, frames: &[Frame]) -> PackageGeometry {
    // Pass one: how big is each frame. `frames` is pre-order, so iterating backwards visits a
    // frame after everything nested in it.
    let mut size: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
    for index in (0..frames.len()).rev() {
        let frame = &frames[index];
        let (cw, ch) = content_size(frame, g, &size);
        let (w, h) = if frame.node == usize::MAX {
            (cw, ch)
        } else {
            (cw + PAD * 2.0, ch + PAD * 2.0 + TITLE)
        };
        size.insert(index, (w, h));
    }

    // Pass two: place everything, given the sizes from pass one.
    let mut boxes = Vec::new();
    let mut frame_boxes = Vec::new();
    place(
        0,
        frames,
        g,
        MARGIN,
        MARGIN,
        &size,
        &mut boxes,
        &mut frame_boxes,
    );

    let width = boxes
        .iter()
        .chain(frame_boxes.iter())
        .map(|(_, x, _, w, _)| x + w)
        .fold(0.0f32, f32::max)
        + MARGIN;
    let height = boxes
        .iter()
        .chain(frame_boxes.iter())
        .map(|(_, _, y, _, h)| y + h)
        .fold(0.0f32, f32::max)
        + MARGIN;

    PackageGeometry {
        boxes,
        frames: frame_boxes,
        width,
        height,
    }
}

/// How much room a frame's contents need, not counting its own padding and title.
fn content_size(frame: &Frame, g: &Graph, size: &BTreeMap<usize, (f32, f32)>) -> (f32, f32) {
    let cols = ((frame.members.len() as f32).sqrt().ceil() as usize).max(1);
    let mut row_w = 0.0f32;
    let mut row_h = 0.0f32;
    let mut w = 0.0f32;
    let mut h = 0.0f32;
    for (i, &member) in frame.members.iter().enumerate() {
        let (mw, mh) = member_size(&g.nodes[member]);
        if i > 0 && i % cols == 0 {
            w = w.max(row_w - GAP);
            h += row_h + GAP;
            row_w = 0.0;
            row_h = 0.0;
        }
        row_w += mw + GAP;
        row_h = row_h.max(mh);
    }
    if !frame.members.is_empty() {
        w = w.max(row_w - GAP);
        h += row_h;
    }
    for &child in &frame.children {
        let (cw, ch) = size.get(&child).copied().unwrap_or((0.0, 0.0));
        if h > 0.0 {
            h += GAP;
        }
        w = w.max(cw);
        h += ch;
    }
    (w, h)
}

fn place(
    index: usize,
    frames: &[Frame],
    g: &Graph,
    x: f32,
    y: f32,
    size: &BTreeMap<usize, (f32, f32)>,
    boxes: &mut Vec<(usize, f32, f32, f32, f32)>,
    frame_boxes: &mut Vec<(usize, f32, f32, f32, f32)>,
) {
    let frame = &frames[index];
    let (w, h) = size.get(&index).copied().unwrap_or((0.0, 0.0));
    if frame.node != usize::MAX {
        frame_boxes.push((frame.node, x, y, w, h));
    }
    let inner_x = if frame.node == usize::MAX { x } else { x + PAD };
    let mut inner_y = if frame.node == usize::MAX {
        y
    } else {
        y + TITLE + PAD
    };

    let cols = ((frame.members.len() as f32).sqrt().ceil() as usize).max(1);
    let mut cx = inner_x;
    let mut row_h = 0.0f32;
    for (i, &member) in frame.members.iter().enumerate() {
        if i > 0 && i % cols == 0 {
            cx = inner_x;
            inner_y += row_h + GAP;
            row_h = 0.0;
        }
        let (mw, mh) = member_size(&g.nodes[member]);
        boxes.push((member, cx, inner_y, mw, mh));
        row_h = row_h.max(mh);
        cx += mw + GAP;
    }
    if !frame.members.is_empty() {
        inner_y += row_h + GAP;
    }
    for &child in &frame.children {
        let (_, ch) = size.get(&child).copied().unwrap_or((0.0, 0.0));
        place(child, frames, g, inner_x, inner_y, size, boxes, frame_boxes);
        inner_y += ch + GAP;
    }
}

fn member_size(node: &Node) -> (f32, f32) {
    let mut w =
        text_w(&format!("«{}»", node.stereotype), 9.0).max(text_w(node.label(), 12.0)) + 26.0;
    for attr in &node.attributes {
        w = w.max(text_w(attr, 10.0) + 24.0);
    }
    if let Some(constraint) = &node.constraint {
        w = w.max(text_w(constraint, 9.5) + 24.0);
    }
    (w.max(MIN_W), BOX_H)
}

fn text_w(s: &str, size: f32) -> f32 {
    s.chars()
        .map(|c| {
            if c.is_ascii() {
                size * 0.55
            } else {
                size * 0.98
            }
        })
        .sum()
}

/// Rebuild the frame tree from a projected graph.
///
/// The canvas receives a `Graph`, not a `Packages`, and the graph carries the tree in each node's
/// `parent` plus the `is_frame` flag. Rebuilding here keeps one source of truth: whatever a
/// caller projected is what gets laid out, and the geometry cannot describe a different tree from
/// the one the document holds.
pub fn frames_of(g: &Graph) -> Vec<Frame> {
    let mut frames: Vec<Frame> = vec![Frame {
        node: usize::MAX,
        depth: 0,
        parent: None,
        members: Vec::new(),
        children: Vec::new(),
    }];
    let mut frame_of_node: BTreeMap<usize, usize> = BTreeMap::new();

    // The frame a node belongs to: walk up its parents to the nearest package. A feature of a
    // `part def` has that definition as its parent, and both live in the same frame, so stopping
    // at the first non-package parent would put features at the top level.
    fn home(node: usize, g: &Graph, frame_of_node: &BTreeMap<usize, usize>) -> usize {
        let mut cursor = g.nodes[node].parent;
        while let Some(p) = cursor {
            if let Some(&frame) = frame_of_node.get(&p) {
                return frame;
            }
            cursor = g.nodes[p].parent;
        }
        0
    }

    // Nodes arrive in pre-order, so a package is always seen after the package that contains it.
    for i in 0..g.nodes.len() {
        let found = home(i, g, &frame_of_node);
        if !g.nodes[i].is_frame {
            frames[found].members.push(i);
            continue;
        }
        let parent_node = frames[found].node;
        let index = frames.len();
        frames.push(Frame {
            node: i,
            depth: frames[found].depth + 1,
            parent: if parent_node == usize::MAX {
                None
            } else {
                Some(parent_node)
            },
            members: Vec::new(),
            children: Vec::new(),
        });
        frames[found].children.push(index);
        frame_of_node.insert(i, index);
    }
    frames
}
