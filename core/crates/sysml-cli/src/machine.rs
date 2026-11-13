//! Machine-readable forms of the commands that only printed text.
//!
//! `check`, `prov`, `ledger` and `conformance` already answered JSON, so an agent could read
//! four of the eighteen verbs. The other five — `outline`, `resolve`, `graph`, `render`,
//! `status` — were text only, and worse, they accepted `--json` and ignored it: a caller
//! following the convention got a table meant for a person and no signal that the contract had
//! not applied.
//!
//! The shape here is chosen for the caller, not to mirror the text. Each document carries a
//! `status` field so a script reads one value instead of parsing prose, and the arrays are
//! always present even when empty, so a consumer never has to distinguish "absent" from "none".

use serde_json::{json, Value};
use sysml_model::Element;
use sysml_resolve::{Index, Resolution};

/// One element of the model tree, as an agent reads it.
pub fn outline(roots: &[Element], index: &Index) -> Value {
    fn node(el: &Element, index: &Index) -> Value {
        let id = el.id;
        let mut object = serde_json::Map::new();
        object.insert("name".into(), json!(el.name));
        object.insert(
            "qualified".into(),
            json!(index.qualified.get(id).cloned().unwrap_or_default()),
        );
        object.insert("kind".into(), json!(el.kind.keyword()));
        object.insert("line".into(), json!(el.line));
        if let Some(text) = &el.type_ref {
            // The written text, not the resolved target: this command does not resolve, and a
            // caller that wants the target asks `resolve`.
            object.insert("type".into(), json!(text));
        }
        if let Some(shape) = el.import_shape {
            object.insert(
                "import".into(),
                json!(shape.suffix().trim_start_matches(' ')),
            );
        }
        if let Some(v) = el.visibility {
            object.insert("visibility".into(), json!(v.keyword()));
        }
        if let Some(d) = &el.direction {
            object.insert("direction".into(), json!(d.keyword()));
        }
        if let Some(c) = &el.constraint {
            object.insert("constraint".into(), json!(c));
        }
        if !el.children.is_empty() {
            let children: Vec<Value> = el.children.iter().map(|c| node(c, index)).collect();
            object.insert("children".into(), Value::Array(children));
        }
        Value::Object(object)
    }
    Value::Array(roots.iter().map(|el| node(el, index)).collect())
}

/// Every written reference and what it bound to.
pub fn resolution(loaded: &sysml_syntax::Loaded, index: &Index) -> Value {
    let library_size: usize = loaded.library.iter().map(sysml_model::Element::size).sum();
    let project_end = index.len() - library_size;

    let symbols: Vec<Value> = (0..project_end)
        .map(|id| {
            json!({
                "qualified": index.qualified[id],
                "kind": index.kind[id].keyword_as(index.is_definition[id]),
                "line": index.line[id],
            })
        })
        .collect();

    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();
    let mut ambiguous = Vec::new();
    let mut not_accessible = Vec::new();
    for reference in index.references(&loaded.roots) {
        let from = index.qualified[reference.from].clone();
        match &reference.target {
            Resolution::Found(id) => resolved.push(json!({
                "from": from,
                "text": reference.text,
                "target": index.qualified[*id],
                "inPredicate": reference.in_predicate,
            })),
            Resolution::Ambiguous(ids) => ambiguous.push(json!({
                "from": from,
                "text": reference.text,
                "between": ids
                    .iter()
                    .map(|id| index.qualified[*id].clone())
                    .collect::<Vec<_>>(),
                "inPredicate": reference.in_predicate,
            })),
            Resolution::Unresolved => unresolved.push(json!({
                "from": from,
                "text": reference.text,
                "inPredicate": reference.in_predicate,
            })),
            // A separate list rather than another entry in `unresolved`: the name does resolve,
            // and what is wrong is the operator the path used to reach it.
            Resolution::NotAccessible => not_accessible.push(json!({
                "from": from,
                "text": reference.text,
                "inPredicate": reference.in_predicate,
            })),
        }
    }

    let imports: Vec<Value> = index
        .imports()
        .iter()
        .map(|entry| {
            json!({
                "owner": entry
                    .owner
                    .map(|id| index.qualified[id].clone())
                    .unwrap_or_default(),
                "visibility": entry.visibility.map(|v| v.keyword()),
                "shape": entry.shape.suffix(),
                "resolved": entry.target.found().map(|id| index.qualified[id].clone()),
                "bound": entry
                    .bound
                    .iter()
                    .map(|(name, id)| json!({"name": name, "target": index.qualified[*id]}))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    json!({
        "status": if unresolved.is_empty() && ambiguous.is_empty() && not_accessible.is_empty() {
            "pass"
        } else {
            "failed"
        },
        "symbols": symbols,
        "resolved": resolved,
        "unresolved": unresolved,
        "ambiguous": ambiguous,
        "notAccessible": not_accessible,
        "imports": imports,
    })
}

/// A projection, as nodes and edges a caller can lay out itself.
pub fn graph(projected: &sysml_view::Graph) -> Value {
    let nodes: Vec<Value> = projected
        .nodes
        .iter()
        .map(|n| {
            json!({
                "alias": n.alias,
                "name": n.name,
                "bare": n.bare,
                "kind": n.stereotype,
                "isDefinition": n.is_def,
                "attributes": n.attributes,
                "constraint": n.constraint,
                "parent": n.parent.map(|p| projected.nodes[p].alias.clone()),
                "element": n.element,
            })
        })
        .collect();
    let edges: Vec<Value> = projected
        .edges
        .iter()
        .map(|e| {
            json!({
                "from": projected.nodes[e.from].alias,
                "to": projected.nodes[e.to].alias,
                "arrow": format!("{:?}", e.arrow),
                "label": e.label,
            })
        })
        .collect();
    json!({ "nodes": nodes, "edges": edges })
}

/// The four numbers `status` prints, plus the strings a caller has to echo back.
pub fn status(summary: &str, audit: &sysml_ledger::Audit, semantic_errors: usize) -> Value {
    let status = if audit.error_count() == 0 && semantic_errors == 0 {
        "pass"
    } else if semantic_errors == 0 && audit.not_started() {
        "unfinished"
    } else {
        "failed"
    };
    json!({
        "status": status,
        "semanticErrors": semantic_errors,
        "provenanceErrors": audit.error_count(),
        "ledgerStarted": !audit.not_started(),
        "unaccountedParts": audit.unaccounted_parts,
        "blocked": audit.blocking(),
        "summary": summary,
        "issues": audit
            .issues
            .iter()
            .map(|i| json!({
                "code": i.code,
                "severity": match i.severity {
                    sysml_ledger::Severity::Error => "error",
                    sysml_ledger::Severity::Warning => "warning",
                },
                "at": i.at,
                "message": i.message,
            }))
            .collect::<Vec<_>>(),
    })
}

/// The ladder, for a caller that is not a person.
///
/// The shape lives beside the ladder itself so the canvas and the command line cannot answer the
/// same question with two different documents.
pub fn next(plan: &sysml_studio::next::Plan, facts: &sysml_studio::next::Facts) -> Value {
    sysml_studio::next::to_json(plan, facts)
}
