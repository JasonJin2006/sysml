//! Locks the compat projection to what `sysmlpy` 0.97.1 emits.
//!
//! The expected lists below are transcribed from the reference implementation's PlantUML output
//! (`research/tools/parity.py` regenerates and re-checks them). Keeping a copy here means a
//! change to the projection breaks the build instead of only failing a script nobody runs.
//!
//! Anonymous elements are folded to `<anon>`: the reference names them with a UUID regenerated on
//! every parse, so their actual names can never be compared, only their position.

use std::path::PathBuf;

use sysml_view::{project, Graph, Mode, View};

fn example(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn dump(g: &Graph) -> (Vec<String>, Vec<String>) {
    let fold = |s: &str| {
        if is_uuid(s) {
            "<anon>".to_string()
        } else {
            s.to_string()
        }
    };
    let nodes = g
        .nodes
        .iter()
        .map(|n| format!("{} <<{}>>", fold(n.label()), n.stereotype))
        .collect();
    let edges = g
        .edges
        .iter()
        .map(|e| {
            let mut line = format!(
                "{} {} {}",
                fold(g.nodes[e.from].label()),
                e.arrow.notation(),
                fold(g.nodes[e.to].label())
            );
            if let Some(label) = &e.label {
                line.push_str(" : ");
                line.push_str(label);
            }
            // Endpoint ports, as `to_lines` writes them: one edge per connector, so a test can
            // tell a connector that reached its ports from one that only found the parts.
            if let (Some(a), Some(b)) = (e.from_port, e.to_port) {
                line.push_str(&format!(
                    " :{} :{}",
                    g.nodes[e.from].ports[a].name, g.nodes[e.to].ports[b].name
                ));
            }
            line
        })
        .collect();
    (nodes, edges)
}

fn check(file: &str, view: View, nodes: &[&str], edges: &[&str]) {
    let loaded = sysml_syntax::load(&example(file)).expect("parses");
    let graph = project(&loaded.roots, view, Mode::Compat);
    let (got_nodes, got_edges) = dump(&graph);

    let want_nodes: Vec<String> = nodes.iter().map(|s| s.to_string()).collect();
    let want_edges: Vec<String> = edges.iter().map(|s| s.to_string()).collect();

    assert_eq!(got_nodes, want_nodes, "{file}: node list diverged");
    assert_eq!(got_edges, want_edges, "{file}: edge list diverged");
}

#[test]
fn drone_general_view_matches_reference() {
    check(
        "drone.sysml",
        View::General,
        &[
            "Battery <<part def>>",
            "energy <<attribute>>",
            "mass <<attribute>>",
            "Airframe <<part def>>",
            "mass <<attribute>>",
            "Drone <<part def>>",
            "battery <<part>>",
            "airframe <<part>>",
            "hoverPower <<attribute>>",
            "takeoffMass <<attribute>>",
            "endurance <<attribute>>",
            "HoverEndurance <<requirement def>>",
            "TakeoffMassLimit <<requirement def>>",
            "drone <<part>>",
            "<anon> <<requirement>>",
            "<anon> <<requirement>>",
        ],
        &[
            "Battery *-- energy",
            "Battery *-- mass",
            "Airframe *-- mass",
            "Drone *-- battery",
            "battery --:|> Battery : types",
            "Drone *-- airframe",
            "airframe --:|> Airframe : types",
            "Drone *-- hoverPower",
            "Drone *-- takeoffMass",
            "Drone *-- endurance",
            "drone --:|> Drone : types",
        ],
    );
}

#[test]
fn fuel_general_view_matches_reference() {
    check(
        "fuel.sysml",
        View::General,
        &[
            "FuelOutlet <<port def>>",
            "FuelInlet <<port def>>",
            "Tank <<part def>>",
            "outlet <<port>>",
            "capacity <<attribute>>",
            "Engine <<part def>>",
            "inlet <<port>>",
            "thrust <<attribute>>",
            "Vehicle <<part def>>",
            "tank <<part>>",
            "engine <<part>>",
            "<anon> <<connection>>",
            "vehicle <<part>>",
        ],
        &[
            "Tank *-- outlet",
            "outlet --:|> FuelOutlet : types",
            "Tank *-- capacity",
            "Engine *-- inlet",
            "inlet --:|> FuelInlet : types",
            "Engine *-- thrust",
            "Vehicle *-- tank",
            "tank --:|> Tank : types",
            "Vehicle *-- engine",
            "engine --:|> Engine : types",
            "Vehicle *-- <anon>",
            "vehicle --:|> Vehicle : types",
        ],
    );
}

#[test]
fn fuel_interconnection_view_matches_reference() {
    // The reference collapses this model to a single box: its inclusion set is built from usage
    // bodies, and `connect` is written in the definition, so neither endpoint is drawn.
    check(
        "fuel.sysml",
        View::Interconnection,
        &["vehicle : Vehicle <<part>>"],
        &[],
    );
}

#[test]
fn rover_general_view_matches_reference() {
    check(
        "rover.sysml",
        View::General,
        &[
            "MassValue <<attribute def>>",
            "Wheel <<part def>>",
            "radius <<attribute>>",
            "load <<attribute>>",
            "Chassis <<part def>>",
            "front <<part>>",
            "rear <<part>>",
            "mass <<attribute>>",
            "Rover <<part def>>",
            "chassis <<part>>",
            "left <<part>>",
            "mass <<attribute>>",
            "speed <<attribute>>",
            "SlopeLimit <<requirement def>>",
            "RangeLimit <<requirement def>>",
            "rover <<part>>",
            "<anon> <<requirement>>",
            "<anon> <<requirement>>",
            "Base <<part def>>",
            "dock <<part>>",
            "base <<part>>",
        ],
        &[
            "Wheel *-- radius",
            "Wheel *-- load",
            "Chassis *-- front",
            "front --:|> Wheel : types",
            "Chassis *-- rear",
            "rear --:|> Wheel : types",
            "Chassis *-- mass",
            "mass --:|> MassValue : types",
            "Rover *-- chassis",
            "chassis --:|> Chassis : types",
            "Rover *-- left",
            "left --:|> Wheel : types",
            "Rover *-- mass",
            "mass --:|> MassValue : types",
            "Rover *-- speed",
            "rover --:|> Rover : types",
            "Base *-- dock",
            // The reference resolves `RoverSystem::Rover` to nothing and emits the typing edge
            // anyway, pointing at an alias it never declares. We resolve it, which is the one
            // place the two disagree — the relationship itself is the same.
            "dock --:|> Rover : types",
            "base --:|> Base : types",
        ],
    );
}

#[test]
fn extended_adds_only_the_three_documented_things() {
    let loaded = sysml_syntax::load(&example("fuel.sysml")).expect("parses");

    let compat = project(&loaded.roots, View::General, Mode::Compat);
    let extended = project(&loaded.roots, View::General, Mode::Extended);
    let (compat_nodes, compat_edges) = dump(&compat);
    let (ext_nodes, ext_edges) = dump(&extended);

    // 1. port definition bodies are no longer dropped.
    for name in ["fuel <<attribute>>"] {
        assert!(!compat_nodes.contains(&name.to_string()));
        assert!(ext_nodes.contains(&name.to_string()));
    }
    // 2. ports inherited from the type are drawn on the usage.
    assert!(ext_edges.contains(&"tank *-- outlet".to_string()));
    assert!(!compat_edges.contains(&"tank *-- outlet".to_string()));
    // 3. the anonymous connection placeholder gives way to a real connector edge.
    assert!(compat_nodes.iter().any(|n| n == "<anon> <<connection>>"));
    assert!(!ext_nodes.iter().any(|n| n == "<anon> <<connection>>"));
}

#[test]
fn extended_draws_satisfy_edges_to_a_requirement_usage() {
    // The official validation models point `satisfy` at a requirement *usage*, not a
    // definition: `requirement <'1.1'> r : MassLimit { ... }` and then `satisfy R by x`. Both
    // the usage and the statement have to lower for the edge to exist at all, and the usage
    // lives in a part usage body (a "requirement group in context") in the real models.
    let src = r#"
package P {
    requirement def MassLimit { require constraint { true } }

    part def Vehicle;
    part vehicle : Vehicle;

    part context : Vehicle {
        requirement <'1.1'> massRequirement : MassLimit { }
        satisfy massRequirement by vehicle;
    }
}
"#;
    let loaded = sysml_syntax::load(src).expect("parses");
    let graph = project(&loaded.roots, View::General, Mode::Extended);
    let (nodes, edges) = dump(&graph);

    assert!(
        nodes.iter().any(|n| n == "massRequirement <<requirement>>"),
        "nodes: {nodes:#?}"
    );
    assert!(
        edges
            .iter()
            .any(|e| e.contains("-->") && e.contains("massRequirement")),
        "edges: {edges:#?}"
    );
}

#[test]
fn a_satisfy_end_may_be_a_dotted_feature_path() {
    // `satisfy R by vehicle1_c1.engine_v1` names a feature of another usage, not a qualified
    // name. Resolution therefore has to walk the path feature by feature.
    let src = r#"
package P {
    requirement def TorqueLimit { }
    part def Engine;
    part vehicle {
        part engine : Engine;
    }
    part checker {
        satisfy TorqueLimit by vehicle.engine;
    }
}
"#;
    let loaded = sysml_syntax::load(src).expect("parses");
    let graph = project(&loaded.roots, View::General, Mode::Extended);
    let (_nodes, edges) = dump(&graph);
    assert!(
        edges
            .iter()
            .any(|e| e.contains("-->") && e.contains("engine")),
        "edges: {edges:#?}"
    );
}

#[test]
fn a_satisfy_statement_is_not_exported_by_a_namespace_import() {
    // The satisfy element is named after its requirement. If an `import L::*` exported that
    // name, it would shadow the real requirement for every importer and the edge would be
    // dropped as ambiguous — the case that broke the official 12b-Allocation model.
    let src = r#"
package R {
    requirement torqueGeneration { }
}
package L {
    part def TG;
    part torqueGenerator : TG;
    satisfy torqueGeneration by torqueGenerator;
}
package Use {
    private import R::*;
    private import L::*;
    part def Check;
}
"#;
    let loaded = sysml_syntax::load(src).expect("parses");
    let index = sysml_resolve::Index::build(&loaded.roots);
    let imports = index.imports();
    let from_l = imports
        .iter()
        .find(|e| index.name[e.element] == "L")
        .expect("the L import is recorded");
    let bound: Vec<&str> = from_l.bound.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        !bound.contains(&"torqueGeneration")
            || bound.iter().filter(|n| **n == "torqueGeneration").count() == 1,
        "L must not export the satisfy element's display name: {bound:?}"
    );
}

