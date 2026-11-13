//! Line jumps at orthogonal crossings.
//!
//! Two relationships that cross are ambiguous unless the picture says which strand is continuous.
//! Under an orthogonal convention the pair is always an orthogonal pair, so the usual reading —
//! the horizontal wire hops, the vertical one passes underneath — is enough. A small arc at the
//! crossing makes the two read as separate lines instead of a junction.
//!
//! This lives beside the geometry rather than in either renderer because both the SVG export and
//! the editable canvas draw the same picture. Two implementations would put the hops in different
//! places, and the difference would show up as "the export and the canvas disagree", which is the
//! class of bug the shared geometry exists to prevent.
//!
//! # Shared bus runs
//!
//! Fan-out edges leave one point and travel the same horizontal lane — the ownership bus a
//! composite draws. Every member of that bus therefore meets the same vertical run at the same
//! point, and giving each of them a hop stacks N identical arcs on one crossing. Only one strand
//! of a collinear run carries the arcs, chosen by edge index so the choice does not depend on the
//! order the caller happened to build its list in. Without this the drone's own example draws
//! three arcs where the picture has one crossing.

/// Radius of the arc, in content units.
///
/// Small enough to read as a crossing rather than a detour, large enough to survive a 1px stroke
/// at a zoomed-out view.
pub const RADIUS: f32 = 4.0;

/// Above this many lines, crossings are drawn plainly instead of arced.
///
/// A hop answers "which strand is continuous here?" for a reader following one line. That question
/// stops being answerable long before the arcs stop being emitted: a twelve-thousand-element model
/// puts about four hundred crossings on a single edge — four million arcs, two hundred megabytes of
/// path data, and five of the render's six seconds spent formatting it. At that density the arcs are
/// the picture rather than a reading aid, so they are dropped and the strands cross plainly.
///
/// Two thousand lines is well past any diagram a person reads on one page and well under the point
/// where the arcs dominate the cost.
const HOP_LINE_LIMIT: usize = 2_000;

/// One axis-aligned run, tagged with the polyline that owns it.
#[derive(Clone, Copy)]
struct Run {
    line: usize,
    /// The coordinate held constant: `y` for a horizontal run, `x` for a vertical one.
    at: f32,
    /// The interval covered along the other axis.
    from: f32,
    to: f32,
    horizontal: bool,
}

fn runs(polylines: &[Vec<(f32, f32)>]) -> Vec<Run> {
    let mut out = Vec::new();
    for (line, points) in polylines.iter().enumerate() {
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let horizontal = (a.1 - b.1).abs() < 0.01;
            let vertical = (a.0 - b.0).abs() < 0.01;
            if horizontal && (a.0 - b.0).abs() >= 0.01 {
                out.push(Run {
                    line,
                    at: a.1,
                    from: a.0.min(b.0),
                    to: a.0.max(b.0),
                    horizontal: true,
                });
            } else if vertical && (a.1 - b.1).abs() >= 0.01 {
                out.push(Run {
                    line,
                    at: a.0,
                    from: a.1.min(b.1),
                    to: a.1.max(b.1),
                    horizontal: false,
                });
            }
        }
    }
    out
}

