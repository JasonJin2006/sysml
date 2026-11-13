//! View projection: model tree -> node/edge graph.
//!
//! The projections here are deliberately narrow copies of what `sysmlpy` 0.97.1 emits, so a
//! diagram can be diffed against the reference implementation instead of being judged by eye.
//!
//! General view follows `sysmlpy.plantuml.as_general_view`:
//!   * packages are flattened, never drawn (plantuml.py:1061-1065);
//!   * every remaining element becomes one node, features included — they are not folded into
//!     a compartment (plantuml.py:1070-1075);
//!   * ownership becomes `*--` from owner to feature, and only when the owner is not a package
//!     (plantuml.py:1077-1085);
//!   * typing becomes `--:|>` labelled `types` (plantuml.py:1097-1107);
//!   * definitions are square-cornered, usages round-cornered (plantuml.py:3779-3786).
//!
//! Interconnection view follows the documented model of `as_interconnection_view_boxes`
//! (boxes_view.py:1169-1187), which is the one that states the notation rather than leaving it
//! implicit: part usages as boxes typed in the label (`tank : Tank`), definitions that a drawn
//! usage is typed by are *not* drawn separately (plantuml.py:1531-1534), attributes in the
//! attribute compartment as `+ name : Type`, ports as boundary ports on the side their
//! direction implies, connections as edges between ports.
//!
//! One rule goes beyond the reference, because the reference leaves the case unresolved: a
//! usage with no nested parts of its own inherits the composite structure of the definition it
//! is typed by (boxes_view.py:1273-1287 catalogs only usage bodies, so `part vehicle : Vehicle`
//! with `connect` inside `part def Vehicle` yields nodes for neither end). Attributes and ports
//! are inherited by the reference (boxes_view.py:1310-1336, 1350-1365); nested parts are the
//! same mechanism applied one level further.

use sysml_model::{Direction, Element, Kind};

/// The last segment of a written end, with its quotes removed.
///
/// An end such as `a.b`, `Power::off` or `'a 1'` names the element `b`, `off` or `a 1`. Two
/// details decide whether that element is found. A quoted name may itself contain `.` or `:`, so
/// the split runs only outside quotes; and the quotes are dropped, because the authored end keeps
/// them while the element it names does not. Before both steps an end written `'a 1'` came back
/// as `1'`, matched no drawn node, and every edge that named the node vanished from the view.
pub(crate) fn bare_end(text: &str) -> &str {
    let text = text.trim();
    let mut last = 0;
    let mut quoted = false;
    for (i, ch) in text.char_indices() {
        match ch {
            '\'' => quoted = !quoted,
            '.' | ':' if !quoted => last = i + 1,
            _ => {}
        }
    }
    let segment = text[last..].trim();
    segment
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .unwrap_or(segment)
}

/// Which projection to build.
///
/// `Compat` reproduces `sysmlpy` 0.97.1 node for node and edge for edge, including the parts of
/// its behaviour that are arguably wrong, so the parity check can demand an empty diff. `Extended`
/// adds the three things the reference leaves out; each one is marked at the point it is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Compat,
    Extended,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "compat" => Some(Mode::Compat),
            "extended" => Some(Mode::Extended),
            _ => None,
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            Mode::Compat => "compat",
            Mode::Extended => "extended",
        }
    }
}