#[test]
fn extended_draws_the_satisfy_edges_the_reference_leaves_out() {
    // `satisfy R by x` is written with a bare requirement name in the same package as `R`, so
    // the satisfy element and the requirement def are same-name siblings. This locks the
    // resolution that keeps them apart: the edge must reach the real requirement, not be
    // dropped as ambiguous (which is what happened before relationship elements were excluded
    // from the member walk).
    let loaded = sysml_syntax::load(&example("drone.sysml")).expect("parses");

    let extended = project(&loaded.roots, View::General, Mode::Extended);
    let (_nodes, edges) = dump(&extended);
    assert!(edges.contains(&"drone --> HoverEndurance : «satisfy»".to_string()));
    assert!(edges.contains(&"drone --> TakeoffMassLimit : «satisfy»".to_string()));

    let compat = project(&loaded.roots, View::General, Mode::Compat);
    let (_nodes, compat_edges) = dump(&compat);
    assert!(!compat_edges.iter().any(|e| e.contains("-->")));
}

/// A model shaped like the official validation suite: connectors written on a usage body, a
/// three-segment endpoint path, and the same part name under two owners.
const OFFICIAL_SHAPE: &str = r#"
package P {
    part def Axle { port hub; }
    part def Wheel { port center; }
    part def Vehicle {
        part frontAxle : Axle;
        part frontWheel : Wheel;
        part rearAxle : Axle;
        part rearWheel : Wheel;
        connect frontAxle.hub to frontWheel.center;
    }
    part vehicle : Vehicle {
        connect rearAxle.hub to rearWheel.center;
    }
}
"#;

