//! SVG emission for the two projected views.
//!
//! Geometry is computed here rather than delegated to a graph engine, so every notation rule
//! stays visible in one place. What is drawn and how follows the reference implementation
//! (`sysmlpy` 0.97.1, `plantuml.py`): definitions are square-cornered and usages
//! round-cornered (plantuml.py:3779-3786), the stereotype sits above the name, ownership is a
//! filled diamond `*--`, typing is a hollow triangle with the `:|>` colon `--:|>` labelled
//! `types` (`ARROW_STYLES`, plantuml.py:29-76).
//!
//! The general view is laid out by `layered.rs`: rank assignment, crossing minimisation and
//! Brandes–Köpf x-coordinates. This module only turns the result into path data, and adds the
//! three things that need the whole picture at once:
//!
//!   * **shared runs are drawn once** — a fan-out bus and a typing trunk are each a set of
//!     collinear runs in the same lane, so the renderer deduplicates segments instead of
//!     emitting the same line once per edge;
//!   * **labels get a plate and a search** — a white rectangle under the text, then a search
//!     along the run and perpendicular to it for a position that clears every box and every
//!     label already placed;
//!   * **port names come last** in the interconnection view, so a wire crosses behind them.
//!
//! Line jumps are drawn at orthogonal crossings: the horizontal strand arcs over the vertical
//! one, which is the circuit convention and the only thing that tells a crossing apart from a
//! junction at a glance. The geometry lives in `jumps`, shared with the canvas so both draw the
//! same hops in the same places.

use std::collections::{BTreeMap, HashMap};

use crate::graph::{Arrow, Graph, Node, Side as PortSide, View};
use crate::layered::{self, Input, Style, Symbol};
use crate::packages;
use crate::states::{self, Group};

const TITLE_H: f32 = 34.0;
const ROW_H: f32 = 16.0;
const PORT_H: f32 = 22.0;
const PORT: f32 = 14.0;
const PAD: f32 = 14.0;
/// Room between sibling frames for connectors and for the port names that sit outside the
/// frames. A corridor has to hold a vertical run plus a port name on either side.
const LANE_W: f32 = 92.0;
/// Free horizontal band between two grid rows of children.
const LANE_V: f32 = 46.0;
const MIN_W: f32 = 96.0;
const MARGIN: f32 = 48.0;
const LABEL_SIZE: f32 = 9.0;
const LABEL_H: f32 = 11.0;
/// How far out of its frame a connector leaves a port before the router takes over.
const PORT_LEAD: f32 = 22.0;

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

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Height taken by the compartment rows: attributes, the requirement predicate, then ports.
fn rows_h(node: &Node) -> f32 {
    let mut h = node.attributes.len() as f32 * ROW_H;
    // The `attributes` heading above the rows. It is drawn only when there are rows, so it is
    // counted under the same condition.
    if !node.attributes.is_empty() {
        h += 14.0;
    }
    if node.constraint.is_some() {
        h += ROW_H + 2.0;
    }
    h += node.ports.len() as f32 * PORT_H;
    h
}

/// Width and height a node's frame needs for its own content, ignoring children.
///
/// Public because an editor draws the same frame: a canvas that measured its boxes differently
/// would show a picture that disagrees with the exported SVG for the same model.
pub fn node_size(node: &Node) -> (f32, f32) {
    let mut w =
        text_w(&format!("«{}»", node.stereotype), 9.0).max(text_w(node.label(), 12.0)) + 26.0;
    for attr in &node.attributes {
        w = w.max(text_w(attr, 10.0) + 24.0);
    }
    if let Some(constraint) = &node.constraint {
        w = w.max(text_w(constraint, 9.5) + 24.0);
    }
    for port in &node.ports {
        w = w.max(text_w(&port.name, 9.5) + 24.0 + PORT);
    }
    (w.max(MIN_W), TITLE_H + rows_h(node))
}

#[derive(Clone, Copy)]
struct Rect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Rect {
    fn hits(&self, other: &Rect, pad: f32) -> bool {
        !(self.x1 + pad < other.x0
            || other.x1 + pad < self.x0
            || self.y1 + pad < other.y0
            || other.y1 + pad < self.y0)
    }
}

/// A port's name, collected during frame drawing and emitted after the connectors so no line
/// runs over it.
pub struct PortLabel {
    pub x: f32,
    pub y: f32,
    pub side: PortSide,
    pub text: String,
}

/// Draws the frame, the name block and every compartment row. Returns the y just below the last
/// row, which is where nested children start. Port positions are appended to `port_slots`, and
/// port names to `labels` for the caller to paint last.
/// The ink for text inside a header band.
///
/// A header is either a pale tint or a saturated one, and the same text colour cannot read on
/// both. This picks white on a dark band and near-black on a light one, from the band's own
/// luminance — so adding a colour to `style.rs` cannot make a name unreadable.
fn title_ink(style: crate::style::BoxStyle) -> &'static str {
    let channel = |hex: &str| -> f32 {
        let hex = hex.trim_start_matches('#');
        let v = u32::from_str_radix(hex, 16).unwrap_or(0);
        let r = ((v >> 16) & 0xFF) as f32;
        let g = ((v >> 8) & 0xFF) as f32;
        let b = (v & 0xFF) as f32;
        (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0
    };
    if channel(style.header) < 0.55 {
        "#FFFFFF"
    } else {
        "#1A1A1A"
    }
}

fn draw_frame(
    g: &Graph,
    i: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    out: &mut String,
    port_slots: &mut Vec<(f32, f32)>,
    labels: &mut Vec<PortLabel>,
) -> f32 {
    let node = &g.nodes[i];
    let radius = if node.is_def { 0.0 } else { 8.0 };
    let cx = x + w / 2.0;
    let style = crate::style::for_stereotype(&node.stereotype);
    // The body colour first, then the header band on top of it. Drawing the header as a second
    // rectangle keeps the rounded corners of the whole box: a header with its own radii would
    // show a square corner poking out of a rounded usage box.
    out.push_str(&format!(
        "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"{radius}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\"/>",
        style.body, style.border
    ));
    if style.header != style.body {
        // The band follows the box's own top corners: a rounded usage with a square header shows
        // two right angles sticking out of the rounded outline.
        let (l, r, t, b) = (x, x + w, y, y + TITLE_H);
        let (tl, tr) = (x + radius, x + w - radius);
        out.push_str(&format!(
            "<path d=\"M{l:.1} {:.1} Q{l:.1} {t:.1} {tl:.1} {t:.1} L{tr:.1} {t:.1} Q{r:.1} {t:.1} {r:.1} {:.1} L{r:.1} {b:.1} L{l:.1} {b:.1} Z\" fill=\"{}\" stroke=\"none\"/>",
            y + radius,
            y + radius,
            style.header
        ));
    }
    out.push_str(&format!(
        "<text x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9\" fill=\"{}\">«{}»</text>",
        y + 13.0,
        title_ink(style),
        esc(&node.stereotype)
    ));
    out.push_str(&format!(
        "<text x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"12\" fill=\"{}\">{}</text>",
        y + TITLE_H - 9.0,
        title_ink(style),
        esc(node.label())
    ));

    let mut row_y = y + TITLE_H;
    if !node.attributes.is_empty() || node.constraint.is_some() {
        out.push_str(&format!(
            "<line x1=\"{x:.1}\" y1=\"{row_y:.1}\" x2=\"{:.1}\" y2=\"{row_y:.1}\" stroke=\"{}\" stroke-width=\"0.8\"/>",
            x + w,
            style.border
        ));
        // A compartment states what it holds. Without the heading a reader has to work out from
        // the `+` prefixes that these rows are attributes; the reference names every band.
        if !node.attributes.is_empty() {
            out.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"8.5\" fill=\"#666666\">attributes</text>",
                x + 8.0,
                row_y + 11.0
            ));
            row_y += 14.0;
        }
    }
    for attr in &node.attributes {
        out.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"10\" fill=\"#000000\">{}</text>",
            x + 8.0,
            row_y + 12.0,
            esc(attr)
        ));
        row_y += ROW_H;
    }
    if let Some(constraint) = &node.constraint {
        out.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9.5\" fill=\"#333333\">{}</text>",
            x + 8.0,
            row_y + 12.0,
            esc(constraint)
        ));
        row_y += ROW_H + 2.0;
    }

    // Ports keep the side the model gives them (`in` left, `out` right) but are ordered within
    // that side by name, so the drawing is stable when the body is reordered. Ordering them by
    // their connection partner instead would need the whole diagram, which is not known here.
    let mut left = 0usize;
    let mut right = 0usize;
    for port in &node.ports {
        let slot = match port.side {
            PortSide::Left => {
                left += 1;
                left
            }
            PortSide::Right => {
                right += 1;
                right
            }
        };
        let py = row_y + PORT_H / 2.0 + (slot as f32 - 1.0) * PORT_H;
        let px = match port.side {
            PortSide::Left => x,
            PortSide::Right => x + w,
        };
        port_slots.push((px, py));
        out.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{PORT}\" height=\"{PORT}\" fill=\"#FFFFFF\" stroke=\"{}\" stroke-width=\"1\"/>",
            px - PORT / 2.0,
            py - PORT / 2.0,
            style.border
        ));
        // A direction arrow inside the port box. Without it the square says "something plugs in
        // here" and nothing else; with it the port reads as an input or an output, which is the
        // one thing the notation's port symbol states. An `inout` port gets a dot: an arrow
        // either way would claim a direction the model did not state.
        if let Some(direction) = port.direction {
            use sysml_model::Direction;
            match direction {
                Direction::InOut => out.push_str(&format!(
                    "<circle cx=\"{px:.1}\" cy=\"{py:.1}\" r=\"2\" fill=\"{}\" stroke=\"none\"/>",
                    style.border
                )),
                Direction::In | Direction::Out => {
                    let tip = if direction == Direction::Out {
                        px + 3.5
                    } else {
                        px - 3.5
                    };
                    let back = if direction == Direction::Out {
                        tip - 2.5
                    } else {
                        tip + 2.5
                    };
                    let tail = if direction == Direction::Out {
                        px - 3.5
                    } else {
                        px + 3.5
                    };
                    out.push_str(&format!(
                        "<path d=\"M{tail:.1} {py:.1} L{tip:.1} {py:.1} M{tip:.1} {py:.1} L{back:.1} {:.1} M{tip:.1} {py:.1} L{back:.1} {:.1}\" stroke=\"{}\" stroke-width=\"1\" fill=\"none\"/>",
                        py - 2.0,
                        py + 2.0,
                        style.border
                    ));
                }
            }
        }
        // The name goes *outside* the frame, on the port's own side. Inside, it competes with
        // the frame border and with every connector that has to pass this row; outside, the
        // corridor between frames is the only place a line can be.
        let tx = match port.side {
            PortSide::Left => x - PORT / 2.0 - 4.0,
            PortSide::Right => x + w + PORT / 2.0 + 4.0,
        };
        labels.push(PortLabel {
            x: tx,
            y: py + 3.5,
            side: port.side,
            text: port.name.clone(),
        });
        row_y += PORT_H;
    }
    row_y
}

