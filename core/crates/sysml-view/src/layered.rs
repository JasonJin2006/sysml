//! Layered layout — the Sugiyama framework, the pipeline `dot` uses.
//!
//! The general view is a small DAG: ownership goes from an owner to the features declared in
//! its body, typing from a usage to the definition it is typed by, satisfy from the satisfying
//! element to the requirement. Drawing it well needs five decisions in this order, and getting
//! the first one wrong is what made the old drawing hard to read.
//!
//! 1. **Break cycles.** `part def A { part b : B; }` with `part def B { part a : A; }` closes a
//!    loop. A greedy feedback-arc set (Eades–Lin–Smyth) yields a linear order; edges pointing
//!    backwards in it are flipped for layout and remembered, so the symbol still lands on the
//!    node the model names.
//! 2. **Rank.** Longest path over the acyclic graph, then a bounded median descent towards the
//!    neighbours' ranks. That is the objective network simplex minimises (total edge length
//!    under `rank(v) > rank(u)`), reached without the tableau. Ranks come from the *graph*, not
//!    from composite depth: `drone : Drone` sits above `Drone`, which is how the relationship
//!    reads. Ranking by depth put them side by side and forced every typing edge through the
//!    header.
//! 3. **Virtual nodes.** An edge spanning several ranks becomes a chain of unit hops, one
//!    virtual node per intervening rank. That reserves a column for a long edge before anything
//!    is placed; the old code instead hunted for a free column at draw time, which is how a line
//!    ended up threading through whatever happened to be in the way.
//! 4. **Order.** Median heuristic plus adjacent transposition, keeping the best ordering seen.
//!    The objective is the crossing count, the number that actually predicts readability.
//! 5. **X coordinates.** Brandes–Köpf, *Fast and Simple Horizontal Coordinate Assignment*
//!    (2001): four alignments (down/up × right/left), median of the four, then a packing pass.
//!    Aligning a node with the median of its neighbours is what straightens a virtual chain into
//!    one vertical run instead of a staircase.
//!
//! Then geometry: lanes for horizontal runs (interval colouring on x-spans, so two runs
//! overlapping in x never share a y), and attachment slots (a node can be an owner and a type at
//! once, so its border may carry both a diamond and a triangle).

use std::collections::HashMap;

use crate::graph::Arrow;

/// Horizontal gap between two real boxes in the same rank.
pub const SEP: f32 = 34.0;
/// Horizontal gap between two virtual nodes; smaller than `SEP` so parallel long edges stay
/// bundled instead of spreading across the canvas.
pub const SEP_V: f32 = 16.0;
/// Vertical distance between two lanes in one gap.
pub const LANE_STEP: f32 = 15.0;
/// Free space at each end of a gap, outside its lanes.
pub const LANE_PAD: f32 = 26.0;
/// Smallest gap between two ranks, used when a gap carries no lanes.
pub const MIN_GAP: f32 = 54.0;
/// Space above the first rank before header lanes are counted.
pub const HEADER: f32 = 92.0;
/// Horizontal distance between two attachment slots on the same side of a node.
const SLOT_STEP: f32 = 26.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Side {
    Top,
    Bottom,
}

#[derive(Clone)]
pub struct LNode {
    /// Index into the caller's node list, or `None` for a virtual node.
    pub real: Option<usize>,
    pub w: f32,
    pub h: f32,
    pub rank: usize,
    pub x: f32,
    pub y: f32,
}

impl LNode {
    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }
    pub fn top(&self) -> f32 {
        self.y
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn is_virtual(&self) -> bool {
        self.real.is_none()
    }
}

/// A relationship symbol, positioned at one end of an edge's polyline.
#[derive(Clone, Copy)]
pub struct Symbol {
    /// `true` for the first point of the polyline, `false` for the last.
    pub at_start: bool,
    /// Which way the head points: down into a box's top border, or up into its bottom border.
    pub points_down: bool,
}

pub struct Route {
    /// Index into the caller's edge list.
    pub edge: usize,
    /// Orthogonal path from the upper end to the lower end.
    pub points: Vec<(f32, f32)>,
    /// Where the notation symbol goes, if the relationship has one.
    pub head: Option<Symbol>,
    /// Preferred position of the edge label.
    pub label: Option<(f32, f32)>,
}

/// One edge as the layout sees it.
pub struct Input {
    pub index: usize,
    pub from: usize,
    pub to: usize,
    pub kind: Arrow,
}

/// How the boxes are arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    /// Children drawn inside their owner's rectangle, ranks left to right.
    #[default]
    Nested,
    /// Every node on its own row, indented by ownership depth.
    Tree,
}

/// A horizontal run that needs a lane.
struct Group {
    gap: usize,
    lo: f32,
    hi: f32,
    lane: usize,
    y: f32,
}

/// One hop of an edge, from a node in rank `gap` to a node in rank `gap + 1`.
struct Hop {
    upper: usize,
    lower: usize,
    gap: usize,
}

pub struct Layered {
    pub nodes: Vec<LNode>,
    pub routes: Vec<Route>,
    pub width: f32,
    pub height: f32,
}