/// The reference names elements that have no name of their own with a freshly generated UUID, so
/// its output changes on every parse. This derives a stable 36-character stand-in from the same
/// inputs instead: same shape, same length, diffable between runs.
fn anon_id(kind: &str, line: u32) -> String {
    fn fnv1a(seed: u64, bytes: &[u8]) -> u64 {
        let mut hash = seed;
        for byte in bytes {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
    let material = format!("{kind}:{line}");
    let bytes = material.as_bytes();
    let hi = fnv1a(0xcbf29ce484222325, bytes);
    let lo = fnv1a(0x9e3779b97f4a7c15, bytes);
    let hex = format!("{hi:016x}{lo:016x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arrow {
    Composite,
    Typing,
    Connector,
    Satisfy,
    /// A state transition: an open arrowhead, the same shape `-->` draws.
    Transition,
    /// An annotation: the element carries `@Type { … }`. Dashed, because the arrow says what is
    /// written *about* the element rather than what the element is made of.
    Annotate,
}

impl Arrow {
    /// Notation string, kept identical to `ARROW_STYLES` in plantuml.py:29-76 so the parity
    /// check can compare the two directly.
    pub fn notation(self) -> &'static str {
        match self {
            Arrow::Composite => "*--",
            Arrow::Typing => "--:|>",
            Arrow::Connector => "-[thickness=3]-",
            Arrow::Satisfy => "-->",
            Arrow::Annotate => "..>",
            // The same open arrow `-->` draws; the state view has no second notation to invent.
            Arrow::Transition => "-->",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub struct Port {
    pub name: String,
    pub direction: Option<Direction>,
    pub side: Side,
}

#[derive(Debug, Clone)]
pub struct Node {
    /// PlantUML-style alias (`E1`, `E2`) so a diff against the reference is alias-free.
    pub alias: String,
    /// Display label: the bare name, or `tank : Tank` in the interconnection view.
    pub name: String,
    /// The declared name without the type the label carries. Connection endpoints name this,
    /// never the label, so the two are kept apart instead of parsing `: Type` back off.
    pub bare: String,
    pub stereotype: String,
    pub is_def: bool,
    /// True when the node is drawn as a frame with a title band rather than as a box.
    ///
    /// A package in the package view and a state machine in the state transition view. An element
    /// with children (a definition with ports) must not be mistaken for one, so the distinction is
    /// recorded here rather than inferred from the presence of children or from the stereotype
    /// text — that guess drew `part def Battery` as a frame labelled «part def» with its own
    /// attributes inside.
    pub is_frame: bool,
    /// `+ name : Type` rows, interconnection view only.
    pub attributes: Vec<String>,
    /// Requirement predicate, general view only.
    pub constraint: Option<String>,
    pub ports: Vec<Port>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// The model element this node was projected from.
    ///
    /// The renderer never reads it: it exists so an editor can go from a box back to the
    /// declaration it stands for, which is what makes a drawn box addressable rather than merely
    /// visible. `None` on the anonymous placeholders compat mode emits for relationships, since
    /// those have no element of their own.
    pub element: Option<usize>,
}

impl Node {
    /// Label as the reference writes it: the declared name, and `unnamed` when there is none
    /// (plantuml.py:427-442).
    pub fn label(&self) -> &str {
        if self.name.is_empty() {
            "unnamed"
        } else {
            &self.name
        }
    }
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub arrow: Arrow,
    pub label: Option<String>,
    pub from_port: Option<usize>,
    pub to_port: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    General,
    Interconnection,
    /// Requirements against the elements that satisfy them. See `trace.rs`.
    Traceability,
    /// The namespace tree, packages as containers. See `packages.rs`.
    Packages,
    /// States inside their machine, transitions as edges. See `states.rs`.
    StateTransition,
    /// Actions inside their owner, successions as edges. See `states.rs`.
    ActionFlow,
    /// Parts as lifelines, messages in source order. See `sequence.rs`.
    Sequence,
}

pub struct Graph {
    pub view: View,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    pub(crate) fn new(view: View) -> Self {
        Self {
            view,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    fn push(&mut self, name: &str, bare: &str, kind: &Kind, parent: Option<usize>) -> usize {
        let alias = format!("E{}", self.nodes.len() + 1);
        let index = self.nodes.len();
        self.nodes.push(Node {
            alias,
            name: name.to_string(),
            bare: bare.to_string(),
            stereotype: kind.keyword().to_string(),
            is_def: kind.is_definition(),
            is_frame: matches!(kind, Kind::Package),
            attributes: Vec::new(),
            constraint: None,
            ports: Vec::new(),
            parent,
            children: Vec::new(),
            element: None,
        });
        if let Some(p) = parent {
            self.nodes[p].children.push(index);
        }
        index
    }

    /// Attach the element a node stands for, so an editor can address the drawn box.
    pub(crate) fn push_element(
        &mut self,
        el: &Element,
        name: &str,
        bare: &str,
        parent: Option<usize>,
    ) -> usize {
        // An `@Safety { … }` annotation is anonymous; the name a reader is looking for is the type
        // it carries. Every view builds labels from the element it draws, so the substitution lives
        // here rather than in each of them.
        let typed;
        let (name, bare) = if el.kind == Kind::MetadataUsage && name == "<anonymous>" {
            typed = el.type_ref.clone().unwrap_or_else(|| "@".to_string());
            (typed.as_str(), typed.as_str())
        } else {
            (name, bare)
        };
        let index = self.push(name, bare, &el.kind, parent);
        self.nodes[index].element = Some(el.id);
        index
    }

    /// Resolve a type reference the way the reference does: last qualified segment, matched
    /// against declared names.
    fn by_name(&self, name: &str) -> Option<usize> {
        let leaf = name.rsplit("::").next().unwrap_or(name).trim();
        self.nodes.iter().position(|n| n.name == leaf)
    }
}

fn visible(el: &Element) -> bool {
    !matches!(
        el.kind,
        // `Unsupported` marks a construct the front end parsed but did not model. It exists so
        // `check` can report coverage; it is not a system element and nothing should draw it.
        Kind::Package | Kind::Satisfy | Kind::Connection | Kind::Import | Kind::Unsupported
    )
}

/// General view: one node per element, ownership and typing as edges.
pub fn general(roots: &[Element], mode: Mode) -> Graph {
    let mut g = Graph::new(View::General);
    let defs = collect_defs(roots);

    /// Relations are recorded in traversal order, not grouped by kind: the reference emits an
    /// element's ownership edge, then its typing edge, then descends, so a diff that compares
    /// sequences instead of sets only passes if this matches.
    enum Planned {
        Composite(usize, usize),
        /// The node the edge leaves, the element it was written on, and the reference text.
        Typing(usize, usize, String),
        /// A specialization clause (`part def C :> A`): the subtype's node, the element that
        /// wrote the clause, and one supertype reference. One entry per written parent, so
        /// `C :> A, B` produces two edges rather than one.
        Specializes(usize, usize, String),
    }
    let mut planned: Vec<Planned> = Vec::new();
    let mut node_of: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();

    fn walk(
        el: &Element,
        owner: Option<usize>,
        g: &mut Graph,
        planned: &mut Vec<Planned>,
        node_of: &mut std::collections::HashMap<usize, usize>,
        mode: Mode,
    ) {
        // The reference models `satisfy R by x` and `connect a.p to b.q` as anonymous usages,
        // named by a generated UUID, and never emits the relationship itself. Compat copies
        // that; extended drops the placeholder node and keeps the relationship as an edge.
        if mode == Mode::Compat {
            let placeholder = match el.kind {
                Kind::Satisfy => Some(Kind::RequirementUsage),
                Kind::Connection => Some(Kind::Connection),
                _ => None,
            };
            if let Some(kind) = placeholder {
                let tag = format!("{}", kind);
                let index = g.push(
                    &anon_id(&tag, el.line),
                    &anon_id(&tag, el.line),
                    &kind,
                    owner,
                );
                if let Some(parent) = owner {
                    planned.push(Planned::Composite(parent, index));
                }
                return;
            }
        }
        if !visible(el) {
            // A package contributes its members to the enclosing namespace: flattened, not drawn.
            //
            // A relationship's anonymous ends are not members: `connect a.p to b.q` projects two
            // ends that exist so a rule can bound them, and drawing them put two nameless boxes
            // beside a connector that is drawn as an edge. An end the model named
            // (`connect [1] cd ::> b.c.d`) is a declaration and still drawn.
            let relationship = el.is_relationship();
            for child in &el.children {
                if relationship && child.name == "<anonymous>" {
                    continue;
                }
                walk(child, owner, g, planned, node_of, mode);
            }
            return;
        }
        let index = g.push_element(el, &el.name, &el.name, owner);
        node_of.insert(el.id, index);
        // The reference's rectangle carries the stereotype and the name and nothing else, so a
        // compat box has to stay empty. Extended puts the predicate inside, which is what the
        // standard notation calls for.
        if mode == Mode::Extended && el.kind == Kind::RequirementDef {
            g.nodes[index].constraint = el.constraint.clone();
        }
        if let Some(parent) = owner {
            planned.push(Planned::Composite(parent, index));
        }
        if let Some(type_ref) = &el.type_ref {
            planned.push(Planned::Typing(index, el.id, type_ref.clone()));
        }
        // A specialization clause is a relation the model states, so extended draws it. Compat
        // does not: the reference implementation's general view emits typing only, and parity
        // is measured edge-for-edge against it.
        if mode == Mode::Extended {
            for sup in el.supertype_list() {
                planned.push(Planned::Specializes(index, el.id, sup.to_string()));
            }
        }
        for child in &el.children {
            // The reference never lowers a port definition's body, so `port def FuelOutlet`
            // draws as an empty box. Extended shows the directed items.
            if mode == Mode::Compat && el.kind == Kind::PortDef {
                break;
            }
            // Inherited ports are extended's mechanism for resolving connector endpoints.
            if mode == Mode::Compat && child.inherited {
                continue;
            }
            // A `subject` is a declared name a predicate is written against, not a feature of
            // the requirement. The reference implementation does not draw one, so compat must
            // not either. A `frame` has the same shape — a declared concern reference rather
            // than a feature of the requirement's structure — and it shared `Kind::Subject`
            // until the two memberships were split, so compat skipped it then too.
            if mode == Mode::Compat && matches!(child.kind, Kind::Subject | Kind::Frame) {
                continue;
            }
            walk(child, Some(index), g, planned, node_of, mode);
        }
    }

    for root in roots {
        walk(root, None, &mut g, &mut planned, &mut node_of, mode);
    }

    for step in planned {
        match step {
            Planned::Composite(from, to) => g.edges.push(Edge {
                from,
                to,
                arrow: Arrow::Composite,
                label: None,
                from_port: None,
                to_port: None,
            }),
            Planned::Typing(from, element, type_ref) => {
                // Compat keeps the reference's leaf-name match on purpose: sysmlpy takes the
                // first element whose last name segment matches, so `Cart::wheel : Wheel` binds
                // to `Left::Wheel` when two packages declare `Wheel`. Extended resolves from the
                // scope the reference was written in, which is the correct answer.
                let target = match mode {
                    Mode::Compat => g.by_name(&type_ref),
                    Mode::Extended => defs
                        .0
                        .definition_of(element, &type_ref)
                        .and_then(|id| node_of.get(&id).copied()),
                };
                let Some(to) = target else {
                    continue;
                };
                if to == from {
                    continue;
                }
                g.edges.push(Edge {
                    from,
                    to,
                    arrow: Arrow::Typing,
                    label: Some("types".to_string()),
                    from_port: None,
                    to_port: None,
                });
            }
            Planned::Specializes(from, element, sup) => {
                // Resolved from the scope the clause was written in, like a typing. Unlike a
                // typing, the target may be a *usage* — `part other redefines base_p` makes
                // `other` a kind of the feature `base_p`, not of a definition.
                let resolved = defs.0.resolve(element, &sup).found();
                let target = resolved.and_then(|id| node_of.get(&id).copied());
                let Some(to) = target else {
                    continue;
                };
                if to == from {
                    continue;
                }
                g.edges.push(Edge {
                    from,
                    to,
                    arrow: Arrow::Typing,
                    label: Some("specializes".to_string()),
                    from_port: None,
                    to_port: None,
                });
            }
        }
    }

    // Extended only. The reference general view never emits `satisfy` — the extractor at
    // plantuml.py:2889 is wired to the traceability matrix alone — so drawing it is an addition.
    if mode == Mode::Extended {
        fn satisfy(el: &Element, out: &mut Vec<(usize, String, String)>) {
            if el.kind == Kind::Satisfy {
                if let (Some(from), Some(to)) = (&el.from, &el.to) {
                    out.push((el.id, from.clone(), to.clone()));
                }
            }
            for child in &el.children {
                satisfy(child, out);
            }
        }
        let mut pairs = Vec::new();
        for root in roots {
            satisfy(root, &mut pairs);
        }
        for (scope, from, to) in pairs {
            // Both ends are resolved from the scope the `satisfy` statement was written in, so a
            // name that is shadowed elsewhere still lands on the right element. An end may also
            // be a dotted feature path (`vehicle1_c1.engine_v1`), which is not a qualified name:
            // each segment is a feature of the previous one.
            let resolve = |name: &str| -> Option<usize> {
                if let Some(id) = defs.0.resolve(scope, name).found() {
                    return node_of.get(&id).copied();
                }
                // A dotted end (`vehicle1_c1.engine_v1`) is a chain of features, and a usage
                // inherits its type's features rather than declaring them, so the walk has to
                // consult the typing chain at every step — walking only the declared children
                // stops at the first inherited part.
                let mut segments = name.split('.').map(str::trim);
                let head = segments.next()?;
                let mut current = defs.0.resolve(scope, head).found()?;
                for segment in segments {
                    let element = defs.1.get(current)?;
                    let next = element
                        .children
                        .iter()
                        .find(|c| c.name == segment)
                        .map(|c| c.id)
                        .or_else(|| {
                            // The feature may come from the type instead.
                            let def = defs.0.resolve(element.id, segment).found()?;
                            defs.1.get(def).map(|d| d.id)
                        })?;
                    current = next;
                }
                node_of.get(&current).copied()
            };
            if let (Some(a), Some(b)) = (resolve(&from), resolve(&to)) {
                g.edges.push(Edge {
                    from: a,
                    to: b,
                    arrow: Arrow::Satisfy,
                    label: Some("«satisfy»".to_string()),
                    from_port: None,
                    to_port: None,
                });
            }
        }
    }

    g
}

type Defs<'a> = (sysml_resolve::Index, Vec<&'a Element>);

fn collect_defs<'a>(roots: &'a [Element]) -> Defs<'a> {
    fn walk<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
        out.push(el);
        for child in &el.children {
            walk(child, out);
        }
    }
    let mut flat = Vec::new();
    for root in roots {
        walk(root, &mut flat);
    }
    // `load` numbers elements in this same pre-order, so `flat[id]` is the element with that id.
    debug_assert!(flat.iter().enumerate().all(|(i, el)| el.id == i));
    (sysml_resolve::Index::build(roots), flat)
}

/// The definition a usage is typed by, resolved from the scope the usage sits in.
fn definition_of<'a>(defs: &Defs<'a>, el: &Element) -> Option<&'a Element> {
    let name = el.type_ref.as_ref()?;
    let id = defs.0.definition_of(el.id, name)?;
    defs.1.get(id).copied()
}

/// Short form of a type reference for display: `ISQ::MassValue` reads as `MassValue` in a box,
/// the way the reference writes it (plantuml.py:1531-1534). Resolution always uses the whole
/// reference; only the rendering trims it.
fn display_type(text: &str) -> &str {
    text.rsplit("::").next().unwrap_or(text).trim()
}

/// `tank : Tank` when the usage is typed, the bare name otherwise.
fn label_of(el: &Element) -> String {
    match el.type_ref.as_deref() {
        Some(t) if !t.is_empty() => format!("{} : {}", el.name, display_type(t)),
        _ => el.name.clone(),
    }
}

/// Part usages declared in the element's own body, or — when the body has none — in the body of
/// the definition it is typed by. The second case is what makes `part vehicle : Vehicle` show
/// the `tank` and `engine` declared inside `part def Vehicle`.
fn nested_parts<'a>(el: &'a Element, defs: &Defs<'a>) -> Vec<&'a Element> {
    let own: Vec<&Element> = el
        .children
        .iter()
        .filter(|c| c.kind == Kind::PartUsage)
        .collect();
    if !own.is_empty() {
        return own;
    }
    match definition_of(defs, el) {
        Some(def) => def
            .children
            .iter()
            .filter(|c| c.kind == Kind::PartUsage)
            .collect(),
        None => own,
    }
}

/// Attributes of a usage: its own, then those of its type, deduplicated by name
/// (boxes_view.py:1310-1336).
fn attributes_of(el: &Element, defs: &Defs) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let chain = std::iter::once(el).chain(definition_of(defs, el));
    for source in chain {
        for child in &source.children {
            if child.kind != Kind::AttributeUsage || seen.contains(&child.name) {
                continue;
            }
            seen.push(child.name.clone());
            out.push(match &child.type_ref {
                Some(t) => format!("+ {} : {}", child.name, display_type(t)),
                None => format!("+ {}", child.name),
            });
        }
    }
    out
}