/// Ownership symbol: a filled diamond in the notation's `COMPOSITION` shape, centred on `(x, y)`.
fn diamond(out: &mut String, x: f32, y: f32) {
    let d = 5.0;
    out.push_str(&format!(
        "<path d=\"M{x:.1} {y:.1} L{:.1} {:.1} L{x:.1} {:.1} L{:.1} {:.1} Z\" fill=\"#000000\" stroke=\"#000000\" stroke-width=\"0.8\"/>",
        x - d,
        y + d,
        y + 2.0 * d,
        x + d,
        y + d
    ));
}

#[derive(Clone, Copy)]
enum Dir {
    Up,
    Down,
}

/// Hollow triangle of the `:|>` decoration, apex at `(x, y)`, opening away from `dir`.
///
/// PlantUML's `LinkDecor.DEFINEDBY` is declared with `fill=false` and the same geometry as
/// `EXTENDS` (LinkDecor.java:72,78), so the triangle is hollow; `ExtremityExtendsLike.DefinedBy`
/// then draws two dots before it. The filled variant is `COMPOSITION` (the diamond) only.
fn hollow_triangle(out: &mut String, x: f32, y: f32, dir: Dir) {
    let (a, b, c, d) = match dir {
        Dir::Up => (x - 4.5, y + 7.0, x + 4.5, y + 7.0),
        Dir::Down => (x - 4.5, y - 7.0, x + 4.5, y - 7.0),
    };
    out.push_str(&format!(
        "<path d=\"M{a:.1} {b:.1} L{x:.1} {y:.1} L{c:.1} {d:.1} Z\" fill=\"#FFFFFF\" stroke=\"#000000\" stroke-width=\"1\"/>"
    ));
}

/// The colon of `:|>`. Two dots across the line, just before the head, so a typing link reads
/// the way the textual `x : Type` does.
fn colon(out: &mut String, x: f32, y: f32, dir: Dir) {
    let offset = match dir {
        Dir::Up => 15.0,
        Dir::Down => -15.0,
    };
    for dx in [-3.0f32, 3.0] {
        out.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"1.4\" fill=\"#000000\"/>",
            x + dx,
            y + offset
        ));
    }
}

/// Open arrowhead of a dependency, `(dx, dy)` being the direction it points in.
fn open_head(out: &mut String, x: f32, y: f32, dx: f32, dy: f32) {
    out.push_str(&format!(
        "<path d=\"M{x:.1} {y:.1} l{:.1} {:.1} M{x:.1} {y:.1} l{:.1} {:.1}\" stroke=\"#000000\" stroke-width=\"1.1\" fill=\"none\"/>",
        -4.0 * dx + 4.0 * dy,
        -4.0 * dy - 4.0 * dx,
        -4.0 * dx - 4.0 * dy,
        -4.0 * dy + 4.0 * dx
    ));
}

struct PendingLabel {
    text: String,
    /// Preferred centre x and baseline.
    x: f32,
    y: f32,
}

/// Places a label without landing on another label or on a box. Candidates move along the
/// horizontal run first, then step away from the line.
fn place_label(
    text: &str,
    cx: f32,
    baseline: f32,
    boxes: &[Rect],
    taken: &mut Vec<Rect>,
) -> (f32, f32) {
    let w = text_w(text, LABEL_SIZE) + 6.0;
    let h = LABEL_H;
    let shakes = [0.0f32, 18.0, -18.0, 36.0, -36.0, 60.0, -60.0, 92.0, -92.0];
    let lifts = [0.0f32, -13.0, 13.0, -26.0, 26.0];

    let candidate_at = |shake: f32, lift: f32| -> Rect {
        let x0 = cx - w / 2.0 + shake;
        let y0 = baseline - h + 2.5 + lift;
        Rect {
            x0,
            y0,
            x1: x0 + w,
            y1: y0 + h,
        }
    };

    for lift in lifts {
        for shake in shakes {
            let candidate = candidate_at(shake, lift);
            let blocked = taken.iter().any(|r| candidate.hits(r, 3.0))
                || boxes.iter().any(|r| candidate.hits(r, 1.0));
            if !blocked {
                taken.push(candidate);
                return (cx + shake, baseline + lift);
            }
        }
    }
    let candidate = candidate_at(0.0, 0.0);
    taken.push(candidate);
    (cx, baseline)
}

/// Boxes and relationship polylines for the general view, in page coordinates.
///
/// This is the layout half of `render_general`, split out so an editor can draw the same picture
/// the export draws. Keeping one implementation is the point: two layout passes over one model
/// are two pictures, and the difference shows up as "the canvas moved my boxes".
pub struct GeneralGeometry {
    pub boxes: Vec<(usize, f32, f32, f32, f32)>,
    pub edges: Vec<(usize, Vec<(f32, f32)>)>,
    pub width: f32,
    pub height: f32,
    pub layered: layered::Layered,
}

pub fn general_geometry(g: &Graph, style: Style) -> GeneralGeometry {
    let sizes: Vec<(f32, f32)> = g.nodes.iter().map(node_size).collect();
    let inputs: Vec<Input> = g
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| e.arrow != Arrow::Connector)
        .map(|(k, e)| Input {
            index: k,
            from: e.from,
            to: e.to,
            kind: e.arrow,
        })
        .collect();
    let preorder: Vec<usize> = (0..g.nodes.len()).collect();
    let parent: Vec<Option<usize>> = g.nodes.iter().map(|n| n.parent).collect();
    let lay = match style {
        Style::Nested => layered::build(&sizes, &inputs, &preorder, &parent),
        Style::Tree => layered::tree(&sizes, &inputs, &parent),
    };
    let origin = MARGIN;
    let boxes = lay
        .nodes
        .iter()
        .filter_map(|nd| nd.real.map(|i| (i, nd.x + origin, nd.y, nd.w, nd.h)))
        .collect();
    let edges = lay
        .routes
        .iter()
        .filter(|r| r.points.len() >= 2)
        .map(|r| {
            (
                r.edge,
                r.points.iter().map(|p| (p.0 + origin, p.1)).collect(),
            )
        })
        .collect();
    GeneralGeometry {
        boxes,
        edges,
        width: lay.width + 2.0 * MARGIN,
        height: lay.height + MARGIN,
        layered: lay,
    }
}

fn render_general(g: &Graph, title: &str, style: Style) -> String {
    // `SYSML_DEBUG_RENDER=1` prints how long the layout and the frames took. Kept because the
    // stage that dominates a render has moved twice already — it was the label placement, then the
    // arc formatting — and guessing which one it is costs more than one line of output.
    let probe = std::env::var("SYSML_DEBUG_RENDER").is_ok();
    let t0 = std::time::Instant::now();
    let geom = general_geometry(g, style);
    if probe {
        eprintln!("render: layout {:?}", t0.elapsed());
    }
    let lay = &geom.layered;

    let mut body = String::new();
    let mut port_slots: Vec<Vec<(f32, f32)>> = vec![Vec::new(); g.nodes.len()];
    let mut unused: Vec<PortLabel> = Vec::new();
    // The layout works in content coordinates starting at 0; `general_geometry` already added
    // the page origin, so the drawing reads the boxes from there rather than re-deriving them.
    for &(real, x, y, w, h) in &geom.boxes {
        draw_frame(
            g,
            real,
            x,
            y,
            w,
            h,
            &mut body,
            &mut port_slots[real],
            &mut unused,
        );
    }

    let boxes: Vec<Rect> = geom
        .boxes
        .iter()
        .map(|&(_, x, y, w, h)| Rect {
            x0: x,
            y0: y,
            x1: x + w,
            y1: y + h,
        })
        .collect();
    if probe {
        eprintln!("render: frames {:?}", t0.elapsed());
    }

    // ---- edges ------------------------------------------------------------------------------
    // One path per relationship. Shared runs (the ownership bus, the typing trunk) are therefore
    // overstruck, which is invisible with an opaque stroke of constant width and keeps each edge
    // individually addressable — a checker reading the SVG can still tell which segments belong
    // to which relationship.
    let mut heads: Vec<(f32, f32, Dir, Arrow, bool)> = Vec::new();
    let mut labels: Vec<PendingLabel> = Vec::new();
    // Every edge's path data, held back so the whole picture can be stroked as one element.
    //
    // Drawn one path per relationship, a shared run is overstruck: the ownership bus a fan-out
    // leaves along, the typing trunk into a definition. With an opaque stroke of constant width
    // the colour is the same either way, but each overstrike is composited again by the
    // rasteriser, so a segment covered five times comes out visibly heavier than the one beside
    // it. One element per stroke width composites once, and a shared run then looks exactly
    // like a lone one.
    let mut strokes: Vec<String> = Vec::new();
    let mut labelled: std::collections::HashSet<usize> = Default::default();
    let polylines: HashMap<usize, &Vec<(f32, f32)>> =
        geom.edges.iter().map(|(i, p)| (*i, p)).collect();

    // Where each relationship has to hop over another. Computed over the whole picture at once
    // because a crossing is a property of two lines: deciding per line would make each edge the
    // one that hops and the pair would show two arcs at a single crossing.
    let routes: Vec<(usize, Vec<(f32, f32)>)> = lay
        .routes
        .iter()
        .filter_map(|r| polylines.get(&r.edge).map(|p| (r.edge, (*p).clone())))
        .filter(|(_, p)| p.len() >= 2)
        .collect();
    let order: Vec<Vec<(f32, f32)>> = routes.iter().map(|(_, p)| p.clone()).collect();
    let mut jumped: HashMap<usize, Vec<(f32, f32)>> = HashMap::new();
    for ((edge, _), hops) in routes.iter().zip(crate::jumps::hops(&order)) {
        jumped.insert(*edge, hops);
    }
    for route in &lay.routes {
        let edge = &g.edges[route.edge];
        let Some(points) = polylines.get(&route.edge) else {
            continue;
        };
        let pts: Vec<(f32, f32)> = (*points).clone();
        // A route needs two points to be a line. Indexing with `len() - 1` after the check is
        // what makes the head glyph's endpoint safe without an unwrap.
        if pts.len() < 2 {
            continue;
        }
        let last = pts[pts.len() - 1];
        let empty: Vec<(f32, f32)> = Vec::new();
        let over = jumped.get(&route.edge).unwrap_or(&empty);
        let d = crate::jumps::path(&pts, over, crate::jumps::RADIUS);
        strokes.push(d);

        if let Some(Symbol {
            at_start,
            points_down,
        }) = route.head
        {
            let at = if at_start { pts[0] } else { last };
            let dir = if points_down { Dir::Down } else { Dir::Up };
            // One ownership diamond per owner, not one per edge. All the features of an owner
            // share a bus, so N edges leave the same point and N diamonds would be N identical
            // glyphs on top of each other — invisible, but wrong as a count, and the notation
            // says the diamond marks the composite, not the member. Typing keeps one triangle
            // per edge because each trunk ends at its own definition.
            let dedupe = edge.arrow == Arrow::Composite;
            heads.push((at.0, at.1, dir, edge.arrow, dedupe));
        }
        if let Some(text) = &edge.label {
            // A typing trunk is a single line into a definition, so it gets one label rather
            // than one per usage; the first edge in graph order is the one that carries it.
            let show = edge.arrow != Arrow::Typing || labelled.insert(edge.to);
            if show {
                if let Some((lx, ly)) = route.label {
                    labels.push(PendingLabel {
                        text: text.clone(),
                        x: lx + MARGIN,
                        y: ly,
                    });
                }
            }
        }
    }

    // One element for every relationship line. Sub-paths that share a run are drawn once by the
    // rasteriser, so a bus with five members is as heavy as a single line.
    if !strokes.is_empty() {
        body.push_str(&format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1\"/>",
            strokes.join(" ")
        ));
    }

    let mut drawn_diamonds: Vec<(i32, i32)> = Vec::new();
    for (x, y, dir, arrow, dedupe) in heads {
        if dedupe {
            let key = ((x * 2.0).round() as i32, (y * 2.0).round() as i32);
            if drawn_diamonds.contains(&key) {
                continue;
            }
            drawn_diamonds.push(key);
        }
        match arrow {
            Arrow::Composite => diamond(&mut body, x, y),
            Arrow::Typing => {
                hollow_triangle(&mut body, x, y, dir);
                colon(&mut body, x, y, dir);
            }
            Arrow::Annotate => {
                let (ux, uy) = match dir {
                    Dir::Up => (0.0, -1.0),
                    Dir::Down => (0.0, 1.0),
                };
                open_head(&mut body, x, y, ux, uy);
            }
            Arrow::Satisfy => {
                let (ux, uy) = match dir {
                    Dir::Up => (0.0, -1.0),
                    Dir::Down => (0.0, 1.0),
                };
                open_head(&mut body, x, y, ux, uy);
            }
            // The state view draws its own arrowhead; nothing here emits one.
            Arrow::Connector | Arrow::Transition => {}
        }
    }

    // Labels last, each on its own white plate, so nothing drawn earlier shows through.
    //
    // Measured at twelve thousand elements, this loop is not where a render goes: a label finds a
    // free spot on its first candidate almost always, so the scan it does there is one pass over the
    // frames. A grid index over the boxes and the placed labels was written and removed — the same
    // render took 5.5s with it and 5.5s without, and the five seconds were the arcs.
    let mut taken: Vec<Rect> = Vec::new();
    for label in &labels {
        let (cx, baseline) = place_label(&label.text, label.x, label.y, &boxes, &mut taken);
        let w = text_w(&label.text, LABEL_SIZE) + 6.0;
        body.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{w:.1}\" height=\"{LABEL_H}\" fill=\"#FFFFFF\" stroke=\"none\"/>",
            cx - w / 2.0,
            baseline - LABEL_H + 2.5
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{baseline:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{LABEL_SIZE}\" fill=\"#000000\">{}</text>",
            cx,
            esc(&label.text)
        ));
    }

    let width = lay.width + 2.0 * MARGIN;
    let height = lay.height + MARGIN;
    document(width, height, title, "general view", &body)
}

