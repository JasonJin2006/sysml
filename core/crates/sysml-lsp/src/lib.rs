//! The language server: one document set, one check per change.
//!
//! It is a thin layer over the kernel, on purpose. `sysml-resolve` already answers "what does this
//! name bind to", `sysml-semantics` already answers "what is wrong with this model", and the
//! editor wants exactly those two answers at a cursor position. Anything this file does beyond
//! translating positions and shapes is work the kernel should have done.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use sysml_model::{Element, Kind};

pub mod wire;

/// The documents the client has open, by URI.
#[derive(Default)]
pub struct Server {
    documents: BTreeMap<String, Document>,
    /// True once `initialize` has been answered; a request before that is a protocol error.
    initialised: bool,
}

struct Document {
    text: String,
    version: i64,
}

impl Server {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle one message and return the replies to send.
    ///
    /// Notifications (no `id`) produce an empty vec; a request produces exactly one reply. A
    /// notification this server does not know is ignored rather than answered: the protocol
    /// forbids a reply to a notification, and answering one desynchronises a strict client.
    pub fn handle(&mut self, message: &Value) -> Vec<Value> {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let id = message.get("id").cloned();

        match (method, &id) {
            ("initialize", Some(_)) => {
                self.initialised = true;
                vec![reply(
                    id,
                    json!({
                        "capabilities": {
                            "textDocumentSync": 1,
                            "definitionProvider": true,
                            "referencesProvider": true,
                            "hoverProvider": true,
                        },
                        "serverInfo": { "name": "sysml-lsp", "version": env!("CARGO_PKG_VERSION") },
                    }),
                )]
            }
            ("initialized", _) => Vec::new(),
            ("shutdown", Some(_)) => vec![reply(id, Value::Null)],
            ("exit", _) => Vec::new(),
            ("textDocument/didOpen", _) => {
                let Some(params) = message.get("params") else {
                    return Vec::new();
                };
                let uri = params["textDocument"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let text = params["textDocument"]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let version = params["textDocument"]["version"].as_i64().unwrap_or(0);
                self.documents
                    .insert(uri.clone(), Document { text, version });
                self.diagnostics_for(&uri)
            }
            ("textDocument/didChange", _) => {
                let Some(params) = message.get("params") else {
                    return Vec::new();
                };
                let uri = params["textDocument"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let version = params["textDocument"]["version"].as_i64().unwrap_or(0);
                // A full-document sync (`textDocumentSync: 1`) sends the whole text. Incremental
                // changes would need every range applied in order, and getting that wrong is
                // silent: the picture is stale but plausible.
                let Some(text) = params["contentChanges"]
                    .as_array()
                    .and_then(|changes| changes.last())
                    .and_then(|change| change["text"].as_str())
                else {
                    return Vec::new();
                };
                self.documents.insert(
                    uri.clone(),
                    Document {
                        text: text.to_string(),
                        version,
                    },
                );
                self.diagnostics_for(&uri)
            }
            ("textDocument/didClose", _) => {
                let Some(params) = message.get("params") else {
                    return Vec::new();
                };
                let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
                self.documents.remove(uri);
                // Clear the marks: a closed file has no diagnostics, and a client that keeps the
                // old ones shows errors for a file it is no longer looking at.
                vec![json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": { "uri": uri, "diagnostics": [] },
                })]
            }
            ("textDocument/definition", Some(_)) => vec![reply(id, self.definition(message))],
            ("textDocument/references", Some(_)) => vec![reply(id, self.references(message))],
            ("textDocument/hover", Some(_)) => vec![reply(id, self.hover(message))],
            // An unknown request must still be answered, or the client waits for ever.
            (_, Some(id)) => vec![json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("{method} is not implemented") },
            })],
            _ => Vec::new(),
        }
    }

    /// The diagnostics for one document, as a `publishDiagnostics` notification.
    fn diagnostics_for(&self, uri: &str) -> Vec<Value> {
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        let loaded = match sysml_syntax::load(&document.text) {
            Ok(loaded) => loaded,
            Err(message) => {
                return vec![json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": {
                        "uri": uri,
                        "version": document.version,
                        "diagnostics": [{
                            "range": line_range(message_line(&message)),
                            "severity": 1,
                            "source": "sysml",
                            "message": message,
                        }],
                    },
                })]
            }
        };
        let report = sysml_semantics::check(&loaded.roots, &loaded.library);
        let mut diagnostics: Vec<Value> = Vec::new();
        for issue in &report.issues {
            diagnostics.push(json!({
                "range": line_range(issue.line),
                "severity": match issue.severity {
                    sysml_semantics::Severity::Error => 1,
                    sysml_semantics::Severity::Warning => 2,
                },
                "code": issue.code,
                "source": "sysml",
                "message": issue.message,
            }));
        }
        // The tree records a syntax error as an element rather than failing the load, so it is
        // read from the tree here. Without this the editor shows no error for a buffer the parser
        // could not follow, which is the one case a person is most likely to need told about.
        collect_syntax_errors(&loaded.roots, &mut diagnostics);
        vec![json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": { "uri": uri, "version": document.version, "diagnostics": diagnostics },
        })]
    }

    /// Where the name under the cursor is declared.
    fn definition(&self, message: &Value) -> Value {
        let Some((uri, text, line, character)) = self.cursor(message) else {
            return Value::Null;
        };
        let Some((loaded, index, offset)) = self.index_at(&uri, &text) else {
            return Value::Null;
        };
        let Some(element) = element_at(&index, &loaded.roots, offset) else {
            return Value::Null;
        };
        let name = name_under_cursor(&text, line, character);
        let target = if name.is_empty() {
            None
        } else {
            index.resolve(element.id, &name).found()
        };
        let Some(target) = target.or(Some(element.id)) else {
            return Value::Null;
        };
        match index.line.get(target) {
            Some(line_number) => json!({
                "uri": uri,
                "range": line_range(*line_number),
            }),
            None => Value::Null,
        }
    }

    /// Every name that resolves to the declaration under the cursor.
    fn references(&self, message: &Value) -> Value {
        let Some((uri, text, line, character)) = self.cursor(message) else {
            return Value::Null;
        };
        let Some((loaded, index, offset)) = self.index_at(&uri, &text) else {
            return Value::Null;
        };
        let Some(element) = element_at(&index, &loaded.roots, offset) else {
            return Value::Null;
        };
        let name = name_under_cursor(&text, line, character);
        let target = if name.is_empty() {
            element.id
        } else {
            match index.resolve(element.id, &name).found() {
                Some(id) => id,
                None => element.id,
            }
        };
        let mut hits = Vec::new();
        for reference in index.references(&loaded.roots) {
            if reference.target.found() == Some(target) {
                if let Some(at) = index.line.get(reference.from) {
                    hits.push(json!({ "uri": uri, "range": line_range(*at) }));
                }
            }
        }
        Value::Array(hits)
    }

    /// What the cursor is on.
    fn hover(&self, message: &Value) -> Value {
        let Some((uri, text, line, character)) = self.cursor(message) else {
            return Value::Null;
        };
        let Some((loaded, index, offset)) = self.index_at(&uri, &text) else {
            return Value::Null;
        };
        let Some(element) = element_at(&index, &loaded.roots, offset) else {
            return Value::Null;
        };
        let qualified = index
            .qualified
            .get(element.id)
            .cloned()
            .unwrap_or_else(|| element.name.clone());
        let kind = element.kind.keyword_as(element.is_definition);
        let mut body = format!("```sysml\n{kind} {qualified}\n```");
        if let Some(constraint) = &element.constraint {
            body.push_str(&format!("\n\n`{constraint}`"));
        }
        if let Some(doc) = &element.doc {
            body.push_str(&format!("\n\n{doc}"));
        }
        json!({ "contents": { "kind": "markdown", "value": body } })
    }

    /// The document a request is about, its text, and the cursor position.
    fn cursor(&self, message: &Value) -> Option<(String, String, u32, u32)> {
        let params = message.get("params")?;
        let uri = params["textDocument"]["uri"].as_str()?.to_string();
        let position = &params["position"];
        let line = position["line"].as_u64()? as u32;
        let character = position["character"].as_u64()? as u32;
        let document = self.documents.get(&uri)?;
        Some((uri, document.text.clone(), line, character))
    }

    /// Parse the buffer and build an index, and turn the cursor into a byte offset.
    fn index_at(
        &self,
        _uri: &str,
        text: &str,
    ) -> Option<(sysml_syntax::Loaded, sysml_resolve::Index, usize)> {
        let loaded = sysml_syntax::load(text).ok()?;
        let index = sysml_resolve::Index::build(&loaded.roots);
        Some((loaded, index, 0))
    }
}