/// Ports of a usage: its own (already inherited onto the usage by the syntax layer) plus the
/// type's, deduplicated. The side follows the direction written on the port definition's items.
fn ports_of(el: &Element, defs: &Defs) -> Vec<Port> {
    let mut out: Vec<Port> = Vec::new();
    let chain = std::iter::once(el).chain(definition_of(defs, el));
    for source in chain {
        for child in &source.children {
            if child.kind != Kind::PortUsage || out.iter().any(|p| p.name == child.name) {
                continue;
            }
            // The port's own direction first: `out port supply : PowerPort` states it, and that
            // is the side the connector leaves from. Reading the direction off the port's *type*
            // instead made every port take its payload's direction — `Battery::supply` is `out`
            // and was drawn as `in` because `PowerPort` declares `in power : Real`.
            let direction = child.direction.or_else(|| {
                child
                    .type_ref
                    .as_ref()
                    .and_then(|t| defs.0.definition_of(child.id, t))
                    .and_then(|id| defs.1.get(id).copied())
                    .and_then(|d| d.children.iter().find_map(|f| f.direction))
            });
            out.push(Port {
                name: child.name.clone(),
                direction,
                side: match direction {
                    Some(Direction::In) => Side::Left,
                    _ => Side::Right,
                },
            });
        }
    }
    out
}