/// The points along each polyline where it must hop over another.
///
/// Returns one entry per input polyline, in the order they were given. A crossing is recorded on
/// one horizontal strand only: the vertical one keeps its line, which is what makes exactly one
/// curve at any crossing.
pub fn hops(polylines: &[Vec<(f32, f32)>]) -> Vec<Vec<(f32, f32)>> {
    let mut out: Vec<Vec<(f32, f32)>> = vec![Vec::new(); polylines.len()];
    if polylines.len() > HOP_LINE_LIMIT {
        return out;
    }
    let all = runs(polylines);

    // Arc placement needs room at both ends, or the arc starts on a bend and reads as a rounded
    // corner. Beyond that, two crossings closer together than one arc's diameter cannot both be
    // drawn: the second would start before the first ended.
    let margin = RADIUS + 1.0;
    let spacing = RADIUS * 2.0 + 1.0;

    // The strand that carries the arcs for a shared lane. Grouping by the lane's own coordinate
    // and taking the lowest line index keeps the choice stable and gives each crossing one arc.
    let mut carriers: std::collections::BTreeMap<u64, usize> = Default::default();
    for run in all.iter().filter(|r| r.horizontal) {
        let key = (run.at * 1000.0).round() as i64 as u64;
        let entry = carriers.entry(key).or_insert(run.line);
        if run.line < *entry {
            *entry = run.line;
        }
    }

    // Only a vertical whose `x` falls inside a horizontal's span can cross it, so the verticals go
    // into one array sorted by that coordinate and each horizontal asks for its own window. Every
    // horizontal against every vertical was the work that made a twelve-thousand-element render
    // spend six seconds here; the window makes it a binary search plus the crossings that actually
    // exist, which is what the picture contains.
    let mut verticals: Vec<Run> = all.iter().copied().filter(|r| !r.horizontal).collect();
    verticals.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));

    for h in all.iter().filter(|r| r.horizontal) {
        let key = (h.at * 1000.0).round() as i64 as u64;
        let carrier = carriers.get(&key).copied().unwrap_or(h.line);
        if h.line != carrier {
            continue;
        }
        let lo = verticals.partition_point(|v| v.at <= h.from + margin);
        let hi = verticals.partition_point(|v| v.at < h.to - margin);
        // A run shorter than the two margins together leaves an empty window, and `lo` then sits
        // past `hi`: slicing panics, which is how a stub of a horizontal segment inside a frame took
        // the whole studio down. No vertical can fall strictly inside nothing.
        if lo >= hi {
            continue;
        }
        for v in &verticals[lo..hi] {
            if h.line == v.line {
                continue;
            }
            if h.at <= v.from + margin || h.at >= v.to - margin {
                continue;
            }
            let at = (v.at, h.at);
            if out[h.line]
                .iter()
                .any(|p| (p.0 - at.0).abs() < spacing && (p.1 - at.1).abs() < 0.01)
            {
                continue;
            }
            out[h.line].push(at);
        }
    }
    for list in &mut out {
        list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }
    out
}

