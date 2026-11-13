//! The requirement traceability view: which requirement is met by what, and which is met by
//! nothing.
//!
//! The general view draws a `satisfy` edge when one was written, but it draws every element at
//! once, so the question a reviewer actually asks — "is every requirement implemented, and by
//! which part?" — has to be assembled by eye from a picture that also contains every attribute
//! and every typing edge. This projection answers it directly: one column of requirements, one
//! column of the elements that satisfy them, and a visible mark on a requirement nothing
//! satisfies.
//!
//! # What counts as an implementer
//!
//! Two relationships, and the difference between them is kept rather than smoothed over:
//!
//! * `satisfy R by x` — the model states it. This is the strong one and it becomes an edge.
//! * a requirement's `subject` type — the thing the requirement is *about*, which is what the
//!   constraint is written over. A requirement with a subject but no `satisfy` is not
//!   unimplemented; it is unimplemented *by anything that says so*. The subject is recorded on
//!   the trace but does not draw an edge, because drawing one would claim a satisfaction the
//!   model never stated.
//!
//! A requirement whose constraint names a feature of a part is *not* linked to that part here.
//! The constraint text is quoted on the requirement's own row instead: inferring an implementer
//! from a name inside an expression would invent a relationship, and a traceability view that
//! invents links is worse than none.

use std::collections::BTreeMap;

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use crate::graph::{bare_end, Arrow, Edge, Graph, View};

/// One requirement and what is known about it.
pub struct Trace {
    /// Index into `Graph::nodes`.
    pub node: usize,
    /// Indexes of the nodes that state they satisfy it, in source order.
    pub satisfied_by: Vec<usize>,
    /// The type named by the requirement's `subject`, if it has one. Recorded, not drawn.
    pub subject: Option<usize>,
    /// The constraint text, quoted so a reader can see what has to hold.
    pub predicate: Option<String>,
}

impl Trace {
    /// True when the model states no satisfier for this requirement.
    ///
    /// This is the fact the view exists to show. A requirement with a subject but no `satisfy`
    /// counts as unsatisfied: the subject says what it is about, not that anything meets it.
    pub fn is_unmet(&self) -> bool {
        self.satisfied_by.is_empty()
    }
}

pub struct Traceability {
    pub graph: Graph,
    /// One entry per requirement node, in the order they appear in the model.
    pub traces: Vec<Trace>,
}

/// Every derivation the model states, as `(original, derived)` bare names.
///
/// `connection d : Derivation connect C to R;` parses into two elements on one line: the typed
/// connection and a bare `connect` carrying the ends. The ends are joined to the type by line
/// number, and a connection carrying both counts too, so that fixing the parse later does not
/// silently switch this off.
///
/// Names are returned as written rather than resolved, because both callers — this view and the
/// ladder in the CLI — need the pairing, not the ids, and the resolver would have to be threaded
/// through the view's own index. A name that two elements share is reported once; the corpus does
/// not do that, and the error's direction is to claim less coverage rather than more.
pub fn derivations(roots: &[Element]) -> Vec<(String, String)> {
    let mut lines: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut ends: Vec<(u32, String, String)> = Vec::new();
    fn walk(
        el: &Element,
        lines: &mut std::collections::HashSet<u32>,
        ends: &mut Vec<(u32, String, String)>,
    ) {
        if el
            .type_ref
            .as_deref()
            .is_some_and(|text| text.rsplit("::").next() == Some("Derivation"))
        {
            lines.insert(el.line);
        }
        if let (Some(from), Some(to)) = (&el.from, &el.to) {
            ends.push((el.line, from.clone(), to.clone()));
        }
        for child in &el.children {
            walk(child, lines, ends);
        }
    }
    for root in roots {
        walk(root, &mut lines, &mut ends);
    }
    ends.into_iter()
        .filter(|(line, _, _)| lines.contains(line))
        .map(|(_, from, to)| (bare_end(&from).to_string(), bare_end(&to).to_string()))
        .collect()
}