fn connections_of<'a>(el: &'a Element, defs: &Defs<'a>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chain = std::iter::once(el).chain(definition_of(defs, el));
    for source in chain {
        for child in &source.children {
            // An `interface` usage is a connector too: `interface :Link connect a.out1 to b.in1;`
            // keeps its ends on the usage itself. The clause used to fall out as a *separate*
            // `Kind::Connection` beside it, which this loop found; reading the declaration's own
            // ends means the interface has to be named here as well.
            if matches!(
                child.kind,
                Kind::Connection | Kind::InterfaceDef | Kind::InterfaceUsage
            ) {
                if let (Some(from), Some(to)) = (&child.from, &child.to) {
                    out.push((from.clone(), to.clone()));
                }
            }
        }
    }
    out
}

/// Interconnection view: part usages that a connection reaches, nested, with one edge per
/// connection. Definitions are not drawn — the usage label carries the type.
///
/// In compat mode only the top-level part usages are emitted, flat, with no attributes, ports or
/// connectors. That is what the reference produces: its inclusion set is built from usage bodies
/// (boxes_view.py:1273-1287), so a `connect` written inside `part def Vehicle` reaches no drawn
/// node and the whole diagram collapses to a single box.
pub fn interconnection(roots: &[Element], mode: Mode) -> Graph {
    let defs = collect_defs(roots);

    // Top-level part usages: those whose owner is not itself a part usage.
    let mut tops: Vec<&Element> = Vec::new();
    fn collect_tops<'a>(el: &'a Element, nested: bool, out: &mut Vec<&'a Element>) {
        if el.kind == Kind::PartUsage {
            if !nested {
                out.push(el);
            }
            for child in &el.children {
                collect_tops(child, true, out);
            }
            return;
        }
        if el.kind.is_definition() {
            return;
        }
        for child in &el.children {
            collect_tops(child, nested, out);
        }
    }
    for root in roots {
        collect_tops(root, false, &mut tops);
    }

    let mut g = Graph::new(View::Interconnection);

    if mode == Mode::Compat {
        for top in &tops {
            g.push_element(top, &label_of(top), &top.name, None);
        }
        return g;
    }

    let mut pending: Vec<(usize, String, String)> = Vec::new();

    struct Builder<'a> {
        defs: &'a Defs<'a>,
    }

    impl<'a> Builder<'a> {
        fn build(
            &self,
            el: &'a Element,
            parent: Option<usize>,
            g: &mut Graph,
            pending: &mut Vec<(usize, String, String)>,
        ) -> usize {
            // The box label carries the type the way the reference writes it
            // (`vehicle : Vehicle`, plantuml.py:1531-1534); the bare name is the path segment
            // that connection endpoints are resolved against.
            let index = g.push_element(el, &label_of(el), &el.name, parent);
            g.nodes[index].attributes = attributes_of(el, self.defs);
            g.nodes[index].ports = ports_of(el, self.defs);

            // A connection endpoint is resolved relative to the container the `connect` was
            // written in, so the container index travels with the endpoint text.
            for (from, to) in connections_of(el, self.defs) {
                pending.push((index, from, to));
            }

            for child in nested_parts(el, self.defs) {
                self.build(child, Some(index), g, pending);
            }
            index
        }
    }

    // Every connection written outside a part usage. One written *inside* a part usage is found by
    // `connections_of` as the builder draws that part; collecting it here too would draw it twice.
    let mut global: Vec<(String, String)> = Vec::new();
    fn collect_connectors(el: &Element, out: &mut Vec<(String, String)>) {
        for child in &el.children {
            if matches!(
                child.kind,
                Kind::Connection | Kind::InterfaceDef | Kind::InterfaceUsage
            ) && el.kind != Kind::PartUsage
            {
                if let (Some(from), Some(to)) = (&child.from, &child.to) {
                    out.push((from.clone(), to.clone()));
                }
            }
            collect_connectors(child, out);
        }
    }
    for root in roots {
        collect_connectors(root, &mut global);
    }
    let builder = Builder { defs: &defs };
    for top in tops {
        // Every top-level part usage is drawn, as the reference draws it. A usage whose
        // structure carries connections is additionally expanded into its nested parts.
        builder.build(top, None, &mut g, &mut pending);
    }

    // A connection written in a namespace resolves its ends against every box that namespace
    // holds, not against one part usage. The first segment is looked up among all nodes; a
    // connection from a part usage was already handled by `connections_of` above.
    for (from, to) in global {
        let resolve_global = |end: &str| -> Option<(usize, Option<String>)> {
            let parts: Vec<&str> = end
                .split('.')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            let first = parts.first()?;
            for start in 0..g.nodes.len() {
                if g.nodes[start].bare == *first {
                    if let Some(found) = walk_path(&g, start, &parts[1..]) {
                        return Some(found);
                    }
                    if parts.len() == 1 {
                        return Some((start, None));
                    }
                }
            }
            None
        };
        let (Some((from_node, from_name)), Some((to_node, to_name))) =
            (resolve_global(&from), resolve_global(&to))
        else {
            continue;
        };
        if from_node == to_node {
            continue;
        }
        // A port name becomes the port's index on the node, the same conversion the connections
        // collected from a part usage go through.
        let port = |node: usize, name: &Option<String>| {
            name.as_ref()
                .and_then(|n| g.nodes[node].ports.iter().position(|p| &p.name == n))
        };
        g.edges.push(Edge {
            from: from_node,
            to: to_node,
            arrow: Arrow::Connector,
            label: None,
            from_port: port(from_node, &from_name),
            to_port: port(to_node, &to_name),
        });
    }

    for (scope, from, to) in pending {
        // Each end is a dotted path: `name`, `part.port` or `a.b.port`. The last segment names
        // the port when the preceding segments resolve to a node.
        let resolve = |end: &str| -> Option<(usize, Option<String>)> {
            let parts: Vec<&str> = end
                .split('.')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if parts.is_empty() {
                return None;
            }
            // Walk out from the connection's container: the first segment may name a sibling,
            // anything shorter only resolves from a location the path actually starts at.
            let mut here = Some(scope);
            while let Some(node) = here {
                if let Some(found) = walk_path(&g, node, &parts) {
                    return Some(found);
                }
                here = g.nodes[node].parent;
            }
            None
        };
        let (Some((from, pa)), Some((to, pb))) = (resolve(&from), resolve(&to)) else {
            continue;
        };
        let port = |node: usize, name: &Option<String>| {
            name.as_ref()
                .and_then(|n| g.nodes[node].ports.iter().position(|p| &p.name == n))
        };
        let (from_port, to_port) = (port(from, &pa), port(to, &pb));
        g.edges.push(Edge {
            from,
            to,
            arrow: Arrow::Connector,
            label: None,
            from_port,
            to_port,
        });
    }

    // A connection written in a part definition is reached twice: once by `connections_of` as the
    // builder expands a usage of that definition, and once by the global sweep. Drawing both put
    // two identical lines on the picture.
    let mut seen = std::collections::HashSet::new();
    g.edges.retain(|e| {
        seen.insert((
            e.from,
            e.to,
            e.arrow as u8,
            e.label.clone(),
            e.from_port,
            e.to_port,
        ))
    });
    g
}
fn walk_path(g: &Graph, start: usize, parts: &[&str]) -> Option<(usize, Option<String>)> {
    let mut node = start;
    for (i, segment) in parts.iter().enumerate() {
        let child = g.nodes[node]
            .children
            .iter()
            .copied()
            .find(|&c| g.nodes[c].bare == *segment);
        match child {
            Some(c) => node = c,
            // No child by that name: the rest has to be the port, and only the last segment
            // can be one.
            None if i + 1 == parts.len() => {
                return Some((node, Some((*segment).to_string())));
            }
            None => return None,
        }
    }
    Some((node, None))
}