/// Path data for one polyline, arcing over each jump.
///
/// The arc bulges away from the direction of travel, which is the circuit convention: a
/// left-to-right run rises above the line.
pub fn path(points: &[(f32, f32)], jumps: &[(f32, f32)], radius: f32) -> String {
    let Some(first) = points.first() else {
        return String::new();
    };
    let mut d = format!("M{:.1} {:.1}", first.0, first.1);
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let horizontal = (a.1 - b.1).abs() < 0.01;
        let mut here: Vec<f32> = jumps
            .iter()
            .filter(|(x, y)| {
                if horizontal {
                    (a.1 - y).abs() < 0.01
                        && *x > a.0.min(b.0) + radius
                        && *x < a.0.max(b.0) - radius
                } else {
                    (a.0 - x).abs() < 0.01
                        && *y > a.1.min(b.1) + radius
                        && *y < a.1.max(b.1) - radius
                }
            })
            .map(|(x, y)| if horizontal { *x } else { *y })
            .collect();
        if !horizontal {
            // Nothing hops on a vertical run; the horizontal strand carries the arc.
            d.push_str(&format!(" L{:.1} {:.1}", b.0, b.1));
            continue;
        }
        here.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));
        let forward = b.0 > a.0;
        for at in here {
            let start = if forward { at - radius } else { at + radius };
            let end = if forward { at + radius } else { at - radius };
            d.push_str(&format!(" L{:.1} {:.1}", start, a.1));
            // sweep=1 draws the arc on the side that reads as "over" for a left-to-right run;
            // the reverse direction needs the other flag or the hop bulges into the line.
            let sweep = if forward { 1 } else { 0 };
            d.push_str(&format!(
                " A{radius:.1} {radius:.1} 0 0 {sweep} {:.1} {:.1}",
                end, a.1
            ));
        }
        d.push_str(&format!(" L{:.1} {:.1}", b.0, b.1));
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_horizontal_run_hops_where_a_vertical_one_crosses_it() {
        let lines = vec![
            vec![(0.0, 50.0), (100.0, 50.0)],
            vec![(50.0, 0.0), (50.0, 100.0)],
        ];
        let jumps = hops(&lines);
        assert_eq!(jumps[0], vec![(50.0, 50.0)]);
        assert!(jumps[1].is_empty(), "the vertical strand stays straight");
    }

    #[test]
    fn a_run_shorter_than_the_two_margins_is_not_a_crossing() {
        // A stub of a horizontal segment, ten units wide, with a vertical through the middle: there
        // is no room for an arc between the margins. The window is empty, and taking a slice of it
        // panicked — the studio died on a model with one short run inside a frame.
        let lines = vec![
            vec![(0.0, 0.0), (10.0, 0.0)],
            vec![(5.0, -50.0), (5.0, 50.0)],
        ];
        let jumps = hops(&lines);
        assert!(jumps[0].is_empty(), "no room for a hop");
        assert!(jumps[1].is_empty());
    }

    #[test]
    fn a_crossing_too_close_to_a_bend_is_not_hopped() {
        // The arc would start at the corner and read as a rounded corner instead of a crossing.
        let lines = vec![
            vec![(0.0, 50.0), (100.0, 50.0)],
            vec![(2.0, 0.0), (2.0, 100.0)],
        ];
        assert!(hops(&lines)[0].is_empty());
    }

    #[test]
    fn a_line_does_not_hop_itself() {
        // A polyline that doubles back crosses its own earlier run; that is a routing artefact to
        // be seen, not decorated.
        let lines = vec![vec![
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (50.0, 100.0),
            (50.0, 50.0),
        ]];
        assert!(hops(&lines)[0].is_empty());
    }

    /// The drone's own picture: five features of one owner share a lane, and one vertical run
    /// crosses it. Without the shared-lane rule every member of the bus draws an arc at the same
    /// point, so the one crossing renders as five stacked arcs.
    #[test]
    fn a_shared_lane_draws_one_arc_per_crossing() {
        // Five features leaving one owner: identical first corner, one shared horizontal lane at
        // y=50 running out to x=300, and a separate vertical wire crossing it at x=150.
        let mut lines: Vec<Vec<(f32, f32)>> = (0..5)
            .map(|i| {
                vec![
                    (100.0, 10.0),
                    (100.0, 50.0),
                    (300.0, 50.0),
                    (300.0, 60.0 + i as f32 * 10.0),
                ]
            })
            .collect();
        lines.push(vec![(150.0, 30.0), (150.0, 90.0)]);
        let jumps = hops(&lines);
        let drawn: usize = jumps.iter().map(Vec::len).sum();
        assert_eq!(
            drawn, 1,
            "one crossing means one arc, drawn by one strand of the bus: {jumps:?}"
        );
    }

    #[test]
    fn the_path_contains_an_arc_and_keeps_both_ends() {
        let points = vec![(0.0, 50.0), (100.0, 50.0)];
        let d = path(&points, &[(50.0, 50.0)], RADIUS);
        assert!(d.starts_with("M0.0 50.0"), "{d}");
        assert!(d.ends_with("L100.0 50.0"), "{d}");
        assert!(d.contains(" A4.0 4.0"), "an arc is drawn: {d}");
        // The arc spans exactly the jump width: 2r, centred on the crossing.
        assert!(d.contains("L46.0 50.0"), "{d}");
        assert!(d.contains("54.0 50.0"), "{d}");
    }

    #[test]
    fn a_path_without_jumps_is_the_plain_polyline() {
        let points = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 20.0)];
        let d = path(&points, &[], RADIUS);
        assert_eq!(d, "M0.0 0.0 L10.0 0.0 L10.0 20.0");
    }
}