/// Requirements on the left, implementers on the right.
pub fn traceability(roots: &[Element]) -> Traceability {
    let mut g = Graph::new(View::Traceability);
    let index = Index::build(roots);
    let by_id = elements_by_id(roots);

    // Every requirement, in model order. Definitions and usages are both drawn: a definition
    // states the requirement once, a usage applies it, and a reviewer needs to see which one is
    // unsatisfied.
    let mut requirements: Vec<&Element> = Vec::new();
    fn collect_requirements<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
        if matches!(el.kind, Kind::RequirementDef | Kind::RequirementUsage) {
            out.push(el);
        }
        for child in &el.children {
            collect_requirements(child, out);
        }
    }
    for root in roots {
        collect_requirements(root, &mut requirements);
    }

    let mut req_node: BTreeMap<usize, usize> = BTreeMap::new();
    for requirement in &requirements {
        let index = g.push_element(requirement, &requirement.name, &requirement.name, None);
        g.nodes[index].constraint = requirement
            .predicate
            .as_ref()
            .map(|p| p.render())
            .or_else(|| requirement.constraint.clone());
        req_node.insert(requirement.id, index);
    }

    let mut implementer_node: BTreeMap<usize, usize> = BTreeMap::new();
    let add_implementer =
        |g: &mut Graph, id: usize, map: &mut BTreeMap<usize, usize>| -> Option<usize> {
            if let Some(existing) = map.get(&id) {
                return Some(*existing);
            }
            let el = by_id.get(&id)?;
            let node = g.push_element(el, &el.name, &el.name, None);
            map.insert(id, node);
            Some(node)
        };

    // The subject of each requirement, resolved from the requirement's own scope.
    let mut subjects: BTreeMap<usize, Option<usize>> = BTreeMap::new();
    for requirement in &requirements {
        let mut subject = None;
        for child in &requirement.children {
            if child.kind == Kind::Subject {
                if let Some(text) = child.type_ref.as_deref() {
                    if let Some(id) = index.resolve(requirement.id, text).found() {
                        subject = add_implementer(&mut g, id, &mut implementer_node);
                        break;
                    }
                }
            }
        }
        subjects.insert(requirement.id, subject);
    }

    // Satisfy statements, resolved from the scope they were written in, exactly as the general
    // view does it, so a shadowed name still lands on the right element.
    let mut pairs: Vec<(usize, String, String)> = Vec::new();
    fn collect_satisfies(el: &Element, out: &mut Vec<(usize, String, String)>) {
        if el.kind == Kind::Satisfy {
            if let (Some(from), Some(to)) = (&el.from, &el.to) {
                out.push((el.id, from.clone(), to.clone()));
            }
        }
        for child in &el.children {
            collect_satisfies(child, out);
        }
    }
    for root in roots {
        collect_satisfies(root, &mut pairs);
    }

    let mut satisfied: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (scope, from, to) in pairs {
        // `satisfy R by x` writes the requirement first and the implementer second. The columns
        // are fixed here, so the edge always runs requirement -> implementer regardless of which
        // side the statement named first.
        let Some(req) = index.resolve(scope, &to).found() else {
            continue;
        };
        let Some(impl_) = index.resolve(scope, &from).found() else {
            continue;
        };
        if !req_node.contains_key(&req) {
            // `satisfy` naming something that is not a requirement: legal only if that thing is
            // a requirement, so a miss here means the model is wrong and `check` reports it.
            continue;
        }
        let Some(impl_node) = add_implementer(&mut g, impl_, &mut implementer_node) else {
            continue;
        };
        let list = satisfied.entry(req).or_default();
        if !list.contains(&impl_node) {
            list.push(impl_node);
        }
    }

    // Annotations as edges. `@Safety { level = 3 } part def Brake;` says Brake carries a safety
    // annotation of level 3, which is what a traceability view is for: what relates to what, and
    // what was said about it. Without these the view showed what satisfies what and nothing at all
    // about the elements themselves.
    let mut annotations: Vec<(usize, usize, String)> = Vec::new();
    fn collect_annotations(el: &Element, out: &mut Vec<(usize, usize, String)>) {
        for child in &el.children {
            if child.kind != Kind::MetadataUsage {
                continue;
            }
            let Some(type_ref) = child.type_ref.as_deref() else {
                continue;
            };
            let arguments: Vec<String> = child
                .children
                .iter()
                .filter_map(|argument| {
                    argument
                        .value_expr
                        .as_ref()
                        .map(|value| format!("{} = {}", argument.name, value.render()))
                })
                .collect();
            let label = if arguments.is_empty() {
                format!("@{type_ref}")
            } else {
                format!("@{type_ref} {{{}}}", arguments.join(", "))
            };
            out.push((el.id, child.id, label));
        }
        for child in &el.children {
            collect_annotations(child, out);
        }
    }
    for root in roots {
        collect_annotations(root, &mut annotations);
    }

    let mut annotated_node: BTreeMap<usize, usize> = BTreeMap::new();
    let mut metadata_node: BTreeMap<usize, usize> = BTreeMap::new();
    for (subject, annotation, label) in annotations {
        let Some(subject_node) = add_implementer(&mut g, subject, &mut annotated_node) else {
            continue;
        };
        let Some(annotation_element) = by_id.get(&annotation).copied() else {
            continue;
        };
        let written = annotation_element
            .type_ref
            .clone()
            .unwrap_or_else(|| annotation_element.name.clone());
        // The far end is the metadata definition when the model has one, so two parts annotated by
        // the same type meet at one node. A type that does not resolve is drawn from the annotation
        // itself under the name it wrote — `check` reports the unresolved name, and the view still
        // shows what the model said.
        let definition = annotation_element
            .type_ref
            .as_deref()
            .and_then(|text| index.resolve(annotation, text).found())
            .and_then(|id| by_id.get(&id).copied());
        let far = match definition {
            Some(definition) => {
                let id = definition.id;
                let element = definition;
                *metadata_node
                    .entry(id)
                    .or_insert_with(|| g.push_element(element, &element.name, &element.name, None))
            }
            None => g.push_element(&annotation_element, &written, &written, None),
        };
        g.edges.push(Edge {
            from: subject_node,
            to: far,
            arrow: Arrow::Annotate,
            label: Some(label),
            from_port: None,
            to_port: None,
        });
    }

    let mut traces = Vec::new();
    for requirement in &requirements {
        let Some(&node) = req_node.get(&requirement.id) else {
            continue;
        };
        let satisfied_by = satisfied.get(&requirement.id).cloned().unwrap_or_default();
        for &impl_node in &satisfied_by {
            g.edges.push(Edge {
                from: node,
                to: impl_node,
                arrow: Arrow::Satisfy,
                label: None,
                from_port: None,
                to_port: None,
            });
        }
        traces.push(Trace {
            node,
            satisfied_by,
            subject: subjects.get(&requirement.id).copied().flatten(),
            predicate: requirement
                .predicate
                .as_ref()
                .map(|p| p.render())
                .or_else(|| requirement.constraint.clone()),
        });
    }

    Traceability { graph: g, traces }
}

/// Every element by id, so a resolved id can be turned back into the element it names.
fn elements_by_id(roots: &[Element]) -> BTreeMap<usize, &Element> {
    let mut out = BTreeMap::new();
    fn go<'a>(el: &'a Element, out: &mut BTreeMap<usize, &'a Element>) {
        out.insert(el.id, el);
        for child in &el.children {
            go(child, out);
        }
    }
    for root in roots {
        go(root, &mut out);
    }
    out
}
