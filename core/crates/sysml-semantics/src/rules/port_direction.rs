//! `port-direction` — a connector joins two ports whose directions can meet.
//!
//! A port's direction is the direction of the flow it carries: `out` produces, `in` consumes.
//! `connect a.p to b.q` joins two ports, and a join between two producers carries nothing —
//! neither side consumes what the other makes. The same holds for two consumers. What is legal is
//! `out` against `in`, and anything against a port that states no direction.
//!
//! # What this does not claim
//!
//! SysML v2 does not make directional compatibility a well-formedness rule. The direction is
//! described in prose and the OCL rules constrain a port's multiplicity and its conjugation, not
//! whether two connectors agree. So this reports under its own code, stays out of the conformance
//! count, and is a *warning*: a model may join two `out` ports on purpose — a tap, a test point, a
//! passive junction — and the language does not forbid it.
//!
//! `inout` is accepted against anything, including itself: it is both, which is what the word
//! means.
//!
//! The report code this group reports under is `SEM097`.

use sysml_model::{Direction, Element, Kind, NameRef};
use sysml_resolve::{Index, Resolution};

use super::{Severity, Violation};

const CODE: &str = "SEM097";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// The port a connector end names, if it names one.
///
/// `a.p` walks to `p` and then checks that `p` is a port. A bare `a` names a part, not a port,
/// and this rule has nothing to say about it.
fn port_at(index: &Index, from: usize, end: &str) -> Option<usize> {
    let text = end.trim().trim_start_matches('~').trim();
    if text.is_empty() {
        return None;
    }
    let name = NameRef::new(text, 0);
    let Resolution::Found(id) = index.resolve_path(from, &name) else {
        return None;
    };
    match index.kind.get(id) {
        Some(Kind::PortUsage | Kind::PortDef) => Some(id),
        _ => None,
    }
}

/// The direction a port carries, following its type when the port itself states none.
///
/// `port p : OutPort;` writes no direction: the declaration says what kind of port `p` is, and
/// `OutPort` is where `out` is written. Reading only the port's own modifier found `None` for
/// every port typed by a definition, which is the usual spelling — the rule fired only for a port
/// that stated its direction inline.
fn direction(index: &Index, id: usize) -> Option<Direction> {
    if let Some(own) = index.direction_of(id) {
        return Some(own);
    }
    // The type the port names. A port definition states the direction on the feature it carries
    // (`out item x`), so the answer is the direction of that feature.
    let type_text = index.type_of(id)?;
    let head = type_text.split(',').next().unwrap_or(type_text).trim();
    if head.is_empty() {
        return None;
    }
    let name = NameRef::new(head, 0);
    let Resolution::Found(def) = index.resolve(id, head) else {
        return None;
    };
    let _ = name;
    // `OutPort` itself carries no modifier; its feature does.
    index.children[def]
        .iter()
        .find_map(|&child| index.direction_of(child))
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::Connection {
            return;
        }
        let (Some(to), Some(from)) = (el.to.as_deref(), el.from.as_deref()) else {
            return;
        };
        let (Some(b), Some(a)) = (port_at(index, el.id, to), port_at(index, el.id, from)) else {
            return;
        };
        let (Some(db), Some(da)) = (direction(index, b), direction(index, a)) else {
            return;
        };
        // `inout` meets anything, and an undirected port is not a claim either way.
        if matches!(da, Direction::InOut) || matches!(db, Direction::InOut) {
            return;
        }
        if da != db {
            return;
        }
        let word = match da {
            Direction::Out => "producing",
            Direction::In => "consuming",
            Direction::InOut => unreachable!(),
        };
        out.push(Violation {
            code: CODE,
            element: qualified(el, index),
            line: index.line.get(el.id).copied().unwrap_or(0),
            message: format!(
                "`{}` joins `{from}` and `{to}`, both {word} ports; neither side receives what the \
                 other sends",
                qualified(el, index)
            ),
            severity: Severity::Warning,
        });
    });
    out
}