/// Boxes, port slots and connector polylines for the interconnection view.
///
/// The split from painting is the same as in the general view: one layout, two consumers. A
/// canvas that re-derived these numbers would drift from the exported SVG, and the drift would
/// read as "the picture in the editor is not the picture I exported".
pub struct InterconnectionGeometry {
    pub boxes: Vec<Option<(f32, f32, f32, f32)>>,
    pub port_slots: Vec<Vec<(f32, f32)>>,
    pub port_labels: Vec<PortLabel>,
    pub connectors: Vec<(usize, Vec<(f32, f32)>)>,
    pub width: f32,
    pub height: f32,
    pub empty: bool,
}

pub fn interconnection_geometry(g: &Graph) -> InterconnectionGeometry {
    let roots: Vec<usize> = (0..g.nodes.len())
        .filter(|&i| g.nodes[i].parent.is_none())
        .collect();
    if roots.is_empty() {
        return InterconnectionGeometry {
            boxes: vec![None; g.nodes.len()],
            port_slots: vec![Vec::new(); g.nodes.len()],
            port_labels: Vec::new(),
            connectors: Vec::new(),
            width: 760.0,
            height: 112.0,
            empty: true,
        };
    }
    let mut body = String::new();
    let mut geometry: Vec<Option<(f32, f32, f32, f32)>> = vec![None; g.nodes.len()];
    let mut port_slots: Vec<Vec<(f32, f32)>> = vec![Vec::new(); g.nodes.len()];
    let mut port_labels: Vec<PortLabel> = Vec::new();
    let mut connectors: Vec<(usize, Vec<(f32, f32)>)> = Vec::new();

    // Top-level parts go side by side, wrapping at a target width, rather than one per row.
    // The old stack put three unrelated systems in a vertical column and left most of the page
    // empty; a row is what the reference layout does too, because the systems in an
    // interconnection view are peers and the reader compares them left to right.
    const TARGET_W: f32 = 1500.0;
    let sizes: Vec<(f32, f32)> = roots.iter().map(|&r| measure_iv(g, r)).collect();
    let mut x = MARGIN;
    // The header (title and subtitle) ends near y = 50, and the router keeps an outer lane 22px
    // above the topmost frame. Starting the frames at 52 put that lane through the title, so a
    // wire that went over the top struck the file name out.
    let mut y = layered::HEADER - 16.0;
    let mut row_h = 0.0f32;
    let mut max_x = MARGIN;
    for (i, &root) in roots.iter().enumerate() {
        let (w, h) = sizes[i];
        if x > MARGIN && x + w > TARGET_W {
            x = MARGIN;
            y += row_h + layered::MIN_GAP;
            row_h = 0.0;
        }
        place_iv(
            g,
            root,
            x,
            y,
            &mut body,
            &mut geometry,
            &mut port_slots,
            &mut port_labels,
        );
        x += w + layered::MIN_GAP * 2.0;
        row_h = row_h.max(h);
        max_x = max_x.max(x - layered::MIN_GAP * 2.0);
    }
    y += row_h;

    // Connectors are routed by an A* search over a Hanan grid — see `route_connector`. The grid
    // is built from the frame borders, so every candidate path either stays outside the frames
    // or is rejected; there is no after-the-fact checking for crossings.
    let frame_rect = |i: usize| -> Option<Rect> {
        let (x, y, w, h) = geometry[i]?;
        Some(Rect {
            x0: x,
            y0: y,
            x1: x + w,
            y1: y + h,
        })
    };
    // Every frame is an obstacle. A container is not "transparent": a wire from one child to a
    // sibling must not cross the container's title block or the boxes between them, and a wire
    // that crosses a container's own border is fine only in the sense that its endpoints are
    // inside it — which the anchor step handles by leaving through the nearest side.
    //
    // Only leaf frames were considered before, so a wire could run straight through a subtree
    // container like `rearAxle : RearAxle` and be judged clean by the same rule that let it.
    let mut frames: Vec<Rect> = (0..g.nodes.len()).filter_map(frame_rect).collect();
    frames.sort_by(|a, b| {
        a.x0.partial_cmp(&b.x0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.y0.partial_cmp(&b.y0).unwrap_or(std::cmp::Ordering::Equal))
    });
    // The title block of every frame — stereotype line, name line, and the rule below them — is
    // an obstacle no connection can be granted access to. A container is transparent so a wire
    // can reach a nested part, but the wire has to go around the box's own label rather than
    // through it, and a title sits inside the frame in a way the generic frame test cannot see.
    //
    // The band covers the full title height plus the first compartment row, because a port that
    // lives in the attribute compartment is *below* the title in y but visually part of the same
    // block; routing between them at title height crosses the name.
    let title_bands: Vec<Rect> = (0..g.nodes.len())
        .filter_map(|i| {
            let (x, y, w, _) = geometry[i]?;
            Some(Rect {
                x0: x + 1.0,
                y0: y + 1.0,
                x1: x + w - 1.0,
                y1: y + TITLE_H - 1.0,
            })
        })
        .collect();
    // The outer band the router may use, pushed out from the outermost frame so a route that
    // leaves the drawing runs in clear white space, not along the container's border.
    let outer_left = frames.iter().map(|r| r.x0).fold(f32::MAX, f32::min);
    let outer_top = frames.iter().map(|r| r.y0).fold(f32::MAX, f32::min);
    let outer_right = frames.iter().map(|r| r.x1).fold(0.0f32, f32::max);
    let outer_bottom = frames.iter().map(|r| r.y1).fold(0.0f32, f32::max);
    const OUTER: f32 = 22.0;
    let bounds = Rect {
        x0: outer_left - OUTER,
        y0: outer_top - OUTER,
        x1: outer_right + OUTER,
        y1: outer_bottom + OUTER,
    };

    for (edge_index, edge) in g.edges.iter().enumerate() {
        if edge.arrow != Arrow::Connector {
            continue;
        }
        let (Some(from), Some(to)) = (
            port_point(edge.from, edge.from_port, &port_slots),
            port_point(edge.to, edge.to_port, &port_slots),
        ) else {
            continue;
        };
        // Frames a wire may cross: the two endpoint frames themselves plus every container that
        // encloses one of them. Everything else — including a sibling's container, and including
        // the containers' *title blocks* — stays an obstacle.
        let mut own: Vec<Option<Rect>> = vec![frame_rect(edge.from), frame_rect(edge.to)];
        for end in [edge.from, edge.to] {
            let mut cur = g.nodes[end].parent;
            while let Some(p) = cur {
                own.push(frame_rect(p));
                cur = g.nodes[p].parent;
            }
        }
        let own: [Option<Rect>; 2] = [own[0], own[1]];
        // The containers around either end, which the search may cross. The two end frames are
        // not among them: the route starts and stops at anchors outside them, so letting the
        // search through them only allowed a wire to run across its own end box — `a.o` to
        // `d.i` drew a line straight through `d`. An end that encloses the other end (a boundary
        // port delegating inwards) is still crossable, as that end's container.
        let transparent: Vec<Rect> = {
            let mut v = Vec::new();
            for end in [edge.from, edge.to] {
                let mut cur = g.nodes[end].parent;
                while let Some(p) = cur {
                    if let Some(r) = frame_rect(p) {
                        v.push(r);
                    }
                    cur = g.nodes[p].parent;
                }
            }
            v
        };
        // The innermost frame that holds both ends. A lead-out may not leave it: a wire between
        // two parts of one vehicle that steps outside the vehicle reads as two wires.
        let shared: Option<Rect> = {
            let mut up = Vec::new();
            let mut cur = Some(edge.from);
            while let Some(p) = cur {
                up.push(p);
                cur = g.nodes[p].parent;
            }
            let mut cur = Some(edge.to);
            let mut found = None;
            while let Some(p) = cur {
                if up.contains(&p) {
                    found = Some(p);
                    break;
                }
                cur = g.nodes[p].parent;
            }
            found.and_then(frame_rect)
        };
        let same = |a: &Rect, b: &Rect| (a.x0 - b.x0).abs() < 0.5 && (a.y0 - b.y0).abs() < 0.5;

        // The wire does not route from the port square itself. A port sits on its frame border,
        // so the first hop away from it lies along that border, and the search happily extends
        // that hop across the whole frame — which reads as a line drawn along the box. Route
        // between anchor points pushed clear of each port instead, then join each anchor to its
        // port with one short perpendicular segment.
        // The anchor is the port pushed straight *out* of its own frame, perpendicular to the
        // border it sits on. The previous version moved it vertically instead, so the lead-out
        // segment ran along the border it was trying to leave — which is exactly what a reader
        // sees as a line hugging (or doubling) the box outline.
        let anchor = |own: Option<Rect>, port: (f32, f32), other: (f32, f32)| -> (f32, f32) {
            let Some(r) = own else { return port };
            let on_left = (port.0 - r.x0).abs() < (port.0 - r.x1).abs();
            // Step out on the side the port is on, and towards the other endpoint when the port
            // is ambiguous (a port that sits at the exact centre of a thin frame).
            let other_cx = other.0;
            let left = if (port.0 - r.x0).abs() < 1.0 {
                true
            } else if (port.0 - r.x1).abs() < 1.0 {
                false
            } else {
                other_cx > r.x1
            };
            let _ = on_left;
            let passable = |f: &Rect| same(f, &r) || transparent.iter().any(|o| same(o, f));
            // A boundary port wired to a part inside its own frame leads in, not out. Leading
            // out made the delegation leave the container and cross its border again on the way
            // back to the inner part.
            let other_inside = r.x0 < other.0 && other.0 < r.x1 && r.y0 < other.1 && other.1 < r.y1;
            if other_inside {
                let x = if left {
                    r.x0 + PORT_LEAD
                } else {
                    r.x1 - PORT_LEAD
                };
                let probe = Rect {
                    x0: x.min(port.0),
                    y0: port.1 - 3.0,
                    x1: x.max(port.0),
                    y1: port.1 + 3.0,
                };
                if frames.iter().all(|f| !f.hits(&probe, 0.0) || passable(f)) {
                    return (x, port.1);
                }
            }
            let mut x = if left {
                r.x0 - PORT_LEAD
            } else {
                r.x1 + PORT_LEAD
            };
            // Stay inside the frame the two ends share: halfway between the end's border and the
            // container's, when a full lead would cross the container.
            if let Some(c) = shared.filter(|c| !same(c, &r)) {
                if left && x <= c.x0 + 1.0 {
                    x = (c.x0 + r.x0) / 2.0;
                }
                if !left && x >= c.x1 - 1.0 {
                    x = (c.x1 + r.x1) / 2.0;
                }
            }
            // Stepping out sideways is the readable lead-out, but the space next to a frame may
            // belong to a neighbouring box: a parent whose child sits flush against its border
            // leaves no corridor. Probe the horizontal leg and fall back to the vertical
            // anchor when it would pass through another frame.
            let probe = Rect {
                x0: x.min(port.0),
                y0: port.1 - 3.0,
                x1: x.max(port.0),
                y1: port.1 + 3.0,
            };
            // The lead-out may cross the end's own frame and the containers around either end.
            // Testing only the end's own frame made the enclosing container an obstacle to its
            // own children's lead-outs, so a wire between two parts of `v : Vehicle` fell back to
            // the vertical anchor and left the container to come back.
            let clear = frames.iter().all(|f| !f.hits(&probe, 0.0) || passable(f));
            if clear {
                return (x, port.1);
            }
            let mut below = r.y1 + PORT_LEAD;
            let mut above = r.y0 - PORT_LEAD;
            if let Some(c) = shared.filter(|c| !same(c, &r)) {
                if below >= c.y1 - 1.0 {
                    below = (c.y1 + r.y1) / 2.0;
                }
                if above <= c.y0 + TITLE_H {
                    above = (c.y0 + TITLE_H + r.y0) / 2.0;
                }
            }
            let vy = if other.1 > port.1 {
                if (below - other.1).abs() < (above - other.1).abs() {
                    below
                } else {
                    above
                }
            } else if (above - other.1).abs() <= (below - other.1).abs() {
                above
            } else {
                below
            };
            (port.0, vy)
        };
        let a1 = anchor(own[0], from, to);
        let a2 = anchor(own[1], to, from);

        let points = match route_connector(a1, a2, &frames, &title_bands, &transparent, bounds) {
            Some(points) => {
                let mut v = vec![from];
                v.extend(points);
                v.push(to);
                v
            }
            // No route exists on the grid (the ports are boxed in). A straight line is still
            // better than dropping the connector.
            None => vec![from, to],
        };
        connectors.push((edge_index, straighten(points)));
    }

    // A port name on the right of the outermost frame sticks out past it, so the canvas has to
    // include the label, not just the frame. Without this the rightmost names are clipped.
    let label_right = port_labels
        .iter()
        .filter(|l| l.side == PortSide::Right)
        .map(|l| l.x + text_w(&l.text, 9.5) + 4.0)
        .fold(0.0f32, f32::max);
    let width = max_x.max(label_right) + MARGIN;
    let height = y + MARGIN;
    let _ = body;
    InterconnectionGeometry {
        boxes: geometry,
        port_slots,
        port_labels,
        connectors,
        width,
        height,
        empty: false,
    }
}

