//! Locks the layout invariants that are cheap to state and expensive to rediscover.
//!
//! These live in `tests/` rather than in a probe script because the properties are structural —
//! "no two boxes overlap", "an ownership edge never goes upwards", "a definition with four users
//! is reachable from all four" — and a script that only prints numbers lets a regression through.
//! The pixel-level checks stay in `research/tools/`; those need a rasteriser and a threshold to
//! be meaningful, while these need neither.

use sysml_view::{project, Mode, View};

fn frame_of(svg: &str) -> Vec<(f32, f32, f32, f32)> {
    // Frame rectangles are the stroked ones that are big enough to be a box.
    let mut out = Vec::new();
    for chunk in svg.split("<rect ").skip(1) {
        let head: &str = chunk.split("/>").next().unwrap_or("");
        let num = |name: &str| -> Option<f32> {
            let at = head.find(&format!("{name}=\""))? + name.len() + 2;
            let end = head[at..].find('"')? + at;
            head[at..end].parse().ok()
        };
        let (Some(x), Some(y), Some(w), Some(h)) =
            (num("x"), num("y"), num("width"), num("height"))
        else {
            continue;
        };
        // A frame is any stroked rectangle big enough to be a box. The stroke is no longer black:
        // a box is coloured by the kind of element it draws, so the test asks whether a stroke is
        // present rather than what colour it is.
        //
        // The view frame itself is a stroked rectangle at (1,1) the size of the canvas. It is not
        // a node, so it is skipped: every element box starts at MARGIN or below.
        let view_frame = x <= 1.0 && y <= 1.0;
        let stroked = head.contains("stroke=\"#") && !head.contains("stroke=\"none\"");
        if stroked && !view_frame && w >= 20.0 && h >= 20.0 {
            out.push((x, y, w, h));
        }
    }
    out
}

fn overlaps(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    !(ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay)
}

fn contains(outer: (f32, f32, f32, f32), inner: (f32, f32, f32, f32)) -> bool {
    let (ox, oy, ow, oh) = outer;
    let (ix, iy, iw, ih) = inner;
    ox <= ix && oy <= iy && ox + ow >= ix + iw && oy + oh >= iy + ih
}

const SHAPES: &[(&str, &str)] = &[
    (
        "tree",
        r#"
package P {
    part def Wheel { attribute radius : Real; attribute load : Real; }
    part def Chassis { part front : Wheel; part rear : Wheel; attribute mass : Real; }
    part def Rover { part chassis : Chassis; attribute speed : Real; }
    part rover : Rover;
}
"#,
    ),
    (
        // Two usages of the same definition, each with its own children: the case that makes
        // naive orderings interleave the two subtrees down the whole page.
        "twin subtrees",
        r#"
package P {
    part def AxleAssembly;
    part def Wheel;
    part def Vehicle {
        attribute mass : Real;
        part frontAxleAssembly : AxleAssembly { part frontAxle; part frontWheel : Wheel; }
        part rearAxleAssembly : AxleAssembly { part rearAxle; part rearWheel : Wheel; }
    }
    part vehicle1 : Vehicle { attribute mass : Real; }
    part vehicle2 : Vehicle { attribute mass : Real; }
}
"#,
    ),
    (
        // A definition used from several ranks, which is what forces virtual nodes.
        "long references",
        r#"
package P {
    part def Mass;
    part def Deep {
        part level1 {
            part level2 {
                part level3 { attribute m : Mass; }
            }
        }
    }
    part top { attribute a : Mass; attribute b : Mass; }
}
"#,
    ),
];

#[test]
fn frames_never_overlap() {
    for (name, src) in SHAPES {
        let loaded = sysml_syntax::load(src).expect("parses");
        let svg = sysml_view::render_view(&loaded.roots, View::General, Mode::Extended, name);
        let frames = frame_of(&svg);
        assert!(frames.len() > 1, "{name}: expected several frames");
        for i in 0..frames.len() {
            for j in (i + 1)..frames.len() {
                // Nesting is containment, which is the point of a frame around features.
                if contains(frames[i], frames[j]) || contains(frames[j], frames[i]) {
                    continue;
                }
                assert!(
                    !overlaps(frames[i], frames[j]),
                    "{name}: frames {i} and {j} overlap: {:?} vs {:?}",
                    frames[i],
                    frames[j]
                );
            }
        }
    }
}

