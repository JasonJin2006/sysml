//! Where a variation's alternatives are put.
//!
//! A `variant part` is a member of its variation, so an ownership edge joins the two and the
//! ranker descends — which is right for a part inside a definition and wrong for the one case the
//! picture exists to show. Drawn one below another, the alternatives read as a sequence of members;
//! drawn in a row, they read as what the variation chooses between.

use sysml_view::graph;

const MODEL: &str = r#"package P {
    part def BatteryPack { attribute capacity : Real; }
    part def NmcPack :> BatteryPack { attribute energyDensity : Real; }
    part def LfpPack :> BatteryPack { attribute cycleLife : Real; }

    variation part def BatteryChoices :> BatteryPack {
        variant part longRange : NmcPack;
        variant part standardRange : LfpPack;
    }
}
"#;

/// Where each box ends up, by label.
fn positions(src: &str) -> Vec<(String, f32, f32)> {
    let loaded = sysml_syntax::load(src).expect("parses");
    let g = graph::general(&loaded.roots, sysml_view::Mode::Extended);
    let geom = sysml_view::general_geometry(&g, sysml_view::layered::Style::Nested);
    geom.boxes
        .iter()
        .map(|&(i, x, y, _, _)| (g.nodes[i].label().to_string(), x, y))
        .collect()
}

fn at(list: &[(String, f32, f32)], name: &str) -> (f32, f32) {
    list.iter()
        .find(|(label, _, _)| label == name)
        .map(|(_, x, y)| (*x, *y))
        .unwrap_or_else(|| {
            panic!(
                "no box called {name}: {:?}",
                list.iter().map(|(l, _, _)| l).collect::<Vec<_>>()
            )
        })
}

#[test]
fn the_alternatives_share_a_row_with_their_variation() {
    let list = positions(MODEL);
    let (_, variation_y) = at(&list, "BatteryChoices");
    let (_, long_y) = at(&list, "longRange");
    let (_, standard_y) = at(&list, "standardRange");
    assert!(
        (long_y - variation_y).abs() < 1.0 && (standard_y - variation_y).abs() < 1.0,
        "variation at y={variation_y}, alternatives at y={long_y} and y={standard_y}"
    );
}

#[test]
fn the_alternatives_are_side_by_side_not_stacked() {
    let list = positions(MODEL);
    let (long_x, _) = at(&list, "longRange");
    let (standard_x, _) = at(&list, "standardRange");
    assert!(
        (long_x - standard_x).abs() > 20.0,
        "the two alternatives overlap in x: {long_x} and {standard_x}"
    );
}

#[test]
fn an_ordinary_part_is_still_below_its_definition() {
    // The rule is about a variation's alternatives, not about membership in general. A part inside
    // a definition stays one rank down, which is what makes a general view read as a tree.
    let src = "package P { part def Drone { part battery : Battery; } part def Battery; }";
    let list = positions(src);
    let (_, drone_y) = at(&list, "Drone");
    let (_, battery_y) = at(&list, "battery");
    assert!(
        battery_y > drone_y + 10.0,
        "a plain member should sit below its owner: Drone y={drone_y}, battery y={battery_y}"
    );
}