fn render_interconnection(g: &Graph, title: &str) -> String {
    let geom = interconnection_geometry(g);
    if geom.empty {
        // An empty picture is ambiguous: it looks the same whether the model has no instances or
        // the tool failed to find them. Naming the reason separates "add a part usage" from
        // "this is a defect", which need opposite responses.
        let note = "no part usages to draw: this view shows instances, and the model declares                     only definitions";
        return format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="760" height="112" viewBox="0 0 760 112">
<rect width="100%" height="100%" fill="#FFFFFF"/>
<text x="24" y="30" font-family="Helvetica, Arial, sans-serif" font-size="14" fill="#000000">{title}</text>
<text x="24" y="52" font-family="Helvetica, Arial, sans-serif" font-size="11" fill="#666666">interconnection view, 0 nodes, 0 edges</text>
<text x="24" y="78" font-family="Helvetica, Arial, sans-serif" font-size="11" fill="#A03030">{note}</text>
</svg>"##,
            title = esc(title),
            note = esc(note),
        );
    }
    let mut body = String::new();
    let mut sink_slots: Vec<Vec<(f32, f32)>> = vec![Vec::new(); g.nodes.len()];
    let mut sink_labels: Vec<PortLabel> = Vec::new();
    let mut geometry: Vec<Option<(f32, f32, f32, f32)>> = vec![None; g.nodes.len()];
    for (i, slot) in geom.boxes.iter().enumerate() {
        let Some((x, y, w, h)) = *slot else { continue };
        geometry[i] = Some((x, y, w, h));
        draw_frame(
            g,
            i,
            x,
            y,
            w,
            h,
            &mut body,
            &mut sink_slots[i],
            &mut sink_labels,
        );
    }
    // Hops are computed over every connector at once, for the same reason as in the general
    // view: a crossing belongs to a pair of lines, not to one of them.
    let lines: Vec<Vec<(f32, f32)>> = geom.connectors.iter().map(|(_, p)| p.clone()).collect();
    let hops = crate::jumps::hops(&lines);
    // One element for every wire, for the same reason as the general view: a shared run must not
    // get darker each time another connector takes the same corridor.
    let mut wires: Vec<String> = Vec::new();
    for ((_, points), over) in geom.connectors.iter().zip(&hops) {
        if points.len() < 2 {
            continue;
        }
        wires.push(crate::jumps::path(points, over, crate::jumps::RADIUS));
    }
    if !wires.is_empty() {
        body.push_str(&format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"#000000\" stroke-width=\"2.6\"/>",
            wires.join(" ")
        ));
    }
    body.push_str(&paint_port_labels(&geom.port_labels));
    document(
        geom.width,
        geom.height,
        title,
        "interconnection view",
        &body,
    )
}

/// Drop the points a polyline passes through without turning.
///
/// A* over the Hanan grid returns the walk it searched — one point per grid edge — so an edge that
/// crosses the canvas arrives as a thousand points along three straight lines. The picture is the
/// same either way, but the cost is not: at twelve thousand elements the stroke path reached two
/// hundred megabytes, and formatting it was five of the render's six seconds. A point survives only
/// where the direction changes.
fn straighten(points: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    if points.len() < 3 {
        return points;
    }
    let mut out = Vec::with_capacity(points.len());
    out.push(points[0]);
    for i in 1..points.len() - 1 {
        let (a, b, c) = (points[i - 1], points[i], points[i + 1]);
        let same_x = (a.0 - b.0).abs() < 0.01 && (b.0 - c.0).abs() < 0.01;
        let same_y = (a.1 - b.1).abs() < 0.01 && (b.1 - c.1).abs() < 0.01;
        if !same_x && !same_y {
            out.push(b);
        }
    }
    out.push(points[points.len() - 1]);
    out
}