/// `preorder` is the DFS pre-order index of each node in the ownership forest.
///
/// Sibling contiguity is the one structural property a crossing-optimal ordering does not
/// protect on its own. Without it, the children of two different vehicles interleave along a
/// rank, and the ownership bus of each owner has to span the whole row to reach its own
/// features. Seeding the ordering with pre-order keeps each subtree in one block, which is what
/// makes "these parts belong to that vehicle" visible; the crossing minimiser then refines
/// inside those blocks.
pub fn build(
    sizes: &[(f32, f32)],
    edges: &[Input],
    preorder: &[usize],
    parent: &[Option<usize>],
) -> Layered {
    let n = sizes.len();
    if n == 0 {
        return Layered {
            nodes: Vec::new(),
            routes: Vec::new(),
            width: 160.0,
            height: 100.0,
        };
    }

    // ---- 1. edge list ---------------------------------------------------------------------
    // One entry per distinct ordered pair; `pair_edge` remembers which caller edge it stands
    // for, so a route can be attributed back to the relationship that produced it.
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut pair_edge: Vec<usize> = Vec::new();
    let mut pair_kind: Vec<Arrow> = Vec::new();
    for e in edges {
        if e.kind == Arrow::Connector || e.from == e.to {
            continue;
        }
        if !pairs.iter().any(|&(a, b)| a == e.from && b == e.to) {
            pairs.push((e.from, e.to));
            pair_edge.push(e.index);
            pair_kind.push(e.kind);
        }
    }

    // ---- 2. cycle breaking + ranking ------------------------------------------------------
    // Ranks come from the graph, not from composite depth. Longest path over the acyclic graph
    // is the constraint skeleton (every edge must descend), then a bounded median descent pulls
    // each node towards its neighbours' ranks — the objective network simplex minimises. Pure
    // depth ranking is not enough: `drone : Drone` and `Drone` would share a rank and every
    // typing edge would have to run sideways through the header.
    let sequence = eades_order(n, &pairs);
    let mut position = vec![0usize; n];
    for (slot, &node) in sequence.iter().enumerate() {
        position[node] = slot;
    }
    let mut reversed = vec![false; pairs.len()];
    let mut dag: Vec<(usize, usize)> = Vec::with_capacity(pairs.len());
    for (k, &(u, v)) in pairs.iter().enumerate() {
        if position[u] > position[v] {
            reversed[k] = true;
            dag.push((v, u));
        } else {
            dag.push((u, v));
        }
    }
    let rank = rank_nodes(n, &dag, &sequence);
    let _ = &pair_kind;

    // ---- 3. expand long edges into chains -------------------------------------------------
    let mut nodes: Vec<LNode> = (0..n)
        .map(|i| LNode {
            real: Some(i),
            w: sizes[i].0,
            h: sizes[i].1,
            rank: rank[i],
            x: 0.0,
            y: 0.0,
        })
        .collect();

    let mut hops_of: Vec<Vec<Hop>> = Vec::with_capacity(pairs.len());
    let mut chain: Vec<Vec<usize>> = Vec::with_capacity(pairs.len());
    for &(u, v) in &dag {
        let (ru, rv) = (rank[u], rank[v]);
        let mut list = Vec::new();
        let mut path = vec![u];
        let mut prev = u;
        for r in ru..rv {
            let next = if r + 1 == rv {
                v
            } else {
                let id = nodes.len();
                nodes.push(LNode {
                    real: None,
                    w: 1.0,
                    h: 0.0,
                    rank: r + 1,
                    x: 0.0,
                    y: 0.0,
                });
                id
            };
            list.push(Hop {
                upper: prev,
                lower: next,
                gap: r,
            });
            path.push(next);
            prev = next;
        }
        hops_of.push(list);
        chain.push(path);
    }

    let ranks = nodes.iter().map(|nd| nd.rank).max().unwrap_or(0) + 1;
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); ranks];
    for (id, nd) in nodes.iter().enumerate() {
        layers[nd.rank].push(id);
    }

    // ---- 4. ordering ----------------------------------------------------------------------
    let (up, down) = adjacent(&nodes, &hops_of);
    initial_order(&mut layers, &up, preorder, &nodes);
    minimize_crossings(&mut layers, &up, &down, 40);
    // Crossing count is not the only thing that makes a layered drawing readable: a chain that
    // wanders across the page is one long edge drawn as a staircase, and it costs nothing in
    // the crossing metric. Charge every hop for the horizontal distance it travels, so an
    // ordering that keeps a long edge in one column wins over one that merely crosses less.
    minimize_span(&mut layers, &nodes, &chain, &up, &down);
    // Sibling contiguity is a trade, not a free win: it keeps subtrees readable but can widen a
    // bus enough to add crossings elsewhere. Build both orderings and keep whichever costs less
    // under the combined crossing + span objective, so a two-owner model gets contiguous blocks
    // and a wide single-owner model keeps the crossing-optimal one.
    let unconstrained = layers.clone();
    let unconstrained_cost = order_cost(&unconstrained, &nodes, &chain, &down);

    enforce_blocks(&mut layers, &nodes, parent, preorder);
    // Re-optimise with the blocks in place: it can still swap two neighbours inside a block and
    // it may move a detached node, which is where most of the remaining crossings live.
    refine_within_blocks(&mut layers, &up, &down, &nodes, parent);
    full_refine(&mut layers, &up, &down, &nodes, parent, preorder);
    let blocked_cost = order_cost(&layers, &nodes, &chain, &down);

    if blocked_cost > unconstrained_cost {
        layers = unconstrained;
        // The unconstrained ordering is the better one; give it the same extra refinement so the
        // comparison is between two equally optimised orderings.
        full_refine(&mut layers, &up, &down, &nodes, parent, preorder);
    }

    // ---- 5. x coordinates -----------------------------------------------------------------
    let mut xs = assign_x(&layers, &nodes, &up, &down);
    pack_layers(&mut xs, &layers, &nodes);
    let min_x = xs.iter().copied().fold(f32::MAX, f32::min);
    for (id, nd) in nodes.iter_mut().enumerate() {
        nd.x = xs[id] - min_x;
    }

    // ---- 6. attachment slots --------------------------------------------------------------
    let mut at_side: HashMap<(usize, Side), Vec<Arrow>> = HashMap::new();
    for e in edges {
        if !pairs.contains(&(e.from, e.to)) {
            continue;
        }
        let low = if rank[e.from] < rank[e.to] {
            e.from
        } else {
            e.to
        };
        let high = if rank[e.from] < rank[e.to] {
            e.to
        } else {
            e.from
        };
        for (node, side) in [(low, Side::Bottom), (high, Side::Top)] {
            let list = at_side.entry((node, side)).or_default();
            if !list.contains(&e.kind) {
                list.push(e.kind);
            }
        }
    }
    let mut slots: HashMap<(usize, Side, Arrow), f32> = HashMap::new();
    for ((node, side), kinds) in &at_side {
        let mut kinds = kinds.clone();
        kinds.sort_by_key(|k| kind_order(*k));
        let m = kinds.len() as f32;
        for (i, kind) in kinds.iter().enumerate() {
            let offset = (i as f32 - (m - 1.0) / 2.0) * SLOT_STEP;
            slots.insert((*node, *side, *kind), nodes[*node].cx() + offset);
        }
    }

    // ---- 7. lanes -------------------------------------------------------------------------
    let mut groups: Vec<Group> = Vec::new();
    let mut key_index: HashMap<(usize, Arrow, usize), usize> = HashMap::new();
    let mut group_of: Vec<Vec<Option<usize>>> = vec![Vec::new(); pairs.len()];

    for (k, &(u, v)) in dag.iter().enumerate() {
        let kind = pair_kind[k];
        let last = hops_of[k].len().saturating_sub(1);
        for (h, hop) in hops_of[k].iter().enumerate() {
            let xu = if h == 0 {
                slot_x(&slots, &nodes, u, Side::Bottom, kind)
            } else {
                nodes[hop.upper].cx()
            };
            let xv = if h == last {
                slot_x(&slots, &nodes, v, Side::Top, kind)
            } else {
                nodes[hop.lower].cx()
            };
            group_of[k].push(None);
            if (xu - xv).abs() < 0.5 {
                continue;
            }
            // One lane per (fan-out node, relationship, gap): the ownership bus and the typing
            // trunk fall out of this key instead of being detected afterwards.
            let key = (hop.upper, kind, hop.gap);
            let gi = *key_index.entry(key).or_insert_with(|| {
                groups.push(Group {
                    gap: hop.gap,
                    lo: xu.min(xv),
                    hi: xu.max(xv),
                    lane: 0,
                    y: 0.0,
                });
                groups.len() - 1
            });
            let g = &mut groups[gi];
            g.lo = g.lo.min(xu.min(xv));
            g.hi = g.hi.max(xu.max(xv));
            group_of[k][h] = Some(gi);
        }
    }

    // Interval colouring per gap.
    let mut gap_lanes: HashMap<usize, usize> = HashMap::new();
    {
        let mut by_gap: HashMap<usize, Vec<usize>> = HashMap::new();
        for (gi, g) in groups.iter().enumerate() {
            by_gap.entry(g.gap).or_default().push(gi);
        }
        for (gap, mut ids) in by_gap {
            ids.sort_by(|&a, &b| {
                groups[a]
                    .lo
                    .partial_cmp(&groups[b].lo)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut lanes: Vec<Vec<(f32, f32)>> = Vec::new();
            for gi in ids {
                let (lo, hi) = (groups[gi].lo, groups[gi].hi);
                let mut placed = false;
                for (lane, used) in lanes.iter_mut().enumerate() {
                    if used.iter().all(|&(a, b)| hi + 14.0 < a || b + 14.0 < lo) {
                        used.push((lo, hi));
                        groups[gi].lane = lane;
                        placed = true;
                        break;
                    }
                }
                if !placed {
                    lanes.push(vec![(lo, hi)]);
                    groups[gi].lane = lanes.len() - 1;
                }
            }
            gap_lanes.insert(gap, lanes.len());
        }
    }

    // ---- 8. y coordinates -----------------------------------------------------------------
    let mut rank_h = vec![0.0f32; ranks];
    for nd in &nodes {
        if nd.real.is_some() {
            rank_h[nd.rank] = rank_h[nd.rank].max(nd.h);
        }
    }
    let mut rank_y = vec![0.0f32; ranks];
    let mut y = HEADER;
    for r in 0..ranks {
        rank_y[r] = y;
        let lanes = gap_lanes.get(&r).copied().unwrap_or(0);
        let gap = if r + 1 == ranks {
            0.0
        } else {
            (2.0 * LANE_PAD + lanes as f32 * LANE_STEP).max(MIN_GAP)
        };
        y += rank_h[r] + gap;
    }
    for nd in nodes.iter_mut() {
        nd.y = rank_y[nd.rank] + (rank_h[nd.rank] - nd.h) / 2.0;
    }
    // Lane 0 hugs the upper rank of its gap, so a short edge stays short.
    for g in groups.iter_mut() {
        g.y = rank_y[g.gap] + rank_h[g.gap] + LANE_PAD + g.lane as f32 * LANE_STEP;
    }

    // ---- 9. routes ------------------------------------------------------------------------
    let mut routes = Vec::new();
    for (k, &(u, v)) in dag.iter().enumerate() {
        let kind = pair_kind[k];
        let last = hops_of[k].len().saturating_sub(1);
        let mut points: Vec<(f32, f32)> = Vec::new();
        points.push((
            slot_x(&slots, &nodes, u, Side::Bottom, kind),
            nodes[u].bottom(),
        ));
        let mut widest: Option<(f32, f32, f32)> = None;
        for (h, hop) in hops_of[k].iter().enumerate() {
            let xu = if h == 0 {
                slot_x(&slots, &nodes, u, Side::Bottom, kind)
            } else {
                nodes[hop.upper].cx()
            };
            let xv = if h == last {
                slot_x(&slots, &nodes, v, Side::Top, kind)
            } else {
                nodes[hop.lower].cx()
            };
            if let Some(gi) = group_of[k].get(h).copied().flatten() {
                let g = &groups[gi];
                points.push((xu, g.y));
                points.push((xv, g.y));
                let span = (xv - xu).abs();
                if widest.is_none_or(|(s, _, _)| span > s) {
                    widest = Some((span, (xu + xv) / 2.0, g.y));
                }
            }
            points.push((xv, nodes[hop.lower].top()));
        }
        let points = simplify(points);
        if points.len() < 2 {
            continue;
        }

        // The symbol belongs to the node the model names, which is not always the deeper end:
        // with `part front : Wheel` and `Wheel` ranked above, the triangle sits on the usage's
        // bottom border and points up.
        let target_is_lower = !reversed[k];
        let head = match kind {
            Arrow::Composite => Some(Symbol {
                at_start: true,
                points_down: false,
            }),
            Arrow::Typing | Arrow::Satisfy | Arrow::Annotate => Some(Symbol {
                at_start: !target_is_lower,
                points_down: target_is_lower,
            }),
            // A transition is drawn by the state view, which places its own arrowhead; the
            // layered renderer never sees one.
            Arrow::Connector | Arrow::Transition => None,
        };

        let label = match widest {
            Some((span, mid, ly)) if span > 8.0 => Some((mid, ly - 5.0)),
            _ => {
                let (a, b) = (points[0], points[points.len() - 1]);
                Some(((a.0 + b.0) / 2.0 + 26.0, (a.1 + b.1) / 2.0))
            }
        };
        routes.push(Route {
            edge: pair_edge[k],
            points,
            head,
            label,
        });
    }

    let width = nodes
        .iter()
        .filter(|nd| nd.real.is_some())
        .map(|nd| nd.x + nd.w)
        .fold(0.0f32, f32::max);
    let height = rank_y[ranks - 1] + rank_h[ranks - 1];

    Layered {
        nodes,
        routes,
        width,
        height,
    }
}

fn slot_x(
    slots: &HashMap<(usize, Side, Arrow), f32>,
    nodes: &[LNode],
    node: usize,
    side: Side,
    kind: Arrow,
) -> f32 {
    slots
        .get(&(node, side, kind))
        .copied()
        .unwrap_or_else(|| nodes[node].cx())
}

/// Stable order for attachment slots: ownership symbol first, then typing, then satisfy.
fn kind_order(k: Arrow) -> u8 {
    match k {
        Arrow::Composite => 0,
        Arrow::Typing => 1,
        Arrow::Satisfy => 2,
        Arrow::Connector => 3,
        Arrow::Transition => 4,
        Arrow::Annotate => 5,
    }
}

fn adjacent(nodes: &[LNode], hops: &[Vec<Hop>]) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let mut up = vec![Vec::new(); nodes.len()];
    let mut down = vec![Vec::new(); nodes.len()];
    for list in hops {
        for hop in list {
            down[hop.upper].push(hop.lower);
            up[hop.lower].push(hop.upper);
        }
    }
    (up, down)
}