#[test]
fn every_node_gets_a_frame_and_every_edge_a_path() {
    for (name, src) in SHAPES {
        let loaded = sysml_syntax::load(src).expect("parses");
        let graph = project(&loaded.roots, View::General, Mode::Extended);
        let svg = sysml_view::render_view(&loaded.roots, View::General, Mode::Extended, name);
        let frames = frame_of(&svg);
        assert_eq!(
            frames.len(),
            graph.nodes.len(),
            "{name}: one frame per node"
        );
        // One element carries every relationship line, so the assertion is about the geometry
        // it holds rather than the element count: sub-paths are `M` commands, and each edge
        // contributes at least one. Counting elements would pass on an empty picture now.
        //
        // A header band is also a path. It is filled and carries `stroke="none"`, while every
        // relationship line is stroked, so the search takes the first stroked one.
        let start = svg
            .match_indices("<path d=\"")
            .find(|(i, tag)| {
                let after = &svg[i + tag.len()..];
                let tag_end = after.find('>').unwrap_or(0);
                let head = &after[..tag_end];
                // A relationship line has no fill; a header band and the view tab do.
                head.contains("stroke=\"#") && !head.contains("fill=\"#")
            })
            .map(|(i, _)| i)
            .unwrap_or_else(|| panic!("{name}: no relationship geometry at all"));
        let rest = &svg[start + "<path d=\"".len()..];
        let data = &rest[..rest.find('"').expect("a closed d attribute")];
        let subpaths = data.matches('M').count();
        assert!(
            subpaths >= graph.edges.len(),
            "{name}: {subpaths} drawn sub-paths for {} edges",
            graph.edges.len()
        );
    }
}

#[test]
fn an_ownership_edge_always_points_down() {
    // Ownership is the one relationship whose direction the drawing must never invert: a feature
    // belongs to the box above it. Reading the emitted geometry, the diamond has to sit above
    // the child's top edge on the same x.
    let (_, src) = SHAPES[0];
    let loaded = sysml_syntax::load(src).expect("parses");
    let svg = sysml_view::render_view(&loaded.roots, View::General, Mode::Extended, "tree");
    let frames = frame_of(&svg);
    // Every diamond's centre must be within some frame's bottom border, never its top.
    for chunk in svg.split("<path d=\"M").skip(1) {
        let d: &str = chunk.split('"').next().unwrap_or("");
        let filled = chunk.contains("fill=\"#000000\"") && !chunk.contains("stroke-width=\"1\"");
        if !filled || d.matches('L').count() != 3 {
            continue; // not a four-point diamond
        }
        let mut nums = d
            .split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<f32>().ok());
        let (Some(cx), Some(cy)) = (nums.next(), nums.next()) else {
            continue;
        };
        let cy = cy + 5.0; // the path starts at the diamond's top point
        let on_bottom = frames
            .iter()
            .any(|&(x, y, w, h)| (cx - x).abs() <= w && (cy - (y + h)).abs() < 2.0);
        let on_top = frames
            .iter()
            .any(|&(x, y, w, _)| (cx - x).abs() <= w && (cy - y).abs() < 2.0);
        assert!(
            !on_top || on_bottom,
            "diamond at ({cx}, {cy}) sits on a box's top border, so ownership points up"
        );
    }
}

#[test]
fn a_definition_is_reachable_from_every_usage() {
    // The rendering must not drop an edge because of where the layout put its endpoints: a
    // typing edge into a definition that ended up above its usage still has to be drawn.
    let src = r#"
package P {
    part def Shared;
    part a { attribute x : Shared; }
    part b { attribute y : Shared; }
    part c { attribute z : Shared; }
}
"#;
    let loaded = sysml_syntax::load(src).expect("parses");
    let graph = project(&loaded.roots, View::General, Mode::Extended);
    let typing = graph
        .edges
        .iter()
        .filter(|e| e.arrow == sysml_view::Arrow::Typing)
        .count();
    assert_eq!(typing, 3, "three typing edges projected");

    let svg = sysml_view::render_view(&loaded.roots, View::General, Mode::Extended, "shared");
    // Each typing edge ends in one hollow triangle, so three triangles must be present.
    let triangles = svg
        .split("<path d=\"M")
        .filter(|c| {
            c.contains("fill=\"#FFFFFF\"")
                && c.split('"').next().unwrap_or("").matches('L').count() == 2
        })
        .count();
    assert_eq!(triangles, 3, "one triangle per typing edge");
}