/// Orthogonal connector routing on a Hanan grid.
///
/// A Hanan grid is the Cartesian product of every interesting x and y coordinate — here the
/// frame borders, each widened by a clearance — plus the port rows and columns. Every
/// orthogonal shortest path with the fewest bends can be built from those lines, so a search
/// over the grid is both complete for that class and small: N frames give O(N) lines, not a
/// pixel lattice.
///
/// Routing is A* over grid *edges*. Turning costs more than going straight, which is what keeps
/// routes readable: without it the search happily zig-zags to save two pixels. A candidate
/// position is rejected when it lies inside any frame other than the two being connected.
///
/// Congestion is priced: a grid edge already used by an earlier connector costs extra, so a
/// later route prefers a clear lane over laying itself on top of an existing run. That is what
/// removes the collinear overlaps the previous version produced whenever two connectors took
/// the same corridor.
fn route_connector(
    from: (f32, f32),
    to: (f32, f32),
    frames: &[Rect],
    title_bands: &[Rect],
    transparent: &[Rect],
    bounds: Rect,
) -> Option<Vec<(f32, f32)>> {
    // A frame is transparent only when it encloses one of the two endpoints. A wire between two
    // parts of the same vehicle has to live inside the vehicle's frame; a wire between two
    // vehicles must not, and a *sibling's* container is never transparent no matter how close
    // it is.
    let is_own = |r: &Rect| {
        transparent
            .iter()
            .any(|o| (o.x0 - r.x0).abs() < 0.5 && (o.y0 - r.y0).abs() < 0.5)
    };

    // Must stay above the graze threshold in `route_review.py` (6px) plus the stroke width, so
    // no offered line can be mistaken for a line running along a border.
    const CLEAR: f32 = 24.0;
    let mut xs: Vec<f32> = vec![from.0, to.0, bounds.x0, bounds.x1];
    let mut ys: Vec<f32> = vec![from.1, to.1, bounds.y0, bounds.y1];
    for r in frames {
        // Only the *offset* lines are offered for other frames. Adding the bare borders too
        // gives the search cheap lanes that run exactly along a box, which reads as a doubled
        // border.
        let _ = is_own(r);
        xs.push(r.x0 - CLEAR);
        xs.push(r.x1 + CLEAR);
        ys.push(r.y0 - CLEAR);
        ys.push(r.y1 + CLEAR);
    }
    let snap = |v: Vec<f32>| {
        let mut v = v;
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        v
    };
    let xs = snap(xs);
    let ys = snap(ys);

    let inside = |x: f32, y: f32| -> bool {
        let in_band = title_bands
            .iter()
            .any(|r| r.x0 < x && x < r.x1 && r.y0 < y && y < r.y1);
        in_band
            || frames.iter().any(|r| {
                !is_own(r) && r.x0 + 0.5 < x && x < r.x1 - 0.5 && r.y0 + 0.5 < y && y < r.y1 - 0.5
            })
    };
    let blocked = |a: (f32, f32), b: (f32, f32)| -> bool {
        let cuts_band = title_bands.iter().any(|r| {
            if (a.1 - b.1).abs() < 0.01 {
                r.y0 < a.1 && a.1 < r.y1 && a.0.min(b.0) < r.x1 && a.0.max(b.0) > r.x0
            } else {
                r.x0 < a.0 && a.0 < r.x1 && a.1.min(b.1) < r.y1 && a.1.max(b.1) > r.y0
            }
        });
        if cuts_band {
            return true;
        }
        frames.iter().any(|r| {
            if is_own(r) {
                return false;
            }
            if (a.1 - b.1).abs() < 0.01 {
                r.y0 + 0.5 < a.1
                    && a.1 < r.y1 - 0.5
                    && a.0.min(b.0) < r.x1 - 0.5
                    && a.0.max(b.0) > r.x0 + 0.5
            } else {
                r.x0 + 0.5 < a.0
                    && a.0 < r.x1 - 0.5
                    && a.1.min(b.1) < r.y1 - 0.5
                    && a.1.max(b.1) > r.y0 + 0.5
            }
        })
    };

    let xi = |x: f32| xs.iter().position(|v| (v - x).abs() < 0.6);
    let yi = |y: f32| ys.iter().position(|v| (v - y).abs() < 0.6);
    let (sx, sy) = (xi(from.0)?, yi(from.1)?);
    let (tx, ty) = (xi(to.0)?, yi(to.1)?);

    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let h = |x: usize, y: usize| -> f32 { (xs[x] - xs[tx]).abs() + (ys[y] - ys[ty]).abs() };
    let mut best: HashMap<(usize, usize, u8), f32> = HashMap::new();
    let mut prev: HashMap<(usize, usize, u8), (usize, usize, u8)> = HashMap::new();
    let mut heap: BinaryHeap<Reverse<(u64, usize, usize, u8)>> = BinaryHeap::new();

    best.insert((sx, sy, 0), 0.0);
    heap.push(Reverse((0, sx, sy, 0)));

    let mut goal: Option<(usize, usize, u8)> = None;
    while let Some(Reverse((_, cx, cy, cdir))) = heap.pop() {
        let key = (cx, cy, cdir);
        let cost = *best.get(&key).unwrap_or(&f32::INFINITY);
        if cx == tx && cy == ty {
            goal = Some(key);
            break;
        }
        for (dx, dy, ndir) in [(1i32, 0i32, 1u8), (-1, 0, 1), (0, 1, 2), (0, -1, 2)] {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx < 0 || ny < 0 || nx as usize >= xs.len() || ny as usize >= ys.len() {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            let a = (xs[cx], ys[cy]);
            let b = (xs[nx], ys[ny]);
            // The two port positions are always legal: a port sits on a frame border, so a
            // naive interior test rejects the endpoints themselves and no route is ever found.
            let is_endpoint = |x: usize, y: usize| (x == sx && y == sy) || (x == tx && y == ty);
            if blocked(a, b) || (!is_endpoint(nx, ny) && inside(b.0, b.1)) {
                continue;
            }
            let len = (b.0 - a.0).abs() + (b.1 - a.1).abs();
            let turn = if cdir != 0 && cdir != ndir { 26.0 } else { 0.0 };
            let ncost = cost + len + turn;
            let nkey = (nx, ny, ndir);
            if ncost < *best.get(&nkey).unwrap_or(&f32::INFINITY) {
                best.insert(nkey, ncost);
                prev.insert(nkey, key);
                heap.push(Reverse(((ncost + h(nx, ny)) as u64, nx, ny, ndir)));
            }
        }
    }

    let mut key = goal?;
    let mut pts = vec![(xs[key.0], ys[key.1])];
    while let Some(p) = prev.get(&key) {
        pts.push((xs[p.0], ys[p.1]));
        key = *p;
    }
    pts.reverse();
    // Collapse collinear runs so the emitted path has the fewest commands.
    let mut out: Vec<(f32, f32)> = Vec::new();
    for p in pts {
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            // Three points on one vertical or one horizontal run: the middle one adds nothing.
            if ((a.0 - b.0).abs() < 0.5 && (b.0 - p.0).abs() < 0.5)
                || ((a.1 - b.1).abs() < 0.5 && (b.1 - p.1).abs() < 0.5)
            {
                out.pop();
            }
        }
        out.push(p);
    }
    Some(out)
}

fn port_point(node: usize, port: Option<usize>, ports: &[Vec<(f32, f32)>]) -> Option<(f32, f32)> {
    ports[node].get(port?).copied()
}

/// How many columns a node's children are laid out in. One row is what makes routing
/// impossible: with every child on one line there is no horizontal free band anywhere, so a
/// wire between two children has to cut through a third.
fn grid_cols(n: usize) -> usize {
    if n <= 2 {
        n.max(1)
    } else {
        (n as f32).sqrt().ceil() as usize
    }
}

/// Child positions inside a node, as (index, dx, dy). Computed once per node and reused by both
/// the measurement pass and the placement pass so the two can never disagree.
fn iv_grid(g: &Graph, i: usize, content_y: f32) -> Vec<(usize, f32, f32)> {
    let kids = &g.nodes[i].children;
    let cols = grid_cols(kids.len()).max(1);
    let sizes: Vec<(f32, f32)> = kids.iter().map(|&c| measure_iv(g, c)).collect();

    let mut col_w = vec![0.0f32; cols];
    for (k, (w, _)) in sizes.iter().enumerate() {
        col_w[k % cols] = col_w[k % cols].max(*w);
    }
    let rows = kids.len().div_ceil(cols);
    let mut row_h = vec![0.0f32; rows];
    for (k, (_, h)) in sizes.iter().enumerate() {
        row_h[k / cols] = row_h[k / cols].max(*h);
    }

    let mut out = Vec::with_capacity(kids.len());
    let mut y = content_y;
    for r in 0..rows {
        let mut x = PAD;
        for c in 0..cols {
            let k = r * cols + c;
            if k >= kids.len() {
                break;
            }
            out.push((k, x, y));
            x += col_w[c] + LANE_W;
        }
        y += row_h[r] + LANE_V;
    }
    out
}

fn measure_iv(g: &Graph, i: usize) -> (f32, f32) {
    let (own_w, own_h) = node_size(&g.nodes[i]);
    if g.nodes[i].children.is_empty() {
        return (own_w, own_h);
    }
    let kids = &g.nodes[i].children;
    let cols = grid_cols(kids.len()).max(1);
    let sizes: Vec<(f32, f32)> = kids.iter().map(|&c| measure_iv(g, c)).collect();

    let mut col_w = vec![0.0f32; cols];
    for (k, (w, _)) in sizes.iter().enumerate() {
        col_w[k % cols] = col_w[k % cols].max(*w);
    }
    let rows = kids.len().div_ceil(cols);
    let mut row_h = vec![0.0f32; rows];
    for (k, (_, h)) in sizes.iter().enumerate() {
        row_h[k / cols] = row_h[k / cols].max(*h);
    }

    let grid_w: f32 = col_w.iter().sum::<f32>() + LANE_W * (cols.saturating_sub(1)) as f32;
    let grid_h: f32 = row_h.iter().sum::<f32>() + LANE_V * (rows.saturating_sub(1)) as f32;
    (own_w.max(grid_w + 2.0 * PAD), own_h + grid_h + 2.0 * PAD)
}

fn place_iv(
    g: &Graph,
    i: usize,
    x: f32,
    y: f32,
    out: &mut String,
    geometry: &mut [Option<(f32, f32, f32, f32)>],
    port_slots: &mut [Vec<(f32, f32)>],
    labels: &mut Vec<PortLabel>,
) {
    let (w, h) = measure_iv(g, i);
    geometry[i] = Some((x, y, w, h));
    let content_bottom = draw_frame(g, i, x, y, w, h, out, &mut port_slots[i], labels);

    for (k, dx, dy) in iv_grid(g, i, content_bottom + PAD) {
        place_iv(
            g,
            g.nodes[i].children[k],
            x + dx,
            dy,
            out,
            geometry,
            port_slots,
            labels,
        );
    }
}