fn simplify(points: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(points.len());
    for p in points {
        if let Some(&last) = out.last() {
            if (last.0 - p.0).abs() < 0.4 && (last.1 - p.1).abs() < 0.4 {
                continue;
            }
        }
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let collinear = ((a.0 - b.0).abs() < 0.4 && (b.0 - p.0).abs() < 0.4)
                || ((a.1 - b.1).abs() < 0.4 && (b.1 - p.1).abs() < 0.4);
            if collinear {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// cycle breaking and ranking
// ---------------------------------------------------------------------------------------------

/// Eades–Lin–Smyth greedy feedback-arc set. Returns a linear order in which as few edges as
/// possible point backwards; those are the ones reversed for layout.
fn eades_order(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut out_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(u, v) in edges {
        if u != v {
            out_adj[u].push(v);
            in_adj[v].push(u);
        }
    }
    let mut out_deg: Vec<usize> = out_adj.iter().map(Vec::len).collect();
    let mut in_deg: Vec<usize> = in_adj.iter().map(Vec::len).collect();
    let mut removed = vec![false; n];
    let mut s1: Vec<usize> = Vec::with_capacity(n);
    let mut s2: Vec<usize> = Vec::with_capacity(n);

    let strip =
        |v: usize, removed: &mut Vec<bool>, out_deg: &mut Vec<usize>, in_deg: &mut Vec<usize>| {
            removed[v] = true;
            for i in 0..out_adj[v].len() {
                let w = out_adj[v][i];
                if !removed[w] {
                    in_deg[w] = in_deg[w].saturating_sub(1);
                }
            }
            for i in 0..in_adj[v].len() {
                let u = in_adj[v][i];
                if !removed[u] {
                    out_deg[u] = out_deg[u].saturating_sub(1);
                }
            }
        };

    while s1.len() + s2.len() < n {
        let mut progressed = true;
        while progressed {
            progressed = false;
            for v in 0..n {
                if !removed[v] && out_deg[v] == 0 {
                    strip(v, &mut removed, &mut out_deg, &mut in_deg);
                    s2.push(v);
                    progressed = true;
                }
            }
            for v in 0..n {
                if !removed[v] && in_deg[v] == 0 {
                    strip(v, &mut removed, &mut out_deg, &mut in_deg);
                    s1.push(v);
                    progressed = true;
                }
            }
        }
        if s1.len() + s2.len() >= n {
            break;
        }
        // The break above guarantees at least one node is left, so the maximum exists. The
        // `let Some` keeps that guarantee visible here instead of an unwrap that would panic if
        // the loop invariant ever changed.
        let Some(pick) = (0..n)
            .filter(|&v| !removed[v])
            .max_by_key(|&v| (out_deg[v] as isize - in_deg[v] as isize, usize::MAX - v))
        else {
            break;
        };
        strip(pick, &mut removed, &mut out_deg, &mut in_deg);
        s1.push(pick);
    }

    s2.reverse();
    s1.extend(s2);
    s1
}

/// Longest-path ranks, then a bounded median descent towards the neighbours' ranks.
fn rank_nodes(n: usize, dag: &[(usize, usize)], sequence: &[usize]) -> Vec<usize> {
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(u, v) in dag {
        succ[u].push(v);
        pred[v].push(u);
    }

    let mut earliest = vec![0usize; n];
    for &v in sequence {
        for &u in &pred[v] {
            earliest[v] = earliest[v].max(earliest[u] + 1);
        }
    }
    let max_rank = *earliest.iter().max().unwrap_or(&0);

    let mut latest = vec![max_rank; n];
    for &v in sequence.iter().rev() {
        for &w in &succ[v] {
            latest[v] = latest[v].min(latest[w].saturating_sub(1));
        }
    }

    let mut cur = earliest.clone();
    for _ in 0..12 {
        for &v in sequence {
            let mut candidates: Vec<usize> = Vec::new();
            candidates.extend(pred[v].iter().map(|&u| cur[u] + 1));
            candidates.extend(succ[v].iter().map(|&w| cur[w].saturating_sub(1)));
            if candidates.is_empty() {
                continue;
            }
            candidates.sort_unstable();
            let m = candidates.len() / 2;
            let target = if candidates.len() % 2 == 1 {
                candidates[m]
            } else {
                (candidates[m - 1] + candidates[m]) / 2
            };
            cur[v] = target.clamp(earliest[v], latest[v]);
        }
    }
    // The descent works on integer medians and can leave an edge flat; repair forward.
    for &v in sequence {
        for &u in &pred[v] {
            if cur[v] <= cur[u] {
                cur[v] = cur[u] + 1;
            }
        }
    }
    cur
}

// ---------------------------------------------------------------------------------------------
// ordering
// ---------------------------------------------------------------------------------------------

/// Real nodes keep declaration order; virtual nodes are inserted by the median position of
/// their upper neighbour, top-down, so the optimiser starts from something sane.
fn initial_order(
    layers: &mut [Vec<usize>],
    up: &[Vec<usize>],
    preorder: &[usize],
    nodes: &[LNode],
) {
    // The seed position of a node inside its own rank. Real nodes take their pre-order number,
    // so siblings stay together and a subtree reads left to right in declaration order. A
    // virtual node has no declaration, so it takes the seed of its upper neighbour: putting a
    // chain directly under the edge it continues is what keeps it from drifting to the page
    // margin, which is where a "slot index" fallback used to send it.
    let seed = |v: usize| -> f32 {
        match nodes[v].real {
            Some(real) => preorder.get(real).copied().unwrap_or(real) as f32,
            None => f32::NAN,
        }
    };

    // Rank 0 has no upper neighbours, so its seed is all it has.
    let mut first: Vec<(f32, usize)> = layers[0].iter().map(|&v| (seed(v), v)).collect();
    first.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    layers[0] = first.into_iter().map(|(_, v)| v).collect();

    for r in 1..layers.len() {
        let pos: HashMap<usize, usize> = layers[r - 1]
            .iter()
            .enumerate()
            .map(|(i, &v)| (v, i))
            .collect();
        let mut keyed: Vec<(f32, usize, usize)> = layers[r]
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let mut meds: Vec<usize> =
                    up[v].iter().filter_map(|u| pos.get(u).copied()).collect();
                meds.sort_unstable();
                let own = seed(v);
                let key = if meds.is_empty() {
                    // No upper neighbour: fall back to the declaration seed for a real node and
                    // keep the current slot for a virtual one.
                    if own.is_nan() {
                        i as f32
                    } else {
                        own
                    }
                } else if meds.len() % 2 == 1 {
                    meds[meds.len() / 2] as f32
                } else {
                    (meds[meds.len() / 2 - 1] + meds[meds.len() / 2]) as f32 / 2.0
                };
                (key, i, v)
            })
            .collect();
        keyed.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        });
        layers[r] = keyed.into_iter().map(|(_, _, v)| v).collect();
    }
}