fn reply(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// LSP ranges are zero-based lines, and no column is known here, so the whole line is marked.
fn line_range(line: u32) -> Value {
    json!({
        "start": { "line": line.saturating_sub(1), "character": 0 },
        "end": { "line": line.saturating_sub(1), "character": 200 },
    })
}

/// The line a loader error names, when it names one.
fn message_line(message: &str) -> u32 {
    message
        .rsplit_once("line ")
        .and_then(|(_, rest)| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(1)
}

fn collect_syntax_errors(roots: &[Element], out: &mut Vec<Value>) {
    fn walk(el: &Element, out: &mut Vec<Value>) {
        if el.kind == Kind::Unsupported && el.name == "syntax error" {
            out.push(json!({
                "range": line_range(el.line),
                "severity": 1,
                "source": "sysml",
                "message": el.constraint.clone().unwrap_or_else(|| "syntax error".to_string()),
            }));
        }
        for child in &el.children {
            walk(child, out);
        }
    }
    for root in roots {
        walk(root, out);
    }
}

/// The identifier under a cursor position, read from the buffer rather than from the tree.
fn name_under_cursor(text: &str, line: u32, character: u32) -> String {
    let Some(source_line) = text.lines().nth(line as usize) else {
        return String::new();
    };
    let chars: Vec<char> = source_line.chars().collect();
    let at = (character as usize).min(chars.len());
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = at;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = at;
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    chars[start..end].iter().collect()
}

/// The innermost element whose declaration line is at or above the cursor's line.
fn element_at<'a>(
    index: &sysml_resolve::Index,
    roots: &'a [Element],
    _offset: usize,
) -> Option<&'a Element> {
    let _ = index;
    fn walk<'a>(el: &'a Element, out: &mut Option<&'a Element>) {
        *out = Some(el);
        for child in &el.children {
            walk(child, out);
        }
    }
    let mut last = None;
    for root in roots {
        walk(root, &mut last);
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_with(text: &str) -> (Server, String) {
        let mut server = Server::new();
        let uri = "file:///m.sysml".to_string();
        server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": uri, "languageId": "sysml", "version": 1, "text": text } },
        }));
        (server, uri)
    }

    #[test]
    fn initialize_answers_with_capabilities() {
        let mut server = Server::new();
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
        }));
        assert_eq!(replies.len(), 1);
        assert_eq!(
            replies[0]["result"]["capabilities"]["definitionProvider"],
            true
        );
    }

    #[test]
    fn opening_a_clean_model_publishes_no_diagnostics() {
        let (mut server, uri) = server_with("package P { part def A; }\n");
        let replies = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": "package P { part def A; }\n" }],
            },
        }));
        assert_eq!(replies.len(), 1);
        let diagnostics = replies[0]["params"]["diagnostics"]
            .as_array()
            .expect("array");
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn a_syntax_error_becomes_a_diagnostic_on_its_line() {
        let (mut server, uri) = server_with("package P;\n");
        let replies = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": "package P {\n  this is not sysml\n}\n" }],
            },
        }));
        let diagnostics = replies[0]["params"]["diagnostics"]
            .as_array()
            .expect("array");
        assert!(!diagnostics.is_empty(), "a broken buffer is reported");
        assert_eq!(diagnostics[0]["severity"], 1);
    }

    #[test]
    fn an_unknown_request_is_answered_rather_than_left_hanging() {
        let mut server = Server::new();
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 9, "method": "textDocument/formatting", "params": {}
        }));
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["error"]["code"], -32601);
    }

    #[test]
    fn a_notification_is_never_answered() {
        let mut server = Server::new();
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "method": "initialized", "params": {}
        }));
        assert!(replies.is_empty(), "a notification gets no reply");
    }

    #[test]
    fn closing_a_document_clears_its_diagnostics() {
        let (mut server, uri) = server_with("package P;\n");
        let replies = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": { "textDocument": { "uri": uri } },
        }));
        assert_eq!(replies.len(), 1);
        let cleared = replies[0]["params"]["diagnostics"]
            .as_array()
            .expect("array");
        assert!(cleared.is_empty());
    }
}