/// Paint the port names, each on a white plate, after everything else. A connector may cross a
/// port's row; painted here, the plate hides the line instead of the line hiding the name.
fn paint_port_labels(labels: &[PortLabel]) -> String {
    let mut out = String::new();
    for label in labels {
        let w = text_w(&label.text, 9.5) + 4.0;
        let x0 = match label.side {
            PortSide::Left => label.x - w + 2.0,
            PortSide::Right => label.x - 2.0,
        };
        out.push_str(&format!(
            "<rect x=\"{x0:.1}\" y=\"{:.1}\" width=\"{w:.1}\" height=\"12.0\" fill=\"#FFFFFF\" stroke=\"none\"/>",
            label.y - 9.0
        ));
        let anchor = match label.side {
            PortSide::Left => "end",
            PortSide::Right => "start",
        };
        out.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"{anchor}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9.5\" fill=\"#000000\">{}</text>",
            label.x,
            label.y,
            esc(&label.text)
        ));
    }
    out
}

/// Height the view frame adds above the drawing: room for the `«view» Name` tab.
const VIEW_TAB: f32 = 26.0;

fn document(width: f32, height: f32, title: &str, kind: &str, body: &str) -> String {
    // The picture states its own provenance: which view it is, and what it was narrowed to. The
    // reference draws a tab at the top-left and an `expose` strip at the bottom-right; without
    // them a reader cannot tell a general view from an interconnection view, and cannot tell a
    // filtered picture from a complete one.
    let frame_w = width;
    let frame_h = height + VIEW_TAB;
    let inner = format!(
        r##"<g transform="translate(0,{VIEW_TAB:.0})">{body}</g>"##,
        body = body
    );
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{frame_w:.0}" height="{frame_h:.0}" viewBox="0 0 {frame_w:.0} {frame_h:.0}">
<rect width="100%" height="100%" fill="#FFFFFF"/>
<rect x="1" y="1" width="{:.0}" height="{:.0}" rx="6" fill="none" stroke="#C8DCE6" stroke-width="1"/>
<path d="M1 18 Q1 1 18 1 L{:.0} 1 Q{:.0} 1 {:.0} 18 L1 18 Z" fill="#E8F0F8" stroke="#C8DCE6" stroke-width="1"/>
<text x="10" y="15" font-family="Helvetica, Arial, sans-serif" font-size="10" fill="#1A1A1A">«view» {view} — {title}</text>
{inner}
<rect x="{:.0}" y="{:.0}" width="{:.0}" height="16" rx="3" fill="#E8F0F8" stroke="#C8DCE6" stroke-width="1"/>
<text x="{:.0}" y="{:.0}" text-anchor="end" font-family="Helvetica, Arial, sans-serif" font-size="9" fill="#666666">expose ModelContext::*::**</text>
</svg>"##,
        frame_w - 2.0,
        frame_h - 2.0,
        frame_w - 2.0,
        frame_w - 2.0,
        frame_w - 2.0,
        // The strip is drawn after the drawing so it is never covered by it, and it sits in the
        // bottom-right corner inside the frame.
        frame_w - 158.0,
        frame_h - 20.0,
        152.0,
        frame_w - 10.0,
        frame_h - 8.0,
        view = esc(kind),
        title = esc(title),
    )
}

/// Requirements on the left, implementers on the right, one line per requirement.
///
/// Laid out as two columns rather than a force-directed graph: the question this view answers is
/// "which of these is unmet", and that is a scan down a list, not a reading of a network. A
/// requirement with no satisfier keeps its row and gets a mark, because the absence is the fact
/// worth seeing — dropping it would make the picture look complete.
fn render_traceability(g: &Graph, title: &str) -> String {
    // Node indexes are laid out in model order: requirements first (pushed by `traceability`),
    // then implementers. Rather than re-deriving that split here, read it from the edges: a node
    // with any outgoing `satisfy` edge is a requirement, and one with only incoming edges is an
    // implementer. A requirement with no edge at all is still a requirement, which is why the
    // trace list is passed in.
    let mut out_degree = vec![0usize; g.nodes.len()];
    let mut in_degree = vec![0usize; g.nodes.len()];
    for edge in &g.edges {
        out_degree[edge.from] += 1;
        in_degree[edge.to] += 1;
    }

    // A requirement is a node whose stereotype says so; the projection only ever pushes
    // requirement kinds for the left column.
    let is_requirement = |i: usize| g.nodes[i].stereotype.starts_with("requirement");

    let mut left: Vec<usize> = Vec::new();
    let mut right: Vec<usize> = Vec::new();
    for (i, _) in g.nodes.iter().enumerate() {
        if is_requirement(i) || out_degree[i] > 0 {
            left.push(i);
        } else {
            right.push(i);
        }
    }

    let row_h = 46.0;
    let left_w = left
        .iter()
        .map(|&i| {
            text_w(&g.nodes[i].label(), 12.0)
                .max(text_w(g.nodes[i].constraint.as_deref().unwrap_or(""), 9.5))
        })
        .fold(MIN_W, f32::max)
        + 34.0;
    let right_w = right
        .iter()
        .map(|&i| text_w(&g.nodes[i].label(), 12.0))
        .fold(MIN_W, f32::max)
        + 26.0;
    let gap = 120.0;
    let left_x = MARGIN;
    let right_x = left_x + left_w + gap;
    let top = MARGIN + 28.0;
    let height = top + (left.len().max(1) as f32) * row_h + MARGIN;
    let width = right_x + right_w + MARGIN;

    let mut body = String::new();

    // Column headings, so the two sides are never ambiguous.
    for (x, text) in [(left_x, "requirement"), (right_x, "satisfied by")] {
        body.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9\" fill=\"#666666\">{}</text>",
            top - 12.0,
            esc(text)
        ));
    }

    let mut row_of: Vec<Option<usize>> = vec![None; g.nodes.len()];
    for (row, &node) in left.iter().enumerate() {
        let y = top + row as f32 * row_h;
        row_of[node] = Some(row);
        let n = &g.nodes[node];
        let unmet = out_degree[node] == 0;

        // A requirement nothing satisfies is marked, not hidden. The mark is the whole point of
        // the view: an empty row that reads as "not applicable" would hide the gap.
        let (stroke, dash) = if unmet {
            ("#A03030", " stroke-dasharray=\"4 3\"")
        } else {
            ("#000000", "")
        };
        body.push_str(&format!(
            "<rect x=\"{left_x:.1}\" y=\"{:.1}\" width=\"{left_w:.1}\" height=\"34\" fill=\"#FFFFFF\" stroke=\"{stroke}\" stroke-width=\"1\"{dash}/>",
            y - 12.0
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9\" fill=\"#333333\">«{}»</text>",
            left_x + 8.0,
            y - 1.0,
            esc(&n.stereotype)
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"12\" fill=\"#000000\">{}</text>",
            left_x + 8.0,
            y + 13.0,
            esc(n.label())
        ));
        if let Some(predicate) = &n.constraint {
            body.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9.5\" fill=\"#333333\">{}</text>",
                left_x + 8.0,
                y + 26.0,
                esc(predicate)
            ));
        }
        if unmet {
            body.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9\" fill=\"#A03030\">not satisfied by anything</text>",
                left_x + left_w + 8.0,
                y + 8.0
            ));
        }
    }

    // Implementers sit at the vertical centre of the requirements they serve, so the lines run
    // with as little slope as the picture allows.
    // Two implementers can want the same y — a requirement's subject and the part that satisfies
    // it are different elements, and both hang off the same row. Placing them at the same
    // coordinate drew two boxes exactly on top of each other, which reads as one box with
    // doubled text. Push a collision down by one row until it is free.
    let mut right_row: std::collections::BTreeMap<usize, f32> = Default::default();
    let mut taken: Vec<f32> = Vec::new();
    for (i, &node) in right.iter().enumerate() {
        let served: Vec<f32> = g
            .edges
            .iter()
            .filter(|e| e.to == node)
            .filter_map(|e| row_of[e.from].map(|r| top + r as f32 * row_h))
            .collect();
        let mut y = if served.is_empty() {
            top + i as f32 * row_h
        } else {
            served.iter().sum::<f32>() / served.len() as f32
        };
        while taken.iter().any(|t| (t - y).abs() < row_h / 2.0) {
            y += row_h;
        }
        taken.push(y);
        right_row.insert(node, y);
        right_row.insert(node, y);
        let n = &g.nodes[node];
        body.push_str(&format!(
            "<rect x=\"{right_x:.1}\" y=\"{:.1}\" width=\"{right_w:.1}\" height=\"34\" rx=\"8\" fill=\"#FFFFFF\" stroke=\"#000000\" stroke-width=\"1\"/>",
            y - 12.0
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9\" fill=\"#333333\">«{}»</text>",
            right_x + 8.0,
            y - 1.0,
            esc(&n.stereotype)
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"12\" fill=\"#000000\">{}</text>",
            right_x + 8.0,
            y + 13.0,
            esc(n.label())
        ));
    }

    // One line per satisfaction, drawn after both columns so it runs over neither box.
    for edge in &g.edges {
        let (Some(from_row), Some(&to_y)) = (row_of[edge.from], right_row.get(&edge.to)) else {
            continue;
        };
        let y = top + from_row as f32 * row_h;
        let x0 = left_x + left_w;
        let x1 = right_x;
        let mid = (x0 + x1) / 2.0;
        body.push_str(&format!(
            "<path d=\"M{x0:.1} {y:.1} L{mid:.1} {y:.1} L{mid:.1} {to_y:.1} L{x1:.1} {to_y:.1}\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1\"/>"
        ));
        // The open arrow the notation uses for `satisfy`, pointing at the implementer.
        body.push_str(&format!(
            "<path d=\"M{x1:.1} {to_y:.1} l-4,-4 M{x1:.1} {to_y:.1} l-4,4\" stroke=\"#000000\" stroke-width=\"1.1\" fill=\"none\"/>"
        ));
    }

    document(width, height, title, "traceability view", &body)
}

/// Boxes and connectors for the traceability view, in content coordinates.
///
/// Same contract as `general_geometry`: the export and the canvas both call this, so the two draw
/// one picture. The layout is two columns — requirements on the left, their satisfiers on the
/// right — because the question the view answers ("which of these is unmet") is a scan down a
/// list, not a reading of a network.
pub struct TraceGeometry {
    pub boxes: Vec<(usize, f32, f32, f32, f32)>,
    pub connectors: Vec<(usize, Vec<(f32, f32)>)>,
    /// Requirements with no satisfier, so a caller can mark them.
    pub unmet: Vec<usize>,
    pub width: f32,
    pub height: f32,
}