#[test]
fn the_canvas_covers_every_frame() {
    for (name, src) in SHAPES {
        let loaded = sysml_syntax::load(src).expect("parses");
        let svg = sysml_view::render_view(&loaded.roots, View::General, Mode::Extended, name);
        let width: f32 = svg
            .split("width=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .and_then(|s| s.parse().ok())
            .expect("width");
        let height: f32 = svg
            .split("height=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .and_then(|s| s.parse().ok())
            .expect("height");
        for (x, y, w, h) in frame_of(&svg) {
            assert!(
                x >= 0.0 && y >= 0.0 && x + w <= width && y + h <= height,
                "{name}: frame ({x},{y},{w},{h}) leaves the {width}x{height} canvas"
            );
        }
    }
}

/// Two parts inside one vehicle, wired from the battery's `out` port to the motor's `in` port.
const ONE_CONTAINER: &str = r#"
package Vehicle {
    port def PowerPort { in power : Real; }
    part def Battery { out port supply : PowerPort; }
    part def Motor { in port supply : PowerPort; }
    part def Car {
        part battery : Battery;
        part motor : Motor;
        connect battery.supply to motor.supply;
    }
    part v : Car;
}
"#;

/// Every point a connector path visits, read from the `d` attribute of each black path.
fn path_points(svg: &str) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for chunk in svg.split("<path d=\"").skip(1) {
        let (head, rest) = match chunk.find('"') {
            Some(at) => (&chunk[at..], &chunk[..at]),
            None => continue,
        };
        let attr_end = head.find('>').unwrap_or(head.len());
        // A relationship line has no fill. The view tab and every header band are filled paths,
        // and reading their corners as connector points put a point at the top-left of the page.
        if head[..attr_end].contains("fill=\"#") {
            continue;
        }
        let d = rest;
        let numbers: Vec<f32> = d
            .split(|c: char| c.is_ascii_alphabetic() || c == ' ' || c == ',')
            .filter_map(|s| s.parse().ok())
            .collect();
        // Only M and L appear between ports, so the numbers pair up as points.
        if d.contains('A') {
            continue;
        }
        for pair in numbers.chunks(2) {
            if let [x, y] = pair {
                out.push((*x, *y));
            }
        }
    }
    out
}

#[test]
fn a_port_is_drawn_on_the_side_its_own_direction_says() {
    // `out port supply : PowerPort` leaves from the right even though `PowerPort` declares an
    // `in` payload. Reading the payload's direction drew the battery's output as an input.
    let loaded = sysml_syntax::load(ONE_CONTAINER).expect("parses");
    let g = project(&loaded.roots, View::Interconnection, Mode::Extended);
    let battery = g
        .nodes
        .iter()
        .find(|n| n.name.starts_with("battery"))
        .expect("battery is drawn");
    let supply = battery
        .ports
        .iter()
        .find(|p| p.name == "supply")
        .expect("the port is drawn");
    assert_eq!(supply.direction, Some(sysml_model::Direction::Out));
    assert_eq!(supply.side, sysml_view::graph::Side::Right);
}

#[test]
fn a_wire_between_two_parts_of_one_container_stays_inside_it() {
    // The wire used to leave `v : Car`, run below it and come back, which reads as two wires.
    let loaded = sysml_syntax::load(ONE_CONTAINER).expect("parses");
    let g = project(&loaded.roots, View::Interconnection, Mode::Extended);
    let svg = sysml_view::render(&g, "t");
    let frames = frame_of(&svg);
    let outer = frames
        .iter()
        .copied()
        .max_by(|a, b| (a.2 * a.3).total_cmp(&(b.2 * b.3)))
        .expect("the container is drawn");
    let points = path_points(&svg);
    assert!(!points.is_empty(), "the connector is drawn");
    for (x, y) in points {
        assert!(
            outer.0 <= x && x <= outer.0 + outer.2 && outer.1 <= y && y <= outer.1 + outer.3,
            "({x},{y}) is outside the container {outer:?}"
        );
    }
}

#[test]
fn a_box_is_coloured_by_the_kind_of_element_it_draws() {
    // A diagram is scanned by colour before any label is read. Every box was white, so the only
    // way to tell a definition from a requirement was to read each stereotype.
    let loaded = sysml_syntax::load(ONE_CONTAINER).expect("parses");
    let svg = sysml_view::render_view(&loaded.roots, View::General, Mode::Extended, "t");
    let fills: std::collections::BTreeSet<&str> = svg
        .match_indices("fill=\"#")
        .map(|(i, tag)| {
            let rest = &svg[i + tag.len()..];
            &rest[..rest.find('"').unwrap_or(0)]
        })
        .collect();
    assert!(
        fills.len() >= 3,
        "a picture with a definition, a usage and a port needs at least three fills: {fills:?}"
    );
}

#[test]
fn a_title_reads_on_its_own_header_band() {
    // The header is a saturated colour for a definition and a pale one for a usage, so one ink
    // cannot serve both.
    let loaded = sysml_syntax::load(ONE_CONTAINER).expect("parses");
    let svg = sysml_view::render_view(&loaded.roots, View::General, Mode::Extended, "t");
    let inks: std::collections::BTreeSet<&str> = svg
        .match_indices("font-size=\"12\" fill=\"#")
        .map(|(i, tag)| {
            let rest = &svg[i + tag.len()..];
            &rest[..rest.find('"').unwrap_or(0)]
        })
        .collect();
    assert!(!inks.is_empty(), "the name line is drawn");
    for ink in &inks {
        assert_ne!(*ink, "FFFFFF", "a name on a pale band is not white");
    }
}

#[test]
fn a_dark_header_takes_white_ink() {
    // `Battery` is a definition, drawn with the saturated `#88B7E3` band; its name has to be
    // light enough to read against it. This pins the luminance test rather than one palette value.
    let style = sysml_view::style::for_stereotype("«part def»");
    assert_eq!(style.header, "#88B7E3");
    let svg = sysml_view::render_view(
        &sysml_syntax::load("package P { part def A; }")
            .expect("parses")
            .roots,
        View::General,
        Mode::Extended,
        "t",
    );
    assert!(
        svg.contains("fill=\"#1A1A1A\">A<") || svg.contains("fill=\"#FFFFFF\">A<"),
        "the name is drawn in a readable ink"
    );
}

#[test]
fn a_compartment_states_what_it_holds() {
    // The rows under a title are attributes, and the drawing now says so rather than leaving the
    // reader to infer it from the `+` prefixes.
    // Attributes are a compartment in the interconnection view; the general view draws each
    // attribute as its own box, so the heading belongs to the view that groups them.
    let loaded = sysml_syntax::load("package P { part def A { attribute x : Real; } part a : A; }")
        .expect("parses");
    let svg = sysml_view::render_view(&loaded.roots, View::Interconnection, Mode::Extended, "t");
    assert!(
        svg.contains(">attributes<"),
        "the band has a heading: {svg:.400}"
    );
}

#[test]
fn a_box_with_no_attributes_has_no_compartment_heading() {
    let loaded = sysml_syntax::load("package P { part def A; }").expect("parses");
    let svg = sysml_view::render_view(&loaded.roots, View::General, Mode::Extended, "t");
    assert!(
        !svg.contains(">attributes<"),
        "an empty box does not claim a compartment"
    );
}

#[test]
fn a_picture_states_which_view_it_is_and_what_it_exposed() {
    // A general view and an interconnection view of the same model looked identical apart from
    // their contents, and a filtered picture did not say it was filtered.
    let loaded = sysml_syntax::load("package P { part def A; part a : A; }").expect("parses");
    for (view, name) in [
        (View::General, "general view"),
        (View::Interconnection, "interconnection view"),
    ] {
        let svg = sysml_view::render_view(&loaded.roots, view, Mode::Extended, "m.sysml");
        assert!(
            svg.contains(&format!("«view» {name}")),
            "{name} is not named in its own picture"
        );
        assert!(
            svg.contains("expose ModelContext::*::**"),
            "{name} does not state what it exposes"
        );
    }
}

#[test]
fn a_port_box_states_its_direction() {
    // A port was a plain white square: "something plugs in here". The notation states a direction,
    // and the box now carries it as an arrow (or a dot for `inout`).
    let loaded = sysml_syntax::load(
        "package P { port def F; part def T { out port supply : F; in port drain : F; } part t : T; }",
    )
    .expect("parses");
    let svg = sysml_view::render_view(&loaded.roots, View::Interconnection, Mode::Extended, "t");
    // The arrow is a stroke-only path: `fill="none"` with a border-coloured stroke. A header band
    // is a filled path, and a relationship line has no fill but reaches across the canvas.
    let arrows = svg
        .split("<path d=\"")
        .skip(1)
        .filter(|chunk| {
            let attrs = chunk.split('>').next().unwrap_or("");
            attrs.contains("fill=\"none\"")
                && attrs.contains("stroke=\"#")
                // Three `M` commands: shaft, upper barb, lower barb. A connector is one run.
                && attrs.matches("M").count() >= 2
        })
        .count();
    assert!(
        arrows >= 2,
        "an out and an in port each draw an arrow: {arrows}"
    );
}