fn minimize_crossings(
    layers: &mut [Vec<usize>],
    up: &[Vec<usize>],
    down: &[Vec<usize>],
    rounds: usize,
) {
    let mut best = layers.to_vec();
    let mut best_cross = total_crossings(layers, down);
    for round in 0..rounds {
        let sweep_down = round % 2 == 0;
        let range: Vec<usize> = if sweep_down {
            (1..layers.len()).collect()
        } else {
            (0..layers.len().saturating_sub(1)).rev().collect()
        };
        for r in range {
            let other = if sweep_down { r - 1 } else { r + 1 };
            let pos: HashMap<usize, usize> = layers[other]
                .iter()
                .enumerate()
                .map(|(i, &v)| (v, i))
                .collect();
            let current: Vec<usize> = layers[r].clone();
            let mut keyed: Vec<(f32, usize)> = current
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let nbrs = if sweep_down { &up[v] } else { &down[v] };
                    let mut meds: Vec<usize> =
                        nbrs.iter().filter_map(|u| pos.get(u).copied()).collect();
                    meds.sort_unstable();
                    let key = if meds.is_empty() {
                        i as f32
                    } else if meds.len() % 2 == 1 {
                        meds[meds.len() / 2] as f32
                    } else {
                        (meds[meds.len() / 2 - 1] + meds[meds.len() / 2]) as f32 / 2.0
                    };
                    (key, v)
                })
                .collect();
            keyed.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1.cmp(&b.1))
            });
            layers[r] = keyed.into_iter().map(|(_, v)| v).collect();
        }
        transpose(layers, up, down);
        let cross = total_crossings(layers, down);
        if cross < best_cross {
            best_cross = cross;
            best = layers.to_vec();
        }
    }
    for (r, layer) in layers.iter_mut().enumerate() {
        *layer = best[r].clone();
    }
}