pub fn traceability_geometry(g: &Graph) -> TraceGeometry {
    let is_requirement = |i: usize| g.nodes[i].stereotype.starts_with("requirement");
    let mut out_degree = vec![0usize; g.nodes.len()];
    for edge in &g.edges {
        out_degree[edge.from] += 1;
    }
    let mut left: Vec<usize> = Vec::new();
    let mut right: Vec<usize> = Vec::new();
    for i in 0..g.nodes.len() {
        if is_requirement(i) || out_degree[i] > 0 {
            left.push(i);
        } else {
            right.push(i);
        }
    }

    let row_h = 46.0;
    let left_w = left
        .iter()
        .map(|&i| {
            text_w(&g.nodes[i].label(), 12.0)
                .max(text_w(g.nodes[i].constraint.as_deref().unwrap_or(""), 9.5))
        })
        .fold(MIN_W, f32::max)
        + 34.0;
    let right_w = right
        .iter()
        .map(|&i| text_w(&g.nodes[i].label(), 12.0))
        .fold(MIN_W, f32::max)
        + 26.0;
    let gap = 120.0;
    let left_x = MARGIN;
    let right_x = left_x + left_w + gap;
    let top = MARGIN + 28.0;
    let height = top + (left.len().max(1) as f32) * row_h + MARGIN;
    let width = right_x + right_w + MARGIN;

    let mut boxes = Vec::new();
    let mut row_of: std::collections::BTreeMap<usize, usize> = Default::default();
    for (row, &node) in left.iter().enumerate() {
        let y = top + row as f32 * row_h;
        row_of.insert(node, row);
        // 34 tall, matching the box the renderer draws; the row pitch leaves room for a
        // constraint line underneath without the next box touching it.
        boxes.push((node, left_x, y - 12.0, left_w, 34.0));
    }
    let mut right_y: std::collections::BTreeMap<usize, f32> = Default::default();
    let mut taken: Vec<f32> = Vec::new();
    for (i, &node) in right.iter().enumerate() {
        let served: Vec<f32> = g
            .edges
            .iter()
            .filter(|e| e.to == node)
            .filter_map(|e| row_of.get(&e.from).map(|r| top + *r as f32 * row_h))
            .collect();
        let mut y = if served.is_empty() {
            top + i as f32 * row_h
        } else {
            served.iter().sum::<f32>() / served.len() as f32
        };
        // Two implementers can want the same y: a requirement's subject and the part that
        // satisfies it are different elements hanging off the same row. Without this they drew
        // as one box with doubled text. Push a collision down a row until it is free.
        while taken.iter().any(|t| (t - y).abs() < row_h / 2.0) {
            y += row_h;
        }
        taken.push(y);
        right_y.insert(node, y);
        boxes.push((node, right_x, y - 12.0, right_w, 34.0));
    }

    let mut connectors = Vec::new();
    for (index, edge) in g.edges.iter().enumerate() {
        let (Some(row), Some(&to_y)) = (row_of.get(&edge.from), right_y.get(&edge.to)) else {
            continue;
        };
        let y = top + *row as f32 * row_h;
        let x0 = left_x + left_w;
        let x1 = right_x;
        let mid = (x0 + x1) / 2.0;
        connectors.push((index, vec![(x0, y), (mid, y), (mid, to_y), (x1, to_y)]));
    }

    let unmet = left
        .iter()
        .copied()
        .filter(|&i| out_degree[i] == 0)
        .collect();

    TraceGeometry {
        boxes,
        connectors,
        unmet,
        width,
        height,
    }
}

/// The namespace tree, packages as nested frames.
///
/// Drawn as frames rather than as boxes-and-arrows: the question is "what is inside this package",
/// and containment is what the picture has to show. An arrow between two package boxes would make
/// a reader trace lines to answer a question the geometry can answer by itself.
///
/// The geometry is `packages::packages_geometry`, the same function the studio calls, so the
/// exported file and the canvas place every frame and every box in the same spot. Re-deriving the
/// layout here would let the two drift apart on the first change to either.
fn render_packages(g: &Graph, title: &str) -> String {
    let frames = packages::frames_of(g);
    let geo = packages::packages_geometry(g, &frames);
    let mut body = String::new();

    // Frames in one pass, boxes in the next: a member box then sits on top of its own frame no
    // matter what order the projection emitted the nodes in.
    for (node, x, y, w, h) in &geo.frames {
        let node = &g.nodes[*node];
        // A state and an action are different kinds, and the reference draws them in different
        // colours; before this, every region in both views was the same off-white.
        let style = crate::style::for_stereotype(&node.stereotype);
        body.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\"/>",
            style.body, style.border
        ));
        body.push_str(&format!(
            "<line x1=\"{x:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#000000\" stroke-width=\"0.8\"/>",
            y + packages::TITLE,
            x + w,
            y + packages::TITLE
        ));
        // The name sits in the title band, left-aligned: package names are read as a path
        // (`Top::Sub`), and a centred name would move when a sibling frame grows.
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"12\" fill=\"#000000\">{}</text>",
            x + packages::TITLE_INSET,
            y + packages::TITLE - 6.0,
            esc(node.label())
        ));
    }

    for (node, x, y, w, h) in &geo.boxes {
        let node = &g.nodes[*node];
        body.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"{}\" fill=\"#FFFFFF\" stroke=\"#000000\" stroke-width=\"1\"/>",
            if node.is_def { 0 } else { 8 }
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9\" fill=\"#333333\">«{}»</text>",
            x + w / 2.0,
            y + 13.0,
            esc(&node.stereotype)
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"12\" fill=\"#000000\">{}</text>",
            x + w / 2.0,
            y + 27.0,
            esc(node.label())
        ));
    }

    document(geo.width, geo.height, title, "package view", &body)
}

pub fn render(g: &Graph, title: &str, style: Style) -> String {
    // Only the general view has a second layout today; the other views lay out their own
    // shapes (lifelines, regions, lanes) and draw them the one way.
    match g.view {
        View::General => render_general(g, title, style),
        View::Interconnection => render_interconnection(g, title),
        View::Traceability => render_traceability(g, title),
        View::Packages => render_packages(g, title),
        View::StateTransition => render_states(g, title),
        View::ActionFlow => render_actions(g, title),
        View::Sequence => render_sequence(g, title),
    }
}

/// Boxes and edges for the state transition view, in content coordinates.
///
/// States sit on a circle inside their machine's frame. A circle rather than the layered layout
/// because a state machine is a graph, not a hierarchy: transitions run backwards (`fault then
/// off` returns to a state already placed), and a rank assignment either breaks the cycle by
/// dropping an edge or draws it as a back edge through everything. The circle has no such
/// preference — every state is one step from its neighbours, which is also how state machines are
/// read.
///
/// Same contract as the other geometry functions: the export and the canvas both call this.
pub struct StateGeometry {
    /// `(node, x, y, w, h)` for every state box.
    pub boxes: Vec<(usize, f32, f32, f32, f32)>,
    /// `(edge index, points)` for every transition.
    pub edges: BTreeMap<usize, Vec<(f32, f32)>>,
    /// `(node, x, y, w, h)` for every machine frame, drawn behind its states.
    pub frames: Vec<(usize, f32, f32, f32, f32)>,
    pub width: f32,
    pub height: f32,
}

const STATE_PAD: f32 = 14.0;
const STATE_TITLE: f32 = 22.0;
const STATE_GAP: f32 = 26.0;
const STATE_MARGIN: f32 = 48.0;
const STATE_MIN_W: f32 = 88.0;
const STATE_BOX_H: f32 = 34.0;

fn state_size(node: &Node) -> (f32, f32) {
    let mut w =
        text_w(&format!("«{}»", node.stereotype), 9.0).max(text_w(node.label(), 12.0)) + 26.0;
    if let Some(constraint) = &node.constraint {
        w = w.max(text_w(constraint, 9.5) + 24.0);
    }
    (w.max(STATE_MIN_W), STATE_BOX_H)
}

pub fn states_geometry(g: &Graph, groups: &[Group]) -> StateGeometry {
    let mut boxes: Vec<(usize, f32, f32, f32, f32)> = Vec::new();
    let mut frames: Vec<(usize, f32, f32, f32, f32)> = Vec::new();
    let mut placed: BTreeMap<usize, (f32, f32, f32, f32)> = BTreeMap::new();

    let mut cursor = STATE_MARGIN;
    let mut widest = 0.0f32;
    for machine in groups {
        let n = machine.members.len();
        // Room for the widest box on the circle, on both sides.
        let half = machine
            .members
            .iter()
            .map(|&s| state_size(&g.nodes[s]).0 / 2.0)
            .fold(0.0f32, f32::max);
        // Adjacent boxes are a chord apart, so the radius has to clear their width: a circle
        // sized only by the count of states drew three 88-wide boxes 89 units apart and the
        // transitions between neighbours collapsed to a couple of pixels.
        let half_turn = std::f32::consts::PI / (n.max(1) as f32);
        let radius = if n < 2 {
            0.0
        } else {
            ((half * 2.0 + 34.0) / (2.0 * half_turn.sin())).max(90.0)
        };
        let centre_x = STATE_MARGIN + radius + half + STATE_PAD;
        let centre_y = cursor + STATE_TITLE + STATE_PAD + radius + STATE_BOX_H / 2.0;

        for (i, &state) in machine.members.iter().enumerate() {
            let (w, h) = state_size(&g.nodes[state]);
            let angle = -std::f32::consts::FRAC_PI_2
                + (i as f32) * std::f32::consts::TAU / (n.max(1) as f32);
            let (x, y) = if n < 2 {
                (centre_x - w / 2.0, centre_y - h / 2.0)
            } else {
                (
                    centre_x + radius * angle.cos() - w / 2.0,
                    centre_y + radius * angle.sin() - h / 2.0,
                )
            };
            boxes.push((state, x, y, w, h));
            placed.insert(state, (x, y, w, h));
        }

        // The frame is the bounding box of what it holds, plus the title band on top.
        let (mut lo_x, mut lo_y) = (f32::MAX, f32::MAX);
        let (mut hi_x, mut hi_y) = (f32::MIN, f32::MIN);
        for &state in &machine.members {
            if let Some(&(x, y, w, h)) = placed.get(&state) {
                lo_x = lo_x.min(x);
                lo_y = lo_y.min(y);
                hi_x = hi_x.max(x + w);
                hi_y = hi_y.max(y + h);
            }
        }
        if lo_x == f32::MAX {
            lo_x = centre_x;
            lo_y = cursor + STATE_TITLE;
            hi_x = centre_x + STATE_MIN_W;
            hi_y = lo_y + STATE_BOX_H;
        }
        let fx = lo_x - STATE_PAD;
        let fy = lo_y - STATE_PAD - STATE_TITLE;
        let fw = (hi_x + STATE_PAD) - fx;
        let fh = (hi_y + STATE_PAD) - fy;
        frames.push((machine.node, fx, fy, fw, fh));
        widest = widest.max(fx + fw);
        cursor = fy + fh + STATE_GAP;
    }

    let mut edges = BTreeMap::new();
    for (index, edge) in g.edges.iter().enumerate() {
        let (Some(&(fx, fy, fw, fh)), Some(&(tx, ty, tw, th))) =
            (placed.get(&edge.from), placed.get(&edge.to))
        else {
            continue;
        };
        let points = if edge.from == edge.to {
            self_loop(fx, fy, fw, fh)
        } else {
            let (a, b) = (
                (fx + fw / 2.0, fy + fh / 2.0),
                (tx + tw / 2.0, ty + th / 2.0),
            );
            vec![edge_point(a, b, fw, fh), edge_point(b, a, tw, th)]
        };
        edges.insert(index, points);
    }

    let width = widest + STATE_MARGIN;
    let height = cursor + STATE_MARGIN;
    StateGeometry {
        boxes,
        edges,
        frames,
        width,
        height,
    }
}