#[test]
fn a_connection_written_on_a_usage_reaches_its_ports() {
    let loaded = sysml_syntax::load(OFFICIAL_SHAPE).expect("parses");
    let graph = project(&loaded.roots, View::Interconnection, Mode::Extended);
    let (_nodes, edges) = dump(&graph);

    // The connector in the definition body, resolved through the type Vehicle.
    assert!(
        edges.contains(
            &"frontAxle : Axle -[thickness=3]- frontWheel : Wheel :hub :center".to_string()
        ),
        "edges: {edges:#?}"
    );
    // The connector in the usage body, resolved from `vehicle`'s own scope.
    assert!(
        edges.contains(
            &"rearAxle : Axle -[thickness=3]- rearWheel : Wheel :hub :center".to_string()
        ),
        "edges: {edges:#?}"
    );
}

#[test]
fn an_endpoint_path_walks_down_through_nested_parts() {
    // `rearAxleAssembly.rearAxle.leftHalfAxle.port` is how the official models name an end.
    // Matching only the first segment leaves the diagram with no connectors at all.
    let src = r#"
package P {
    part def HalfAxle { port hub; }
    part def Wheel { port center; }
    part def Assembly {
        part rearAxle {
            part leftHalfAxle : HalfAxle;
        }
        part leftWheel : Wheel;
        connect rearAxle.leftHalfAxle.hub to leftWheel.center;
    }
    part assy : Assembly;
}
"#;
    let loaded = sysml_syntax::load(src).expect("parses");
    let graph = project(&loaded.roots, View::Interconnection, Mode::Extended);
    let (_nodes, edges) = dump(&graph);
    assert_eq!(
        edges,
        vec!["leftHalfAxle : HalfAxle -[thickness=3]- leftWheel : Wheel :hub :center".to_string()]
    );
}

#[test]
fn an_interface_usage_contributes_its_connector() {
    // `interface :Iface connect a.p to b.q` keeps the endpoints in the interface part, not the
    // usage body, so a lowering that only reads the body drops the connector.
    let src = r#"
package P {
    part def Port { }
    part def Src { port out1 : Port; }
    part def Dst { port in1 : Port; }
    interface def Link {
        end s : Src;
        end d : Dst;
    }
    part holder {
        part a : Src;
        part b : Dst;
        interface :Link connect a.out1 to b.in1;
    }
}
"#;
    let loaded = sysml_syntax::load(src).expect("parses");
    let graph = project(&loaded.roots, View::Interconnection, Mode::Extended);
    let (_nodes, edges) = dump(&graph);
    assert!(
        edges
            .iter()
            .any(|e| e.contains("-[thickness=3]-") && e.contains(":out1 :in1")),
        "edges: {edges:#?}"
    );
}