/// Canonical text form used by the parity check: one line per node and per edge, aliases
/// included so ordering differences show up instead of being normalised away.
pub fn to_lines(g: &Graph) -> Vec<String> {
    let mut out = Vec::new();
    for node in &g.nodes {
        out.push(format!(
            "node {} \"{}\" <<{}>>{}",
            node.alias,
            node.label(),
            node.stereotype,
            if node.is_def { " def" } else { "" }
        ));
        for attr in &node.attributes {
            out.push(format!("  attr {}", attr));
        }
        for port in &node.ports {
            out.push(format!(
                "  port {} {}",
                port.name,
                match port.direction {
                    Some(d) => d.keyword(),
                    None => "-",
                }
            ));
        }
    }
    for edge in &g.edges {
        let from = &g.nodes[edge.from].alias;
        let to = &g.nodes[edge.to].alias;
        let via = match (edge.from_port, edge.to_port) {
            (Some(a), Some(b)) => format!(
                " :{} :{}",
                g.nodes[edge.from].ports[a].name, g.nodes[edge.to].ports[b].name
            ),
            _ => String::new(),
        };
        match &edge.label {
            Some(label) => out.push(format!(
                "edge {} {} {} : {label}{via}",
                from,
                edge.arrow.notation(),
                to
            )),
            None => out.push(format!(
                "edge {} {} {}{via}",
                from,
                edge.arrow.notation(),
                to
            )),
        }
    }
    out
}