/// Where the line from `from` towards `to` leaves the box — the point on the border, so the
/// arrowhead lands on the edge of the target instead of its centre.
fn edge_point(from: (f32, f32), to: (f32, f32), w: f32, h: f32) -> (f32, f32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let mut scale = f32::MAX;
    if dx.abs() > 0.01 {
        scale = scale.min((w / 2.0) / dx.abs());
    }
    if dy.abs() > 0.01 {
        scale = scale.min((h / 2.0) / dy.abs());
    }
    if !scale.is_finite() {
        return from;
    }
    (from.0 + dx * scale, from.1 + dy * scale)
}

/// A transition from a state to itself leaves the top edge and comes back, which is the usual
/// notation and needs no arc machinery.
fn self_loop(x: f32, y: f32, w: f32, _h: f32) -> Vec<(f32, f32)> {
    let out = 18.0;
    let (left, right) = (x + w * 0.35, x + w * 0.65);
    vec![
        (left, y),
        (left - 6.0, y - out),
        (right + 6.0, y - out),
        (right, y),
    ]
}

/// The state transition view.
///
/// A machine is a frame with its states on a circle inside it, and every transition is one arrow
/// from the state it leaves to the state it enters. The guard (or the trigger, when there is no
/// guard) sits on the edge, because that is what decides whether the transition is taken and it is
/// the first thing a reader asks about a transition.
fn render_states(g: &Graph, title: &str) -> String {
    render_behaviour(g, title, "state transition view")
}

fn render_actions(g: &Graph, title: &str) -> String {
    render_behaviour(g, title, "action flow view")
}

fn render_behaviour(g: &Graph, title: &str, kind: &str) -> String {
    let groups = states::groups_of(g);
    let geo = states_geometry(g, &groups);
    let mut body = String::new();

    for (node, x, y, w, h) in &geo.frames {
        let node = &g.nodes[*node];
        // A state and an action are different kinds, and the reference draws them in different
        // colours; before this, every region in both views was the same off-white.
        let style = crate::style::for_stereotype(&node.stereotype);
        body.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\"/>",
            style.body, style.border
        ));
        body.push_str(&format!(
            "<line x1=\"{x:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#000000\" stroke-width=\"0.8\"/>",
            y + STATE_TITLE,
            x + w,
            y + STATE_TITLE
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"12\" fill=\"#000000\">{}</text>",
            x + 8.0,
            y + STATE_TITLE - 6.0,
            esc(node.label())
        ));
    }

    for (edge, points) in &geo.edges {
        let points: Vec<String> = points
            .iter()
            .map(|(x, y)| format!("{x:.1},{y:.1}"))
            .collect();
        body.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1\"/>",
            points.join(" ")
        ));
        // The arrowhead goes on the last segment, pointing at the target box.
        if let Some((a, b)) = {
            let last = &geo.edges[edge];
            Some((last[last.len() - 2], last[last.len() - 1]))
        } {
            body.push_str(&arrow_head(a, b));
        }
        if let Some(label) = g.edges.get(*edge).and_then(|e| e.label.as_ref()) {
            let (a, b) = {
                let last = &geo.edges[edge];
                (last[0], last[last.len() - 1])
            };
            let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            let width = text_w(label, 9.0) + 6.0;
            body.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{width:.1}\" height=\"12\" fill=\"#FFFFFF\"/>",
                mx - width / 2.0,
                my - 10.0
            ));
            body.push_str(&format!(
                "<text x=\"{mx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9\" fill=\"#000000\">{}</text>",
                my - 1.0,
                esc(label)
            ));
        }
    }

    for (node, x, y, w, h) in &geo.boxes {
        let node = &g.nodes[*node];
        body.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"8\" fill=\"#FFFFFF\" stroke=\"#000000\" stroke-width=\"1\"/>"
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9\" fill=\"#333333\">«{}»</text>",
            x + w / 2.0,
            y + 13.0,
            esc(&node.stereotype)
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"12\" fill=\"#000000\">{}</text>",
            x + w / 2.0,
            y + 27.0,
            esc(node.label())
        ));
    }

    document(geo.width, geo.height, title, kind, &body)
}

/// An open arrowhead at `to`, pointing along the segment from `from`.
fn arrow_head(from: (f32, f32), to: (f32, f32)) -> String {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let length = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / length, dy / length);
    let len = 8.0;
    let wing = 4.0;
    let (px, py) = (-uy, ux);
    format!(
        "<path d=\"M{:.1},{:.1} L{:.1},{:.1} M{:.1},{:.1} L{:.1},{:.1}\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1\"/>",
        to.0,
        to.1,
        to.0 - ux * len + px * wing,
        to.1 - uy * len + py * wing,
        to.0,
        to.1,
        to.0 - ux * len - px * wing,
        to.1 - uy * len - py * wing
    )
}

/// Lifelines and messages, in content coordinates.
///
/// Each participant is one tall box running the height of the diagram, with its name at the top:
/// the sides of that box are the lifeline, so no separate line element is needed and the canvas —
/// which draws boxes and polylines — can render the view without learning a new shape. Messages
/// are horizontal arrows from one lifeline to another, one row per message, in source order.
pub struct SequenceGeometry {
    /// `(node, x, y, w, h)` per participant.
    pub boxes: Vec<(usize, f32, f32, f32, f32)>,
    /// `(edge index, points)` per message.
    pub edges: BTreeMap<usize, Vec<(f32, f32)>>,
    pub width: f32,
    pub height: f32,
}

const SEQ_COL_W: f32 = 150.0;
pub const SEQ_HEAD: f32 = 46.0;
const SEQ_ROW: f32 = 34.0;
const SEQ_MARGIN: f32 = 48.0;

pub fn sequence_geometry(g: &Graph) -> SequenceGeometry {
    let count = g.nodes.len();
    let rows = g.edges.len();
    let height = SEQ_MARGIN * 2.0 + SEQ_HEAD + (rows as f32).max(1.0) * SEQ_ROW + SEQ_ROW;
    let width = SEQ_MARGIN * 2.0 + (count as f32).max(1.0) * SEQ_COL_W;

    let mut boxes = Vec::new();
    let mut centre = BTreeMap::new();
    for index in 0..count {
        let x = SEQ_MARGIN + index as f32 * SEQ_COL_W;
        boxes.push((
            index,
            x,
            SEQ_MARGIN,
            SEQ_COL_W - 24.0,
            height - SEQ_MARGIN * 2.0,
        ));
        centre.insert(index, x + (SEQ_COL_W - 24.0) / 2.0);
    }

    let mut edges = BTreeMap::new();
    for (row, edge) in g.edges.iter().enumerate() {
        let y = SEQ_MARGIN + SEQ_HEAD + (row as f32 + 1.0) * SEQ_ROW;
        let (Some(&from), Some(&to)) = (centre.get(&edge.from), centre.get(&edge.to)) else {
            continue;
        };
        // Centre to centre: the dashed line inside each box is the lifeline, and a message runs
        // between lifelines. Stopping at the box edges would leave the arrow spanning only the gap
        // between two columns, which is not the distance between the participants.
        let points = if edge.from == edge.to {
            let half = (SEQ_COL_W - 24.0) / 2.0;
            vec![
                (from, y),
                (from + half + 16.0, y - 10.0),
                (from + half + 16.0, y + 10.0),
                (from, y + 4.0),
            ]
        } else {
            vec![(from, y), (to, y)]
        };
        edges.insert(row, points);
    }

    SequenceGeometry {
        boxes,
        edges,
        width,
        height,
    }
}

/// The sequence view.
///
/// A participant is a tall box: its two sides are the lifeline, so the same rectangle that carries
/// the name also carries the time the messages are placed against. A dashed centre line inside each
/// box makes the lifeline read as one even where no message crosses it.
fn render_sequence(g: &Graph, title: &str) -> String {
    let geo = sequence_geometry(g);
    let mut body = String::new();

    for (node, x, y, w, h) in &geo.boxes {
        let node = &g.nodes[*node];
        body.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"#FFFFFF\" stroke=\"#000000\" stroke-width=\"1\"/>"
        ));
        let cx = x + w / 2.0;
        body.push_str(&format!(
            "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" stroke=\"#888888\" stroke-width=\"0.8\" stroke-dasharray=\"4 4\"/>",
            y + SEQ_HEAD,
            y + h
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"12\" fill=\"#000000\">{}</text>",
            x + w / 2.0,
            y + 22.0,
            esc(node.label())
        ));
    }

    for (edge, points) in &geo.edges {
        let points: Vec<String> = points
            .iter()
            .map(|(x, y)| format!("{x:.1},{y:.1}"))
            .collect();
        body.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1\"/>",
            points.join(" ")
        ));
        let last = &geo.edges[edge];
        let (a, b) = (last[last.len() - 2], last[last.len() - 1]);
        body.push_str(&arrow_head(a, b));
        if let Some(label) = g.edges.get(*edge).and_then(|e| e.label.as_ref()) {
            let width = text_w(label, 9.0) + 8.0;
            let (mx, my) = ((a.0 + b.0) / 2.0, a.1);
            body.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{width:.1}\" height=\"12\" fill=\"#FFFFFF\"/>",
                mx - width / 2.0,
                my - 14.0
            ));
            body.push_str(&format!(
                "<text x=\"{mx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"9\" fill=\"#000000\">{}</text>",
                my - 5.0,
                esc(label)
            ));
        }
    }

    document(geo.width, geo.height, title, "sequence view", &body)
}