/// Cost of the current ordering: crossings plus the horizontal spread of every hop.
fn order_cost(
    layers: &[Vec<usize>],
    nodes: &[LNode],
    chain: &[Vec<usize>],
    down: &[Vec<usize>],
) -> f32 {
    let pos = positions(layers);
    let mut cost = total_crossings(layers, down) as f32 * 4.0;
    for path in chain {
        for w in path.windows(2) {
            let (a, b) = (w[0], w[1]);
            let d = (pos[a] as f32 - pos[b] as f32).abs();
            cost += d;
            // A hop that also moves between ranks is one leg of a long edge; weight the ones
            // that skip a rank more heavily, because those are the runs that read as detours.
            if nodes[a].rank + 1 < nodes[b].rank {
                cost += d;
            }
        }
    }
    cost
}

/// Moves a chain's virtual nodes together inside their ranks when that lowers the total cost.
///
/// The median sweeps optimise crossings and have no reason to keep a long edge vertical: moving
/// a virtual node one slot right can remove a crossing while turning the edge into a staircase.
/// This pass looks at each chain and tries pulling its virtual nodes to the position of their
/// neighbours in the same rank, accepting only moves that lower `order_cost`.
fn minimize_span(
    layers: &mut Vec<Vec<usize>>,
    nodes: &[LNode],
    chain: &[Vec<usize>],
    up: &[Vec<usize>],
    down: &[Vec<usize>],
) {
    let mut best = order_cost(layers, nodes, chain, down);
    for _ in 0..3 {
        let mut improved = false;
        for path in chain {
            for idx in 1..path.len().saturating_sub(1) {
                let v = path[idx];
                if !nodes[v].is_virtual() {
                    continue;
                }
                let r = nodes[v].rank;
                let Some(slot) = layers[r].iter().position(|&x| x == v) else {
                    continue;
                };
                // Where the chain wants this node: the average of its two neighbours' slots.
                let mut targets: Vec<usize> = Vec::new();
                for &nbr in &[path[idx - 1], path[idx + 1]] {
                    if let Some(&p) = positions(layers).get(nbr) {
                        let _ = p;
                    }
                    if let Some(pos) = layers
                        .iter()
                        .find_map(|layer| layer.iter().position(|&x| x == nbr))
                    {
                        targets.push(pos);
                    }
                }
                if targets.is_empty() {
                    continue;
                }
                let want = targets.iter().sum::<usize>() / targets.len();
                if want == slot {
                    continue;
                }
                let mut trial = layers.to_vec();
                trial[r].remove(slot);
                let at = want.min(trial[r].len());
                trial[r].insert(at, v);
                let cost = order_cost(&trial, nodes, chain, down);
                if cost < best {
                    best = cost;
                    *layers = trial;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
    let _ = (up, down);
}

fn total_crossings(layers: &[Vec<usize>], down: &[Vec<usize>]) -> usize {
    let mut total = 0;
    for r in 0..layers.len().saturating_sub(1) {
        let pos_up: HashMap<usize, usize> =
            layers[r].iter().enumerate().map(|(i, &v)| (v, i)).collect();
        let pos_lo: HashMap<usize, usize> = layers[r + 1]
            .iter()
            .enumerate()
            .map(|(i, &v)| (v, i))
            .collect();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for &u in &layers[r] {
            for &v in &down[u] {
                if let (Some(&a), Some(&b)) = (pos_up.get(&u), pos_lo.get(&v)) {
                    pairs.push((a, b));
                }
            }
        }
        // Inversions on the lower positions, counted with a Fenwick tree.
        pairs.sort_unstable();
        let m = layers[r + 1].len();
        let mut tree = vec![0usize; m + 1];
        for &(_, b) in &pairs {
            let mut i = b + 1;
            while i <= m {
                tree[i] += 1;
                i += i & i.wrapping_neg();
            }
            let mut j = b;
            while j > 0 {
                total += tree[j];
                j -= j & j.wrapping_neg();
            }
        }
    }
    total
}

/// Crossings contributed by having `a` immediately left of `b` in their shared layer.
fn pair_crossings(
    a: usize,
    b: usize,
    pos_up: &HashMap<usize, usize>,
    pos_lo: &HashMap<usize, usize>,
    up: &[Vec<usize>],
    down: &[Vec<usize>],
) -> usize {
    let mut c = 0;
    for &u in &up[a] {
        for &w in &up[b] {
            if let (Some(&p), Some(&q)) = (pos_up.get(&u), pos_up.get(&w)) {
                if p > q {
                    c += 1;
                }
            }
        }
    }
    for &u in &down[a] {
        for &w in &down[b] {
            if let (Some(&p), Some(&q)) = (pos_lo.get(&u), pos_lo.get(&w)) {
                if p > q {
                    c += 1;
                }
            }
        }
    }
    c
}

/// Adjacent transposition: swap two neighbours whenever that removes crossings.
fn transpose(layers: &mut [Vec<usize>], up: &[Vec<usize>], down: &[Vec<usize>]) {
    let mut improved = true;
    let mut guard = 0;
    while improved && guard < 6 {
        improved = false;
        guard += 1;
        for r in 0..layers.len() {
            let prev_pos: HashMap<usize, usize> = if r > 0 {
                layers[r - 1]
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (v, i))
                    .collect()
            } else {
                HashMap::new()
            };
            let next_pos: HashMap<usize, usize> = if r + 1 < layers.len() {
                layers[r + 1]
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (v, i))
                    .collect()
            } else {
                HashMap::new()
            };
            for i in 0..layers[r].len().saturating_sub(1) {
                let a = layers[r][i];
                let b = layers[r][i + 1];
                let before = pair_crossings(a, b, &prev_pos, &next_pos, up, down);
                if before == 0 {
                    continue;
                }
                let after = pair_crossings(b, a, &prev_pos, &next_pos, up, down);
                if after < before {
                    layers[r].swap(i, i + 1);
                    improved = true;
                }
            }
        }
    }
}

/// Keeps each subtree contiguous in every rank.
///
/// Sibling contiguity has to be enforced for the *whole subtree*, not just for direct children.
/// Two usages of one definition each own a set of parts, and those parts sit two ranks further
/// down; if the ordering interleaves them there, the two ownership buses interleave too, and the
/// drawing shows a lattice instead of two trees. Sorting each rank by the top-level ancestor a
/// node descends from, then by its position within that subtree, makes every subtree a
/// contiguous block from top to bottom.
fn enforce_blocks(
    layers: &mut [Vec<usize>],
    nodes: &[LNode],
    parent: &[Option<usize>],
    preorder: &[usize],
) {
    let n = parent.len();
    // The top-level ancestor of every real node, by walking the projected ownership tree.
    let mut root_of = vec![usize::MAX; n];
    for i in 0..n {
        let mut cur = i;
        let mut guard = 0;
        while let Some(p) = parent[cur] {
            cur = p;
            guard += 1;
            if guard > n {
                break;
            }
        }
        root_of[i] = cur;
    }

    for layer in layers.iter_mut() {
        // A node that is its own root owns no subtree — a definition, a requirement, or any
        // element the projection left detached. Those must keep the position the crossing
        // optimiser gave them: they are the *targets* of references, so their neighbourhood is
        // what decides where they read best. Re-sorting them by declaration order (which is what
        // a naive pre-order key does) shoves every definition to one side and lengthens every
        // typing edge that reaches it; on the official parts-tree model that alone was worth
        // most of the long runs across the page.
        let key_of = |root: usize, node: usize| -> Option<(usize, usize)> {
            if root == node {
                return None;
            }
            Some((
                preorder.get(root).copied().unwrap_or(root),
                preorder.get(node).copied().unwrap_or(node),
            ))
        };
        let mut keyed: Vec<((usize, usize), usize, usize)> = layer
            .iter()
            .enumerate()
            .map(|(slot, &v)| match nodes[v].real {
                Some(real) => match key_of(root_of[real], real) {
                    // Inside a subtree: block by root, then declaration order.
                    Some(key) => (key, slot, v),
                    // Detached: keep the optimiser's slot as both keys.
                    None => ((slot, 0), slot, v),
                },
                // A virtual node follows the block it was dropped into.
                None => ((usize::MAX, usize::MAX), slot, v),
            })
            .collect();
        keyed.sort_by_key(|&(key, slot, _)| (key, slot));
        *layer = keyed.into_iter().map(|(_, _, v)| v).collect();
    }
}

/// Crossing reduction that may also move a node with no ownership block.
///
/// Detached nodes — definitions, requirements, anything the projection did not hang off an owner
/// in that rank — are exactly the ones the reference model points *at*, so their position decides
/// most of the crossings. They are free to move anywhere; only the nodes inside a real ownership
/// subtree are pinned to their block.
fn full_refine(
    layers: &mut [Vec<usize>],
    up: &[Vec<usize>],
    down: &[Vec<usize>],
    nodes: &[LNode],
    parent: &[Option<usize>],
    preorder: &[usize],
) {
    let detached = |v: usize| -> bool {
        match nodes[v].real {
            Some(real) => parent[real].is_none(),
            None => false,
        }
    };
    let family = |v: usize| -> Option<usize> {
        let real = nodes[v].real?;
        Some(parent[real].unwrap_or(real))
    };
    let _ = preorder;

    let mut improved = true;
    let mut guard = 0;
    while improved && guard < 8 {
        improved = false;
        guard += 1;
        for r in 0..layers.len() {
            let prev_pos: HashMap<usize, usize> = if r > 0 {
                layers[r - 1]
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (v, i))
                    .collect()
            } else {
                HashMap::new()
            };
            let next_pos: HashMap<usize, usize> = if r + 1 < layers.len() {
                layers[r + 1]
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (v, i))
                    .collect()
            } else {
                HashMap::new()
            };
            for i in 0..layers[r].len().saturating_sub(1) {
                let (a, b) = (layers[r][i], layers[r][i + 1]);
                // Swapping is allowed when neither node is pinned to a different block.
                let pinned_conflict = match (family(a), family(b)) {
                    (Some(fa), Some(fb)) => fa != fb,
                    _ => false,
                };
                if pinned_conflict && !(detached(a) || detached(b)) {
                    continue;
                }
                // A detached node may move past a block; a pinned node may not.
                if pinned_conflict {
                    // Only allow if the detached one is on the outside of the block it crosses.
                    if detached(a) && !detached(b) {
                        let block_lo = layers[r]
                            .iter()
                            .position(|&v| family(v) == family(b))
                            .unwrap_or(i + 1);
                        if i < block_lo {
                            continue;
                        }
                    } else if detached(b) && !detached(a) {
                        let block_hi = layers[r]
                            .iter()
                            .rposition(|&v| family(v) == family(a))
                            .unwrap_or(i);
                        if i + 1 > block_hi {
                            continue;
                        }
                    }
                }
                let before = pair_crossings(a, b, &prev_pos, &next_pos, up, down);
                if before == 0 {
                    continue;
                }
                let after = pair_crossings(b, a, &prev_pos, &next_pos, up, down);
                if after < before {
                    layers[r].swap(i, i + 1);
                    improved = true;
                }
            }
        }
    }
}

/// Crossing reduction that only swaps neighbours belonging to the same family.
fn refine_within_blocks(
    layers: &mut [Vec<usize>],
    up: &[Vec<usize>],
    down: &[Vec<usize>],
    nodes: &[LNode],
    parent: &[Option<usize>],
) {
    let family = |v: usize| -> Option<usize> {
        let real = nodes[v].real?;
        Some(parent[real].unwrap_or(real))
    };
    let mut guard = 0;
    let mut improved = true;
    while improved && guard < 4 {
        improved = false;
        guard += 1;
        for r in 0..layers.len() {
            let prev_pos: HashMap<usize, usize> = if r > 0 {
                layers[r - 1]
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (v, i))
                    .collect()
            } else {
                HashMap::new()
            };
            let next_pos: HashMap<usize, usize> = if r + 1 < layers.len() {
                layers[r + 1]
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (v, i))
                    .collect()
            } else {
                HashMap::new()
            };
            for i in 0..layers[r].len().saturating_sub(1) {
                let (a, b) = (layers[r][i], layers[r][i + 1]);
                if family(a).is_none() || family(a) != family(b) {
                    continue;
                }
                let before = pair_crossings(a, b, &prev_pos, &next_pos, up, down);
                if before == 0 {
                    continue;
                }
                let after = pair_crossings(b, a, &prev_pos, &next_pos, up, down);
                if after < before {
                    layers[r].swap(i, i + 1);
                    improved = true;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// x coordinates — Brandes & Köpf (2001)
// ---------------------------------------------------------------------------------------------

fn assign_x(
    layers: &[Vec<usize>],
    nodes: &[LNode],
    up: &[Vec<usize>],
    down: &[Vec<usize>],
) -> Vec<f32> {
    let n = nodes.len();
    let gap = |a: usize, b: usize| -> f32 {
        if nodes[a].is_virtual() && nodes[b].is_virtual() {
            SEP_V
        } else {
            SEP
        }
    };

    let mut results: Vec<Vec<f32>> = Vec::new();
    for flip_v in 0..2 {
        for flip_h in 0..2 {
            let mut lay: Vec<Vec<usize>> = layers.to_vec();
            if flip_v == 1 {
                lay.reverse();
            }
            if flip_h == 1 {
                for layer in lay.iter_mut() {
                    layer.reverse();
                }
            }
            let adj: Vec<Vec<usize>> = if flip_v == 0 {
                up.to_vec()
            } else {
                down.to_vec()
            };
            results.push(bk_pass(&lay, nodes, &adj, n, &gap));
        }
    }

    for xs in results.iter_mut() {
        let lo = xs.iter().copied().fold(f32::MAX, f32::min);
        for x in xs.iter_mut() {
            *x -= lo;
        }
    }
    let mut out = vec![0.0f32; n];
    for v in 0..n {
        let mut vals: Vec<f32> = results.iter().map(|xs| xs[v]).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        out[v] = (vals[1] + vals[2]) / 2.0;
    }
    out
}

/// One Brandes–Köpf pass: vertical alignment into blocks, then horizontal compaction with the
/// sink/shift trick, so blocks that cannot reach each other record a relative offset instead of
/// being pushed apart.
fn bk_pass(
    layers: &[Vec<usize>],
    nodes: &[LNode],
    adj: &[Vec<usize>],
    n: usize,
    gap: &dyn Fn(usize, usize) -> f32,
) -> Vec<f32> {
    let mut root: Vec<usize> = (0..n).collect();
    let mut align: Vec<usize> = (0..n).collect();
    let pos = positions(layers);

    for r in 1..layers.len() {
        let mut last: isize = -1;
        for &v in &layers[r] {
            if adj[v].is_empty() {
                continue;
            }
            let mut ws: Vec<usize> = adj[v].clone();
            ws.sort_by_key(|&w| pos[w]);
            let d = ws.len();
            for m in [(d - 1) / 2, d / 2] {
                if align[v] != v {
                    break;
                }
                let u = ws[m];
                if pos[u] as isize > last {
                    align[u] = v;
                    root[v] = root[u];
                    align[v] = root[v];
                    last = pos[u] as isize;
                }
            }
        }
    }

    let mut xs: Vec<Option<f32>> = vec![None; n];
    let mut sink: Vec<usize> = (0..n).collect();
    let mut shift: Vec<f32> = vec![f32::INFINITY; n];
    let mut pred: Vec<Option<usize>> = vec![None; n];
    for layer in layers {
        for i in 1..layer.len() {
            pred[layer[i]] = Some(layer[i - 1]);
        }
    }

    fn place(
        v: usize,
        xs: &mut [Option<f32>],
        sink: &mut [usize],
        shift: &mut [f32],
        align: &[usize],
        root: &[usize],
        pred: &[Option<usize>],
        nodes: &[LNode],
        gap: &dyn Fn(usize, usize) -> f32,
    ) {
        if xs[v].is_some() {
            return;
        }
        xs[v] = Some(0.0);
        let mut w = v;
        let mut guard = 0usize;
        loop {
            guard += 1;
            if guard > 100_000 {
                break;
            }
            if let Some(p) = pred[w] {
                let u = root[p];
                place(u, xs, sink, shift, align, root, pred, nodes, gap);
                if sink[v] == v {
                    sink[v] = sink[u];
                }
                let sep = nodes[p].w / 2.0 + nodes[w].w / 2.0 + gap(p, w);
                let xu = xs[u].unwrap_or(0.0);
                let xv = xs[v].unwrap_or(0.0);
                if sink[v] != sink[u] {
                    let target = xv - xu - sep;
                    if target < shift[sink[u]] {
                        shift[sink[u]] = target;
                    }
                } else if xu + sep > xv {
                    xs[v] = Some(xu + sep);
                }
            }
            w = align[w];
            if w == v {
                break;
            }
        }
    }

    for layer in layers {
        for &v in layer {
            if root[v] == v {
                place(
                    v, &mut xs, &mut sink, &mut shift, &align, &root, &pred, nodes, gap,
                );
            }
        }
    }

    let mut out = vec![0.0f32; n];
    for v in 0..n {
        let base = xs[root[v]].unwrap_or(0.0);
        let s = shift[sink[root[v]]];
        out[v] = base + if s.is_finite() { s } else { 0.0 };
    }
    out
}

fn positions(layers: &[Vec<usize>]) -> Vec<usize> {
    let mut pos = vec![0usize; layers.iter().map(Vec::len).sum()];
    for layer in layers {
        for (i, &v) in layer.iter().enumerate() {
            pos[v] = i;
        }
    }
    pos
}

/// Enforce minimum separation inside every rank. Brandes–Köpf guarantees it per alignment, but
/// the per-node median of four need not, so this is the safety net that keeps the result legal.
fn pack_layers(xs: &mut [f32], layers: &[Vec<usize>], nodes: &[LNode]) {
    for layer in layers {
        for i in 1..layer.len() {
            let (a, b) = (layer[i - 1], layer[i]);
            let gap = if nodes[a].is_virtual() && nodes[b].is_virtual() {
                SEP_V
            } else {
                SEP
            };
            let need = xs[a] + nodes[a].w + gap;
            if xs[b] < need {
                xs[b] = need;
            }
        }
    }
}

/// The tree style: every node on its own row, indented by depth, with one line per edge.
///
/// The layered layout draws ownership as nesting — a child sits inside its owner's rectangle —
/// which is what the reference does. The tree style keeps the same graph and puts every node in
/// one column at the depth its ownership gives it, so a model with six levels of nesting stays
/// readable. Anything that is not an ownership edge (typing, satisfy, connector) is drawn as an
/// arc from the row of its source to the row of its target.
pub fn tree(sizes: &[(f32, f32)], edges: &[Input], parent: &[Option<usize>]) -> Layered {
    let n = sizes.len();
    if n == 0 {
        return Layered {
            nodes: Vec::new(),
            routes: Vec::new(),
            width: 0.0,
            height: 0.0,
        };
    }
    let depth = |mut node: usize| -> usize {
        let mut d = 0usize;
        while let Some(p) = parent.get(node).copied().flatten() {
            d += 1;
            if d > n {
                break;
            }
            node = p;
        }
        d
    };
    let indent = SEP * 2.0;
    let mut y = HEADER;
    let mut nodes = Vec::with_capacity(n);
    let mut row = vec![0usize; n];
    // Pre-order is the order the tree was built in, so a subtree stays contiguous.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&node| (depth(node), node));
    for (position, &node) in order.iter().enumerate() {
        row[node] = position;
        let (w, h) = sizes[node];
        nodes.push(LNode {
            real: Some(node),
            w,
            h,
            rank: position,
            x: indent * depth(node) as f32,
            y,
        });
        y += h + SEP_V;
    }
    let mut width: f32 = nodes.iter().map(|n| n.x + n.w).fold(0.0, f32::max);
    width += SEP;
    let height = y;
    let mut routes = Vec::new();
    for (index, edge) in edges.iter().enumerate() {
        let (from, to) = (edge.from, edge.to);
        if from >= n || to >= n {
            continue;
        }
        // A line from the middle of the source's bottom edge to the middle of the target's top
        // edge, bent at the target's indent so the row it belongs to is visible.
        let a = &nodes[row[from]];
        let b = &nodes[row[to]];
        let start = (a.x + a.w / 2.0, a.y + a.h);
        let end = (b.x + b.w / 2.0, b.y);
        let mid_y = (start.1 + end.1) / 2.0;
        routes.push(Route {
            edge: index,
            points: vec![start, (start.0, mid_y), (end.0, mid_y), end],
            head: None,
            label: None,
        });
    }
    Layered {
        nodes,
        routes,
        width,
        height,
    }
}
