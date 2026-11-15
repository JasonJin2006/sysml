//! The language server: one project, one check per change.
//!
//! It is a thin layer over the kernel, on purpose. `sysml-resolve` already answers "what does this
//! name bind to", `sysml-semantics` already answers "what is wrong with this model", `sysml-ledger`
//! already answers "what has nobody accounted for", and the editor wants exactly those answers.
//! Anything this file does beyond translating positions and shapes is work the kernel should have
//! done.
//!
//! # Why the unit of checking is a project, not a document
//!
//! A SysML model is usually several files, and a name in one is resolved against the others. A
//! server that checked each buffer alone reported `Engine` unresolved in a file that imports it,
//! and reported nothing at all for a file whose dependency had a syntax error. So a change brings
//! the whole project back through the loader, and the diagnostics are published for every file in
//! it: an error in a file the person is not looking at is still an error in their model, and the
//! editor's Problems panel is where they expect to see it.
//!
//! The buffers the client has open win over the copies on disk. An editor check is about the text
//! being typed, not the text last saved.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sysml_model::{Element, Kind};

use crate::diagnostics::{line_column, offset_of, path_from_uri, Checked, Diagnosis, Severity};

pub mod actions;
pub mod completion;
pub mod diagnostics;
pub mod inlay;
pub mod semantic;
pub mod wire;

/// How many files a project may hold before the directory scan gives up.
///
/// A workspace pointed at a home directory would otherwise load every `.sysml` under it on each
/// keystroke. The cap is generous for a model and small enough that the wrong folder is noticed as
/// "this file is checked alone" rather than as a frozen editor.
const MAX_PROJECT_FILES: usize = 200;

/// The documents the client has open, by URI.
#[derive(Default)]
pub struct Server {
    documents: BTreeMap<String, Document>,
    /// The folder `initialize` named, used when a file's own folder is not a project.
    root: Option<PathBuf>,
    /// True once `initialize` has been answered; a request before that is a protocol error.
    initialised: bool,
}

struct Document {
    text: String,
    version: i64,
}

/// The files one check reads, and the library settings that come with them.
struct Files {
    sources: Vec<PathBuf>,
    extra: Vec<PathBuf>,
    pinned: Option<PathBuf>,
    bundled: bool,
    ledger: Option<PathBuf>,
}

/// What a request's cursor resolves to.
struct Cursor {
    checked: Checked,
    /// The id of the element the caret is inside.
    element: usize,
    /// The identifier written at the caret, or the element's own name when the caret is on
    /// whitespace or a keyword.
    name: String,
    /// The buffer the caret is in, and where in it. A rename needs both: the answer is a set of
    /// edits, and an edit without a buffer is an edit to the wrong file.
    file: usize,
    offset: usize,
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
                self.root = root_of(message);
                vec![reply(
                    id,
                    json!({
                        "capabilities": {
                            "textDocumentSync": 1,
                            "definitionProvider": true,
                            "referencesProvider": true,
                            "hoverProvider": true,
                            "documentSymbolProvider": true,
                            "renameProvider": { "prepareProvider": true },
                            // `.` triggers after a feature, `:` after a type position, and `>`
                            // completes a `:>` clause. A client that also asks on every keystroke
                            // gets the same answer, so the list is not gated on these.
                            "completionProvider": {
                                "triggerCharacters": [":", ".", ">", " "],
                                "resolveProvider": false,
                            },
                            "documentFormattingProvider": true,
                            // The colours are computed here rather than re-derived by each client,
                            // so every editor agrees with the diagram about which name is a
                            // definition and which is a use.
                            "semanticTokensProvider": {
                                "legend": {
                                    "tokenTypes": semantic::TYPES,
                                    "tokenModifiers": semantic::MODIFIERS,
                                },
                                "full": true,
                                "range": false,
                            },
                            // What the model implies and does not write. The hint is not part of
                            // the file, so a reader who does not want it loses nothing.
                            "inlayHintProvider": true,
                            // The mechanical fixes for the two diagnostics that have one. Most
                            // problems are a person's decision, so this answers with an empty
                            // list rather than with a suggestion that needs checking.
                            "codeActionProvider": { "codeActionKinds": ["quickfix"] },
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
                self.diagnostics(&uri)
            }
            ("textDocument/didSave", _) => {
                let Some(params) = message.get("params") else {
                    return Vec::new();
                };
                let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
                // A save is the moment the file on disk becomes what the model says, and the
                // moment a ledger written by another tool (`sysml decide`) becomes visible here.
                self.diagnostics(uri)
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
                self.diagnostics(&uri)
            }
            ("textDocument/didClose", _) => {
                let Some(params) = message.get("params") else {
                    return Vec::new();
                };
                let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
                self.documents.remove(uri);
                // The file is still part of the project, so its diagnostics are not cleared here:
                // checking again republishes them from what is on disk. A closed file with a real
                // error must not go quiet just because nobody has it open.
                self.diagnostics(uri)
            }
            ("textDocument/definition", Some(_)) => vec![reply(id, self.definition(message))],
            ("textDocument/references", Some(_)) => vec![reply(id, self.references(message))],
            ("textDocument/hover", Some(_)) => vec![reply(id, self.hover(message))],
            ("textDocument/documentSymbol", Some(_)) => vec![reply(id, self.symbols(message))],
            ("textDocument/prepareRename", Some(_)) => {
                vec![answered(id, self.prepare_rename(message))]
            }
            ("textDocument/rename", Some(_)) => vec![answered(id, self.rename(message))],
            ("textDocument/completion", Some(_)) => vec![reply(id, self.completion(message))],
            ("textDocument/formatting", Some(_)) => vec![reply(id, self.formatting(message))],
            ("textDocument/semanticTokens/full", Some(_)) => {
                vec![reply(id, self.semantic_tokens(message))]
            }
            ("textDocument/inlayHint", Some(_)) => vec![reply(id, self.inlay_hints(message))],
            ("textDocument/codeAction", Some(_)) => vec![reply(id, code_actions(message))],
            // An unknown request must still be answered, or the client waits for ever.
            (_, Some(id)) => vec![json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("{method} is not implemented") },
            })],
            _ => Vec::new(),
        }
    }

    /// Check the project `uri` belongs to and publish the diagnostics of every file in it.
    fn diagnostics(&self, uri: &str) -> Vec<Value> {
        let Some(path) = path_from_uri(uri) else {
            return Vec::new();
        };
        let files = self.files_for(&path);
        let overrides: BTreeMap<PathBuf, String> = self
            .documents
            .iter()
            .filter_map(|(open, document)| Some((path_from_uri(open)?, document.text.clone())))
            .collect();
        let checked = match Checked::project_against(
            &files.sources,
            &files.extra,
            &overrides,
            files.pinned.as_deref(),
            files.bundled,
            files.ledger,
        ) {
            Ok(checked) => checked,
            Err(message) => {
                // The loader refused the project outright — a file that is not there, or a
                // buffer that is not valid at all. The failure belongs to the file that started
                // this check, because that is the one the person is looking at.
                return vec![json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": { "uri": uri, "diagnostics": [{
                        "range": whole_first_line(),
                        "severity": 1,
                        "code": "SYS000",
                        "source": "sysml",
                        "message": message,
                    }] },
                })];
            }
        };
        let diagnoses = checked.diagnose();
        let mut by_file: BTreeMap<usize, Vec<Diagnosis>> = BTreeMap::new();
        for diagnosis in diagnoses {
            by_file.entry(diagnosis.file).or_default().push(diagnosis);
        }
        let mut out = Vec::new();
        for (index, text) in checked.texts.iter().enumerate() {
            let Some(buffer) = checked.paths.get(index) else {
                continue;
            };
            let items: Vec<Value> = by_file
                .get(&index)
                .map(|list| {
                    list.iter()
                        .map(|diagnosis| diagnostic_json(diagnosis, text))
                        .collect()
                })
                .unwrap_or_default();
            out.push(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": {
                    "uri": diagnostics::uri_from_path(buffer),
                    "version": self.version_of(buffer),
                    "diagnostics": items,
                },
            }));
        }
        out
    }

    /// The version the client last sent for a buffer, so an editor can tell a stale publish from
    /// a current one.
    fn version_of(&self, path: &Path) -> Value {
        let uri = diagnostics::uri_from_path(path);
        match self.documents.get(&uri) {
            Some(document) => json!(document.version),
            None => Value::Null,
        }
    }

    /// The files that make up the project `anchor` sits in.
    ///
    /// A manifest is authoritative when there is one: it names the sources, the files that are
    /// only reachable by name, the library and the ledger, and a server that guessed differently
    /// would check a model other than the one `sysml check` checks. Without a manifest, every
    /// `.sysml` and `.kerml` beside the file is the project, because that is what a folder of one
    /// model looks like and an import across those files has to resolve.
    fn files_for(&self, anchor: &Path) -> Files {
        if let Some(manifest) = sysml_syntax::project::discover(anchor) {
            if let Ok(project) = sysml_syntax::project::Project::read(&manifest) {
                let (pinned, bundled) = match &project.stdlib {
                    sysml_syntax::project::Stdlib::Pinned(path) => (Some(path.clone()), false),
                    sysml_syntax::project::Stdlib::Bundled => (None, true),
                    sysml_syntax::project::Stdlib::Discovered => (None, false),
                };
                return Files {
                    sources: project.sources,
                    extra: project.library,
                    pinned,
                    bundled,
                    ledger: Some(project.ledger),
                };
            }
        }
        let mut sources = match anchor.parent() {
            Some(dir) => scan(dir).unwrap_or_default(),
            None => Vec::new(),
        };
        // The file itself comes first when it was not found by the scan: a path outside the
        // scanned folder, or a folder that held more files than the cap allows.
        if !sources.iter().any(|path| path == anchor) {
            sources.insert(0, anchor.to_path_buf());
        }
        Files {
            sources,
            extra: Vec::new(),
            pinned: None,
            bundled: false,
            ledger: None,
        }
    }

    /// The project a request is about, the element the cursor is on, and the name written there.
    ///
    /// Three answers, because the two questions differ. `definition` and `references` want the name
    /// *written* at the caret: on `part engine : Engine` a caret on `engine` asks about `engine`
    /// and one on `Engine` asks about `Engine`. `hover` wants the element the caret is inside.
    ///
    /// So the name is read out of the buffer first, and it is used when the element under the
    /// caret claims it — as its own name, as a reference it wrote, or as its type. A caret on a
    /// keyword or on whitespace falls back to the element itself.
    fn at_cursor(&self, message: &Value) -> Option<Cursor> {
        let params = message.get("params")?;
        let uri = params["textDocument"]["uri"].as_str()?;
        let path = path_from_uri(uri)?;
        let position = &params["position"];
        let line = position["line"].as_u64()? as u32;
        let character = position["character"].as_u64()? as u32;
        let files = self.files_for(&path);
        let overrides: BTreeMap<PathBuf, String> = self
            .documents
            .iter()
            .filter_map(|(open, document)| Some((path_from_uri(open)?, document.text.clone())))
            .collect();
        let checked = Checked::project_against(
            &files.sources,
            &files.extra,
            &overrides,
            files.pinned.as_deref(),
            files.bundled,
            files.ledger,
        )
        .ok()?;
        // The buffer index the cursor is in. A request about a file the project does not hold is
        // answered as if it named the first buffer, which is the one the loader was given first.
        let buffer = checked
            .paths
            .iter()
            .position(|candidate| candidate == &path)?;
        let text = checked.texts.get(buffer)?.clone();
        let offset = offset_of(&text, line, character)?;
        // The tree records one-based lines; the protocol counts from zero.
        let element = element_at(&checked.loaded.roots, buffer, line + 1)?;
        let written = word_at(&text, offset);
        let named = name_of(element);
        let id = element.id;
        let name = if written.is_empty() { named } else { written };
        Some(Cursor {
            checked,
            element: id,
            name,
            file: buffer,
            offset,
        })
    }

    /// What may be typed at the caret.
    ///
    /// The answer comes from `completion::complete`, which does the classifying; this turns the
    /// suggestions into protocol items and resolves the caret to a buffer position. A completion
    /// request does not need the element the caret is inside — the list is the same everywhere —
    /// so a buffer that does not parse yet still gets a full list, which is the state a file is in
    /// almost the entire time somebody is typing it.
    fn completion(&self, message: &Value) -> Value {
        let params = &message["params"];
        let Some(uri) = params["textDocument"]["uri"].as_str() else {
            return Value::Null;
        };
        let Some(path) = path_from_uri(uri) else {
            return Value::Null;
        };
        let line = params["position"]["line"].as_u64().unwrap_or(0) as u32;
        let character = params["position"]["character"].as_u64().unwrap_or(0) as u32;
        let files = self.files_for(&path);
        // The client's unsaved text is what is being completed. The project on disk is what the
        // names come from, because a half-typed buffer does not parse and its own tree would be
        // empty — which is exactly the moment a completion is asked for.
        let overrides: BTreeMap<PathBuf, String> = self
            .documents
            .iter()
            .filter_map(|(open, document)| Some((path_from_uri(open)?, document.text.clone())))
            .collect();
        let checked = match Checked::project_against(
            &files.sources,
            &files.extra,
            &overrides,
            files.pinned.as_deref(),
            files.bundled,
            files.ledger,
        ) {
            Ok(checked) => checked,
            Err(_) => return Value::Null,
        };
        let Some(buffer) = checked.paths.iter().position(|p| p == &path) else {
            return Value::Null;
        };
        let text = checked.texts.get(buffer).cloned().unwrap_or_default();
        let Some(offset) = offset_of(&text, line, character) else {
            return Value::Null;
        };
        let inside = element_at(&checked.loaded.roots, buffer, line + 1);
        let index = sysml_resolve::Index::build(&checked.loaded.roots)
            .with_library(&checked.loaded.library);
        let suggestions = completion::complete(&completion::Context {
            text: &text,
            offset,
            inside,
            index: &index,
            roots: &checked.loaded.roots,
        });
        let items: Vec<Value> = suggestions.iter().map(item_json).collect();
        json!({ "isIncomplete": false, "items": items })
    }

    /// The whole document, re-indented.
    ///
    /// One edit covering the buffer, because that is what the formatter produces: a text. Asking a
    /// client to apply a set of line ranges would make it re-derive an answer this side already
    /// has, and one wrong range would leave a file that parses but is laid out half one way.
    ///
    /// `null` when the buffer is already formatted — the protocol reads "no edits" as "nothing to
    /// do", and an editor that receives an identical replacement still marks the file modified.
    fn formatting(&self, message: &Value) -> Value {
        let Some(uri) = message["params"]["textDocument"]["uri"].as_str() else {
            return Value::Null;
        };
        let Some(path) = path_from_uri(uri) else {
            return Value::Null;
        };
        let files = self.files_for(&path);
        let overrides: BTreeMap<PathBuf, String> = self
            .documents
            .iter()
            .filter_map(|(open, document)| Some((path_from_uri(open)?, document.text.clone())))
            .collect();
        let checked = match Checked::project_against(
            &files.sources,
            &files.extra,
            &overrides,
            files.pinned.as_deref(),
            files.bundled,
            files.ledger,
        ) {
            Ok(checked) => checked,
            Err(_) => return Value::Null,
        };
        let Some(buffer) = checked.paths.iter().position(|p| p == &path) else {
            return Value::Null;
        };
        let Some(text) = checked.texts.get(buffer) else {
            return Value::Null;
        };
        let after = sysml_syntax::format::format(text);
        if &after == text {
            return Value::Null;
        }
        json!([{
            "range": json!({
                "start": { "line": 0, "character": 0 },
                "end": end_of(text),
            }),
            "newText": after,
        }])
    }

    /// Every coloured token in one document, delta-encoded.
    ///
    /// The whole file at once rather than per range: the classification needs the whole project —
    /// a `Battery` in this file may name a definition in another — so computing it for a window
    /// would cost the same and answer less.
    fn semantic_tokens(&self, message: &Value) -> Value {
        let Some(uri) = message["params"]["textDocument"]["uri"].as_str() else {
            return Value::Null;
        };
        let Some(path) = path_from_uri(uri) else {
            return Value::Null;
        };
        let files = self.files_for(&path);
        let overrides: BTreeMap<PathBuf, String> = self
            .documents
            .iter()
            .filter_map(|(open, document)| Some((path_from_uri(open)?, document.text.clone())))
            .collect();
        let checked = match Checked::project_against(
            &files.sources,
            &files.extra,
            &overrides,
            files.pinned.as_deref(),
            files.bundled,
            files.ledger,
        ) {
            Ok(checked) => checked,
            Err(_) => return Value::Null,
        };
        let Some(buffer) = checked.paths.iter().position(|p| p == &path) else {
            return Value::Null;
        };
        let Some(text) = checked.texts.get(buffer) else {
            return Value::Null;
        };
        let tokens = semantic::tokens(&checked.loaded.roots, &checked.loaded.library, text, buffer);
        json!({ "data": semantic::encode(&tokens) })
    }

    /// The state the model implies but does not write, at the offsets it belongs to.
    ///
    /// The range the client asks about is ignored: the hints are computed for the whole file
    /// because the classification needs the whole project, and a window would cost the same and
    /// answer less. A client that asks per viewport simply gets its slice faster for free.
    fn inlay_hints(&self, message: &Value) -> Value {
        let Some(uri) = message["params"]["textDocument"]["uri"].as_str() else {
            return Value::Null;
        };
        let Some(path) = path_from_uri(uri) else {
            return Value::Null;
        };
        let files = self.files_for(&path);
        let overrides: BTreeMap<PathBuf, String> = self
            .documents
            .iter()
            .filter_map(|(open, document)| Some((path_from_uri(open)?, document.text.clone())))
            .collect();
        let checked = match Checked::project_against(
            &files.sources,
            &files.extra,
            &overrides,
            files.pinned.as_deref(),
            files.bundled,
            files.ledger,
        ) {
            Ok(checked) => checked,
            Err(_) => return Value::Null,
        };
        let Some(buffer) = checked.paths.iter().position(|p| p == &path) else {
            return Value::Null;
        };
        let Some(text) = checked.texts.get(buffer) else {
            return Value::Null;
        };
        let hint_list = inlay::hints(&checked.loaded.roots);
        let items: Vec<Value> = hint_list
            .iter()
            .filter(|hint| hint.file == buffer)
            .map(|hint| {
                let (line, character) = line_column(text, hint.offset);
                json!({
                    "position": { "line": line, "character": character },
                    "label": hint.label,
                    "kind": hint.kind,
                    "paddingLeft": hint.padding_left,
                })
            })
            .collect();
        Value::Array(items)
    }

    /// Where the name under the cursor is declared.
    ///
    /// The name is resolved from the element that contains the cursor, and the answer is a range
    /// on the declaration's own name rather than on the whole line: an editor that jumps from
    /// `Wheel` to line 6 column 0 puts the caret before `part def`, which is not where the name is.
    fn definition(&self, message: &Value) -> Value {
        let Some(Cursor {
            checked,
            element: from,
            name,
            ..
        }) = self.at_cursor(message)
        else {
            return Value::Null;
        };
        let index = sysml_resolve::Index::build(&checked.loaded.roots)
            .with_library(&checked.loaded.library);
        let target = if name.is_empty() {
            Some(from)
        } else {
            match index.resolve(from, &name) {
                sysml_resolve::Resolution::Found(id) => Some(id),
                _ => Some(from),
            }
        };
        let Some(target) = target else {
            return Value::Null;
        };
        let Some(element) = checked.find(&index.qualified[target]) else {
            return Value::Null;
        };
        let span = match element.name_span {
            Some(span) => span,
            // An element with no authored name has no place to jump to, and inventing the start
            // of the first file would send the caret somewhere the person did not ask for.
            None => return Value::Null,
        };
        let Some(text) = checked.texts.get(span.file) else {
            return Value::Null;
        };
        json!({
            "uri": diagnostics::uri_from_path(&checked.paths[span.file]),
            "range": range_in(text, span.start, span.end),
        })
    }

    /// Every name that resolves to the declaration under the cursor.
    ///
    /// Searched across the whole project, which is the only answer that is useful: a definition
    /// used from three files has three references, and a client that only heard about the current
    /// buffer showed one.
    fn references(&self, message: &Value) -> Value {
        let Some(Cursor {
            checked,
            element: from,
            name,
            ..
        }) = self.at_cursor(message)
        else {
            return Value::Null;
        };
        let index = sysml_resolve::Index::build(&checked.loaded.roots)
            .with_library(&checked.loaded.library);
        let target = if name.is_empty() {
            from
        } else {
            match index.resolve(from, &name) {
                sysml_resolve::Resolution::Found(id) => id,
                _ => from,
            }
        };
        let mut hits = Vec::new();
        for reference in index.references(&checked.loaded.roots) {
            if reference.target.found() != Some(target) {
                continue;
            }
            let Some(owner) = checked.find(&index.qualified[reference.from]) else {
                continue;
            };
            let Some(span) = position_of(owner, &reference.text).or(owner.name_span) else {
                continue;
            };
            let Some(text) = checked.texts.get(span.file) else {
                continue;
            };
            hits.push(json!({
                "uri": diagnostics::uri_from_path(&checked.paths[span.file]),
                "range": range_in(text, span.start, span.end),
            }));
        }
        Value::Array(hits)
    }

    /// What the cursor is on.
    ///
    /// A name written at the caret wins over the element containing it: on `part engine : Engine`
    /// a caret on `Engine` describes the type, which is what a reader pointing at a name is asking
    /// about. The container is the fallback for a caret on a keyword or in whitespace.
    fn hover(&self, message: &Value) -> Value {
        let Some(Cursor {
            checked,
            element: from,
            name,
            ..
        }) = self.at_cursor(message)
        else {
            return Value::Null;
        };
        let index = sysml_resolve::Index::build(&checked.loaded.roots)
            .with_library(&checked.loaded.library);
        let containing = checked
            .loaded
            .roots
            .iter()
            .flat_map(walk)
            .find(|el| el.id == from);
        let named = match index.resolve(from, &name) {
            sysml_resolve::Resolution::Found(id) => checked
                .loaded
                .roots
                .iter()
                .flat_map(walk)
                .find(|el| el.id == id),
            _ => None,
        };
        let Some(element) = named.or(containing) else {
            return Value::Null;
        };
        let qualified = index
            .qualified
            .get(element.id)
            .cloned()
            .unwrap_or_else(|| element.name.clone());
        let kind = element.kind.keyword_as(element.is_definition);
        let mut body = format!("```sysml\n{kind} {qualified}\n```");
        if element.is_composite {
            body.push_str(
                "\n\n**Composite**: a part of its owning instance, not merely a reference.",
            );
        } else if element.modifiers.is_reference {
            body.push_str("\n\n**Reference**: refers to an instance without composite ownership.");
        }
        if let Some(constraint) = &element.constraint {
            body.push_str(&format!("\n\n`{constraint}`"));
        }
        if let Some(doc) = &element.doc {
            body.push_str(&format!("\n\n{doc}"));
        }
        json!({ "contents": { "kind": "markdown", "value": body } })
    }

    /// The file's declarations, for the editor's own outline.
    ///
    /// The kernel's own outline (`sysml_studio::document::outline`) is what the canvas navigator
    /// lists, and an editor's outline showing a different tree would be two answers to one
    /// question. That crate is a server with an HTTP surface, so the rule for which elements count
    /// is restated here rather than depended on: relationship statements are claims about elements,
    /// not members, and a name nobody declared is not a symbol.
    fn symbols(&self, message: &Value) -> Value {
        let Some(uri) = message["params"]["textDocument"]["uri"].as_str() else {
            return Value::Null;
        };
        let Some(path) = path_from_uri(uri) else {
            return Value::Null;
        };
        let files = self.files_for(&path);
        let overrides: BTreeMap<PathBuf, String> = self
            .documents
            .iter()
            .filter_map(|(open, document)| Some((path_from_uri(open)?, document.text.clone())))
            .collect();
        let Ok(checked) = Checked::project_against(
            &files.sources,
            &files.extra,
            &overrides,
            files.pinned.as_deref(),
            files.bundled,
            files.ledger,
        ) else {
            return Value::Null;
        };
        let Some(buffer) = checked.paths.iter().position(|p| p == &path) else {
            return Value::Null;
        };
        let text = checked.texts.get(buffer).cloned().unwrap_or_default();
        let mut out = Vec::new();
        for root in &checked.loaded.roots {
            symbols_in(root, buffer, &text, &mut out);
        }
        Value::Array(out)
    }

    /// The range a rename would replace, or `null` when nothing here can be renamed.
    ///
    /// A client asks this before it opens its rename box, so the answer doubles as the check: a
    /// caret on a keyword, on punctuation, or on a name that resolves to nothing gets `null` and
    /// the editor never shows a field that could only fail.
    ///
    /// The range is the *declaration's* name when the caret is on a declaration, and the written
    /// word when it is on a reference. Renaming through a reference renames the element it binds
    /// to, which is what a person expects from `F2` on a usage.
    fn prepare_rename(&self, message: &Value) -> Value {
        let Some(cursor) = self.at_cursor(message) else {
            return Value::Null;
        };
        let Some(span) = self.rename_span(&cursor) else {
            return Value::Null;
        };
        let Some(text) = cursor.checked.texts.get(span.file) else {
            return Value::Null;
        };
        json!({
            "range": range_in(text, span.start, span.end),
            "placeholder": cursor.checked.texts
                .get(span.file)
                .and_then(|t| t.get(span.start..span.end))
                .unwrap_or_default(),
        })
    }

    /// Rename the element at the caret, everywhere the model refers to it.
    ///
    /// The edit itself is the canvas's own (`sysml_studio::edit::rename`): it rewrites the
    /// declaration and then every reference the *resolver* binds to that element, rather than
    /// every token that spells the same word. That distinction is the whole point of the feature —
    /// a `Battery` in one package and a `Battery` in another are different elements, and a
    /// text-replacing rename would silently merge them in somebody else's model.
    ///
    /// The work is done against the buffers this server holds, unsaved text included, so the
    /// answer matches what the person sees rather than what was last saved.
    fn rename(&self, message: &Value) -> Value {
        let Some(cursor) = self.at_cursor(message) else {
            return error_reply("nothing to rename under the cursor");
        };
        let Some(new_name) = message["params"]["newName"].as_str() else {
            return error_reply("rename needs a new name");
        };
        // A new name that is a reserved word would produce a file the checker then rejects, so the
        // rename is refused here with the same advice the checker gives. A space is not enough to
        // make it legal either: `part 'frame';` quotes the word, and a rename cannot add quotes to
        // every reference without also changing what those references mean to a person reading.
        if sysml_parser::is_reserved_word(new_name.trim()) {
            let word = new_name.trim();
            return error_reply(&format!(
                "`{word}` is a reserved word and cannot be a name; write `'{word}'` to use it as one"
            ));
        }
        // The element to rename: the one the caret is on when it is a declaration, or the one the
        // word under the caret binds to.
        let checked = &cursor.checked;
        let index = sysml_resolve::Index::build(&checked.loaded.roots)
            .with_library(&checked.loaded.library);
        let Some(target) = rename_target(&index, &cursor) else {
            return error_reply("nothing to rename under the cursor");
        };
        let Some(qualified) = index.qualified.get(target).cloned() else {
            return error_reply("that element has no name");
        };
        let sources = sysml_studio::edit::Sources::of(&checked.paths, checked.texts.clone());
        match sysml_studio::edit::rename(&sources, &qualified, new_name) {
            Ok(outcome) => workspace_edit(&outcome, &checked.paths),
            Err(refused) => error_reply(&refused.0),
        }
    }

    /// The span a rename would replace, when there is one.
    fn rename_span(&self, cursor: &Cursor) -> Option<sysml_model::TextRange> {
        let index = sysml_resolve::Index::build(&cursor.checked.loaded.roots)
            .with_library(&cursor.checked.loaded.library);
        let target = rename_target(&index, cursor)?;
        // Looked up by id, not by name: the project may hold two elements called `Battery`, and a
        // search by qualified name answers with whichever the tree walk meets first.
        let element = element_by_id(&cursor.checked.loaded.roots, target)?;
        // A declaration is renamed at its own name; a reference at the word written there.
        let at_word = element
            .ref_spans
            .iter()
            .find(|(written, span)| {
                *written == cursor.name
                    && span.file == cursor.file
                    && span.start <= cursor.offset
                    && cursor.offset <= span.end
            })
            .map(|(_, span)| *span);
        at_word.or(element.name_span)
    }
}

fn position_range(value: &Value) -> Option<(u32, u32)> {
    Some((
        value["line"].as_u64()? as u32,
        value["character"].as_u64()? as u32,
    ))
}

/// The fixes that apply to the diagnostics the client reported.
///
/// Driven by the client's own diagnostic list rather than by recomputing them: the client asks
/// about the problems it drew, and a fix anchored to a range this side computed could land on a
/// different token after an edit the client has already sent. `diagnostics` is what makes the two
/// agree — it is the same range the squiggle is under.
fn code_actions(message: &Value) -> Value {
    let Some(params) = message.get("params") else {
        return Value::Array(Vec::new());
    };
    let Some(uri) = params["textDocument"]["uri"].as_str() else {
        return Value::Array(Vec::new());
    };
    let Some(context) = params.get("context") else {
        return Value::Array(Vec::new());
    };
    let mut out = Vec::new();
    let empty = Vec::new();
    let diagnostics = context["diagnostics"].as_array().unwrap_or(&empty);
    for diagnostic in diagnostics {
        let Some(code) = diagnostic["code"].as_str() else {
            continue;
        };
        let text = diagnostic["message"].as_str().unwrap_or_default();
        let range = &diagnostic["range"];
        let Some(start) = position_range(&range["start"]) else {
            continue;
        };
        let Some(end) = position_range(&range["end"]) else {
            continue;
        };
        for fix in actions::fixes(code, text, start, end) {
            out.push(json!({
                "title": fix.title,
                "kind": "quickfix",
                "diagnostics": [diagnostic],
                "edit": {
                    "changes": {
                        uri: [{
                            "range": {
                                "start": { "line": fix.edit.start.0, "character": fix.edit.start.1 },
                                "end": { "line": fix.edit.end.0, "character": fix.edit.end.1 },
                            },
                            "newText": fix.edit.text,
                        }],
                    },
                },
            }));
        }
    }
    Value::Array(out)
}

/// The element with this id, in a pre-order walk of the project roots.
fn element_by_id(roots: &[Element], id: usize) -> Option<&Element> {
    fn walk(el: &Element, id: usize) -> Option<&Element> {
        if el.id == id {
            return Some(el);
        }
        el.children.iter().find_map(|child| walk(child, id))
    }
    roots.iter().find_map(|root| walk(root, id))
}

/// The element a rename at this cursor should change.
///
/// A word that binds to something wins: `F2` on a usage renames the definition it is typed by,
/// which is what a person means. A caret on a declaration with no reference there answers the
/// declaration itself.
fn rename_target(index: &sysml_resolve::Index, cursor: &Cursor) -> Option<usize> {
    if cursor.name.is_empty() {
        return Some(cursor.element);
    }
    match index.resolve(cursor.element, &cursor.name) {
        sysml_resolve::Resolution::Found(id) => Some(id),
        _ => Some(cursor.element),
    }
}

/// A `WorkspaceEdit` carrying every buffer the rename changed.
///
/// Only the buffers that actually differ are listed, and each is sent as a whole-buffer
/// replacement. That is deliberate: the kernel computed one text per file and verified it by
/// reparsing, so asking a client to apply a set of ranges would re-derive an edit this side
/// already proved correct — and one wrong range would corrupt the file where a whole-buffer swap
/// cannot.
fn workspace_edit(outcome: &sysml_studio::edit::Outcome, paths: &[std::path::PathBuf]) -> Value {
    let mut changes = serde_json::Map::new();
    for (index, after) in outcome.after.texts.iter().enumerate() {
        let before = outcome.before.texts.get(index);
        if before == Some(after) {
            continue;
        }
        let Some(path) = paths.get(index) else {
            continue;
        };
        let text = before.map(String::as_str).unwrap_or_default();
        changes.insert(
            diagnostics::uri_from_path(path),
            json!([{
                "range": json!({
                    "start": { "line": 0, "character": 0 },
                    "end": end_of(text),
                }),
                "newText": after,
            }]),
        );
    }
    json!({ "changes": Value::Object(changes) })
}

/// The position just past the end of `text`, as a protocol position.
fn end_of(text: &str) -> Value {
    let (line, character) = line_column(text, text.len());
    json!({ "line": line, "character": character })
}

/// One completion suggestion, in the protocol's shape.
///
/// `sortText` is what orders the list — a client that sorts by `label` would put `Battery` above
/// `part`, and a keyword is rarely what a person wants after `: `. `filterText` is the insert
/// without its quotes, so typing `vehicle` still matches `'vehicle-trailer system'`.
fn item_json(suggestion: &completion::Suggestion) -> Value {
    let bare = suggestion.insert.trim_matches('\'').replace("\\'", "'");
    json!({
        "label": suggestion.label,
        "kind": suggestion.kind,
        "detail": suggestion.detail,
        "sortText": suggestion.rank,
        "filterText": bare,
        "insertText": suggestion.insert,
        "insertTextFormat": 1,
    })
}

/// A JSON-RPC error, so a client shows the reason rather than failing silently.
///
/// The `error` object sits at the top level of the response, beside `id` — not inside `result`.
/// A client that finds it in `result` treats the call as a success and hands the editor a value
/// with neither `changes` nor `range`, so the rename dialog closes having done nothing and said
/// nothing. The request routing checks for this shape and builds the message itself.
fn error_reply(message: &str) -> Value {
    json!({ "error": { "code": -32603, "message": message } })
}

/// Whether a handler's answer is an error rather than a result.
/// The symbols one element and its members contribute, in the protocol's nesting shape.
///
/// Only elements written in `buffer` are listed: the project may have ten files and this is the
/// outline of one, and a member inherited from a type in another file is not a line here.
fn symbols_in(el: &Element, buffer: usize, text: &str, out: &mut Vec<Value>) {
    if matches!(
        el.kind,
        Kind::Import | Kind::Satisfy | Kind::Connection | Kind::Unsupported
    ) {
        return;
    }
    let mut children = Vec::new();
    for child in &el.children {
        symbols_in(child, buffer, text, &mut children);
    }
    let own = el.name_span.filter(|span| span.file == buffer);
    if own.is_none() && children.is_empty() {
        return;
    }
    let name = if el.name.is_empty() || el.name == "<anonymous>" {
        el.kind.keyword().to_string()
    } else {
        el.name.clone()
    };
    let range = match own {
        Some(span) => range_in(text, span.start, span.end),
        None => range_in(text, 0, 0),
    };
    out.push(json!({
        "name": name,
        // The protocol's symbolic kinds: a package is a namespace, a definition is a class, and
        // everything else that names a member is a field. The kernel's own kinds do not map to
        // the protocol's list one for one, and a client draws the icon from this alone.
        "kind": match el.kind {
            Kind::Package => 3,
            _ if el.kind.is_definition() => 5,
            _ => 13,
        },
        "detail": el.kind.keyword_as(el.is_definition),
        "range": range,
        "selectionRange": range,
        "children": children,
    }));
}

fn reply(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// A handler's answer placed where the protocol expects it.
///
/// A success becomes `result`; a refusal becomes a top-level `error`, which is the only shape a
/// client treats as a failure. Putting the error inside `result` made a refused rename look like
/// a successful one that changed nothing.
fn answered(id: Option<Value>, outcome: Value) -> Value {
    match outcome.get("error") {
        Some(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        None => reply(id, outcome),
    }
}

fn whole_first_line() -> Value {
    json!({
        "start": { "line": 0, "character": 0 },
        "end": { "line": 0, "character": 200 },
    })
}

/// The protocol's range for a byte span.
fn range_in(text: &str, start: usize, end: usize) -> Value {
    let (sl, sc) = line_column(text, start);
    let (el, ec) = line_column(text, end);
    json!({
        "start": { "line": sl, "character": sc },
        "end": { "line": el, "character": ec },
    })
}

/// One diagnostic, with its range.
fn diagnostic_json(diagnosis: &Diagnosis, text: &str) -> Value {
    let range = match diagnosis.span {
        Some(span) => range_in(text, span.start, span.end),
        None => {
            // Only a line is known. Marking the whole line is what the server did for every
            // problem before it knew spans; it is a fallback now, not the rule.
            let line = diagnosis.line.saturating_sub(1);
            json!({
                "start": { "line": line, "character": 0 },
                "end": { "line": line, "character": 200 },
            })
        }
    };
    json!({
        "range": range,
        "severity": match diagnosis.severity {
            Severity::Error => 1,
            Severity::Warning => 2,
            // Information. A model whose ledger has not been started is not broken, and the
            // command line says so with its own exit code; a squiggle here would contradict it.
            Severity::Info => 3,
        },
        "code": diagnosis.code,
        "source": "sysml",
        "message": diagnosis.message,
    })
}

/// Every element of a tree, pre-order.
fn walk(el: &Element) -> Vec<&Element> {
    let mut out = vec![el];
    for child in &el.children {
        out.extend(walk(child));
    }
    out
}

/// The element the line at `line` (one-based) is inside.
///
/// The innermost element whose own extent contains the line, where an element's extent is its
/// declaration's line through the last line any descendant declares. That is what "the thing I am
/// looking at" means: a cursor inside a `part def`'s body belongs to the `part def` until a member
/// is declared on or above the cursor's line, and then it belongs to that member.
///
/// The previous answer was the last element of the tree, whatever the cursor was on — hover
/// reported the final declaration in the file and go-to-definition jumped there from anywhere,
/// which is why both looked broken.
/// The element the caret on `line` of `file` is inside.
///
/// The innermost element whose extent covers the line **and that was written in `file`**. The
/// file test is not optional: a model is several files, their lines are numbered from one in each,
/// and every file tends to hold a declaration on its second line. Without it, a caret on `P`'s
/// second line answered with whatever `Q` declared on its second line — hover named the wrong
/// element and a rename changed a definition nobody had touched.
///
/// An element written elsewhere is skipped, but its descendants are still walked: a synthesised
/// element can carry no span of its own while its children do.
fn element_at<'a>(roots: &'a [Element], file: usize, line: u32) -> Option<&'a Element> {
    fn extent(el: &Element) -> u32 {
        el.children
            .iter()
            .map(extent)
            .fold(el.line, |last, child| last.max(child))
    }
    /// Whether any span this element kept was written in `file`.
    fn belongs(el: &Element, file: usize) -> bool {
        let mut spans = [el.name_span, el.value_span, el.type_span]
            .into_iter()
            .flatten()
            .chain(el.ref_spans.iter().map(|(_, span)| *span));
        spans.any(|span| span.file == file)
    }
    fn deepest<'a>(el: &'a Element, file: usize, line: u32, best: &mut Option<&'a Element>) {
        if belongs(el, file) && el.line <= line && line <= extent(el) {
            *best = Some(el);
        }
        for child in &el.children {
            deepest(child, file, line, best);
        }
    }
    let mut best = None;
    for root in roots {
        deepest(root, file, line, &mut best);
    }
    best
}

/// The identifier the caret at `offset` is in or beside.
///
/// Read from the buffer, because the tree records a declaration's own name and the name at the
/// caret may be a reference the declaration only mentions. A caret just after a word counts as
/// inside it: an editor places the caret at the end of a name after a double-click or a search
/// jump, and answering "nothing" there makes go-to-definition depend on aiming one character left.
fn word_at(text: &str, offset: usize) -> String {
    let offset = offset.min(text.len());
    if !text.is_char_boundary(offset) {
        return String::new();
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let start = text[..offset]
        .char_indices()
        .rev()
        .find(|(_, c)| !is_word(*c))
        .map(|(index, c)| index + c.len_utf8())
        .unwrap_or(0);
    let end = text[offset..]
        .find(|c: char| !is_word(c))
        .map(|index| offset + index)
        .unwrap_or(text.len());
    text[start..end].to_string()
}

/// The name an element answers to, for a lookup from the cursor.
fn name_of(element: &Element) -> String {
    if element.name == "<anonymous>" || element.name == "syntax error" {
        String::new()
    } else {
        element.name.clone()
    }
}

/// The span of a reference named `text` inside one element.
fn position_of(el: &Element, text: &str) -> Option<sysml_model::TextRange> {
    let first = text.split("::").next().unwrap_or(text);
    for (written, span) in &el.ref_spans {
        if written == text || written == first {
            return Some(*span);
        }
    }
    if el
        .type_ref
        .as_deref()
        .is_some_and(|t| t == text || t == first)
    {
        return el.type_span;
    }
    None
}

/// The workspace folder the client named, when it named one.
fn root_of(message: &Value) -> Option<PathBuf> {
    let params = message.get("params")?;
    if let Some(folders) = params.get("workspaceFolders").and_then(Value::as_array) {
        if let Some(first) = folders.first().and_then(|f| f["uri"].as_str()) {
            return path_from_uri(first);
        }
    }
    params["rootUri"].as_str().and_then(path_from_uri)
}

/// Every `.sysml` and `.kerml` under `dir`, sorted, up to [`MAX_PROJECT_FILES`].
///
/// Sorted because `read_dir` is not: ids are assigned in load order, so two machines would
/// otherwise number one model differently and every diagnostic would carry a different id.
fn scan(dir: &Path) -> Option<Vec<PathBuf>> {
    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        if out.len() >= MAX_PROJECT_FILES {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if out.len() >= MAX_PROJECT_FILES {
                return;
            }
            if path.is_dir() {
                // Build output and dependency directories are not the model, and walking them on
                // every keystroke is the difference between a check and a stall.
                let skip = matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("target" | "node_modules" | ".git" | ".svn" | "dist" | "out")
                );
                if !skip {
                    collect(&path, out);
                }
            } else if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("sysml" | "kerml")
            ) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    collect(dir, &mut out);
    out.sort();
    out.dedup();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_with(files: &[(&str, &str)]) -> (Server, PathBuf) {
        let dir = crate::diagnostics::scratch("server");
        let mut first = PathBuf::new();
        for (name, text) in files {
            let path = dir.join(name);
            std::fs::write(&path, text).expect("write");
            if first.as_os_str().is_empty() {
                first = path;
            }
        }
        let mut server = Server::new();
        let uri = diagnostics::uri_from_path(&first);
        server.handle(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "rootUri": diagnostics::uri_from_path(&dir) }
        }));
        server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": uri, "languageId": "sysml", "version": 1,
                                          "text": files[0].1 } },
        }));
        (server, first)
    }

    /// The diagnostics published for one URI.
    fn diagnostics_of(replies: &[Value], path: &Path) -> Vec<Value> {
        let want = diagnostics::uri_from_path(path);
        replies
            .iter()
            .filter(|reply| reply["params"]["uri"] == want)
            .flat_map(|reply| {
                reply["params"]["diagnostics"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .collect()
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
    fn opening_a_clean_model_reports_only_the_ledger() {
        let (mut server, path) = server_with(&[("m.sysml", "package P { part def A; }\n")]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path), "version": 2 },
                "contentChanges": [{ "text": "package P { part def A; }\n" }],
            },
        }));
        let diagnostics = diagnostics_of(&replies, &path);
        assert!(
            diagnostics
                .iter()
                .all(|d| d["code"].as_str().is_some_and(|c| c.starts_with("PROV"))),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn a_syntax_error_becomes_a_diagnostic_on_its_line() {
        let (mut server, path) = server_with(&[("m.sysml", "package P;\n")]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path), "version": 2 },
                "contentChanges": [{ "text": "package P {\n  this is not sysml\n}\n" }],
            },
        }));
        let diagnostics = diagnostics_of(&replies, &path);
        assert!(!diagnostics.is_empty(), "a broken buffer is reported");
        let syntax: Vec<&Value> = diagnostics
            .iter()
            .filter(|d| d["code"] == "SYN001")
            .collect();
        // Two tokens on that line cannot be placed, and the parser reports both — the same two
        // the command line prints. What matters here is that they arrive at all, carry an error
        // severity and name a range on the line the person has to fix.
        assert_eq!(syntax.len(), 2, "the parser's own count, not one squiggle");
        for one in &syntax {
            assert_eq!(one["severity"], 1);
            assert_eq!(one["range"]["start"]["line"], 1, "{one}");
        }
    }

    #[test]
    fn a_name_another_file_declares_resolves() {
        let (mut server, path) = server_with(&[
            (
                "a.sysml",
                "package A {\n  private import B::*;\n  part def Car { part engine : Engine; }\n}\n",
            ),
            ("b.sysml", "package B {\n  part def Engine;\n}\n"),
        ]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path), "version": 2 },
                "contentChanges": [{ "text": "package A {\n  private import B::*;\n  part def Car { part engine : Engine; }\n}\n" }],
            },
        }));
        let diagnostics = diagnostics_of(&replies, &path);
        assert!(
            !diagnostics.iter().any(|d| d["code"] == "REF001"),
            "Engine is declared in the file beside this one: {diagnostics:?}"
        );
    }

    #[test]
    fn an_unresolved_name_is_reported_rather_than_silently_accepted() {
        let (mut server, path) = server_with(&[(
            "a.sysml",
            "package A {\n  part def Car { part engine : Engine; }\n}\n",
        )]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path), "version": 2 },
                "contentChanges": [{ "text": "package A {\n  part def Car { part engine : Engine; }\n}\n" }],
            },
        }));
        let diagnostics = diagnostics_of(&replies, &path);
        assert!(
            diagnostics.iter().any(|d| d["code"] == "REF001"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn a_diagnostic_range_covers_the_name_not_the_line() {
        let (mut server, path) = server_with(&[(
            "a.sysml",
            "package A {\n  part def Car { part engine : Engine; }\n}\n",
        )]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path), "version": 2 },
                "contentChanges": [{ "text": "package A {\n  part def Car { part engine : Engine; }\n}\n" }],
            },
        }));
        let diagnostics = diagnostics_of(&replies, &path);
        let found = diagnostics
            .iter()
            .find(|d| d["code"] == "REF001")
            .expect("an unresolved type");
        let range = &found["range"];
        assert_eq!(range["start"]["line"], 1);
        assert!(
            range["start"]["character"].as_u64().unwrap_or(0) > 0,
            "the range starts at the name, not at column 0: {range}"
        );
        assert!(
            range["end"]["character"] != 200,
            "the range ends at the name, not at a made-up column: {range}"
        );
    }

    #[test]
    fn an_unknown_request_is_answered_rather_than_left_hanging() {
        let mut server = Server::new();
        // `textDocument/signatureHelp` is genuinely not implemented. This has now named
        // `formatting` and `codeAction`, each of which then became a request the server answers.
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 9, "method": "textDocument/signatureHelp", "params": {}
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
    fn closing_a_document_keeps_the_diagnostics_of_the_file() {
        let (mut server, path) = server_with(&[("m.sysml", "package P { part def A; }\n")]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": { "textDocument": { "uri": diagnostics::uri_from_path(&path) } },
        }));
        // The file is still in the project, and the ledger still has something to say about it.
        // Clearing the list would make a real problem disappear when a tab closed.
        assert!(!replies.is_empty(), "the project is checked again");
    }

    #[test]
    fn hover_describes_the_element_under_the_cursor() {
        let text =
            "package P {\n  part def Wheel;\n  part def Car {\n    part wheel : Wheel;\n  }\n}\n";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 7, "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                "position": { "line": 3, "character": 5 },
            },
        }));
        let value = replies[0]["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_default();
        assert!(
            value.contains("P::Car::wheel"),
            "the cursor is on `wheel`, not on the last declaration in the file: {value}"
        );
    }

    #[test]
    fn ownership_is_explained_in_hover_but_not_inserted_as_an_inlay_hint() {
        let text = "package P {\n  part def Wheel;\n  part def Car {\n    part wheel : Wheel;\n    ref part spare : Wheel;\n    composite part axle : Wheel;\n    attribute count = 4;\n  }\n}\n";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        for (line, character, name, ownership) in [
            (3, 10, "P::Car::wheel", Some("**Composite**")),
            (4, 15, "P::Car::spare", Some("**Reference**")),
            (5, 20, "P::Car::axle", Some("**Composite**")),
            (3, 19, "P::Wheel", None),
            (2, 12, "P::Car", None),
        ] {
            let replies = server.handle(&json!({
                "jsonrpc": "2.0", "id": 7, "method": "textDocument/hover",
                "params": {
                    "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                    "position": { "line": line, "character": character },
                },
            }));
            let value = replies[0]["result"]["contents"]["value"]
                .as_str()
                .expect("hover text");
            assert!(value.contains(name), "{value}");
            if let Some(label) = ownership {
                assert!(value.contains(label), "{value}");
            } else {
                assert!(!value.contains("**Composite**"), "{value}");
                assert!(!value.contains("**Reference**"), "{value}");
            }
        }
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 8, "method": "textDocument/inlayHint",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 9, "character": 0 },
                },
            },
        }));
        let hints = replies[0]["result"].as_array().expect("inlay hints");
        assert_eq!(hints.len(), 1, "{hints:?}");
        assert_eq!(hints[0]["label"], ": Integer");
        assert_eq!(hints[0]["kind"], 1);
    }

    #[test]
    fn hover_on_an_objective_names_it_and_quotes_its_prose() {
        // The objective's own `doc` is what states the objective. A hover that answered with the
        // enclosing case would be describing the wrong thing and dropping the sentence that makes
        // the element worth pointing at.
        let text = "package P {\n  requirement def R;\n  analysis def A {\n    objective obj : R {\n      doc /* the objective prose */\n    }\n  }\n}\n";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 7, "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                "position": { "line": 3, "character": 15 },
            },
        }));
        let value = replies[0]["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_default();
        assert!(value.contains("P::A::obj"), "the objective itself: {value}");
        assert!(
            value.contains("the objective prose"),
            "and the sentence it states: {value}"
        );
    }

    #[test]
    fn definition_points_at_the_declaration_name() {
        let text =
            "package P {\n  part def Wheel;\n  part def Car {\n    part wheel : Wheel;\n  }\n}\n";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 8, "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                "position": { "line": 3, "character": 17 },
            },
        }));
        let range = &replies[0]["result"]["range"];
        assert_eq!(
            range["start"]["line"], 1,
            "Wheel is declared on line 2 (zero-based 1)"
        );
        assert!(
            range["start"]["character"].as_u64().unwrap_or(0) > 0,
            "the caret lands on the name: {range}"
        );
    }

    #[test]
    fn a_caret_on_the_name_declared_here_answers_that_name_not_the_type() {
        // `part wheel : Wheel` declares `wheel` and names `Wheel`. A caret on each half has to get
        // a different answer, or go-to-definition on a declaration jumps to its own type.
        let text =
            "package P {\n  part def Wheel;\n  part def Car {\n    part wheel : Wheel;\n  }\n}\n";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        let mut ask = |character: u64| {
            let replies = server.handle(&json!({
                "jsonrpc": "2.0", "id": 8, "method": "textDocument/definition",
                "params": {
                    "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                    "position": { "line": 3, "character": character },
                },
            }));
            replies[0]["result"].clone()
        };
        // Columns: `part wheel : Wheel` starts at 4; `wheel` is 9..14, `Wheel` is 17..22.
        assert_eq!(
            ask(11)["range"]["start"]["line"],
            3,
            "the declaration itself"
        );
        assert_eq!(
            ask(19)["range"]["start"]["line"],
            1,
            "the type, declared on the second line"
        );
    }

    #[test]
    fn a_word_at_the_caret_is_read_from_the_buffer() {
        let text = "part weight : Mass;\n";
        assert_eq!(word_at(text, 6), "weight");
        assert_eq!(word_at(text, 15), "Mass");
        // Just past a word is still that word: a double-click and a search jump both leave the
        // caret at the end of the name.
        assert_eq!(word_at(text, 10), "weight");
        // On the punctuation between two names there is no name.
        assert_eq!(word_at(text, 13), "");
    }

    /// Open two files in one project folder, the way an editor holds a model split across files.
    fn server_with_two(first: (&str, &str), second: (&str, &str)) -> (Server, PathBuf, PathBuf) {
        let dir = crate::diagnostics::scratch("rename");
        let one = dir.join(first.0);
        let two = dir.join(second.0);
        std::fs::write(&one, first.1).expect("write first");
        std::fs::write(&two, second.1).expect("write second");
        let mut server = Server::new();
        server.handle(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "rootUri": diagnostics::uri_from_path(&dir) }
        }));
        for (path, text) in [(&one, first.1), (&two, second.1)] {
            server.handle(&json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": { "textDocument": { "uri": diagnostics::uri_from_path(path),
                                              "languageId": "sysml", "version": 1, "text": text } },
            }));
        }
        (server, one, two)
    }

    /// The new text a rename produced for one file, or `None` when it changed nothing there.
    fn renamed(result: &Value, path: &Path) -> Option<String> {
        result["changes"][diagnostics::uri_from_path(path)][0]["newText"]
            .as_str()
            .map(str::to_string)
    }

    #[test]
    fn rename_changes_the_declaration_and_its_references() {
        let text = "package P {\n  part def Battery;\n  part def Car {\n    part battery : Battery;\n  }\n}\n";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                "position": { "line": 1, "character": 13 },
                "newName": "PowerCell",
            },
        }));
        let after = renamed(&replies[0]["result"], &path).expect("the file changed");
        assert!(after.contains("part def PowerCell;"), "{after}");
        assert!(
            after.contains("part battery : PowerCell;"),
            "the reference moved too: {after}"
        );
    }

    #[test]
    fn rename_touches_only_the_element_that_was_asked_for() {
        // Two files, each declaring a `Battery`. Renaming one must not touch the other: a
        // text-replacing rename would, and the model would still parse, which is the worst kind
        // of wrong.
        let one = "package P {\n  part def Battery;\n}\n";
        let two = "package Q {\n  part def Battery;\n}\n";
        let (mut server, first, second) = server_with_two(("a.sysml", one), ("b.sysml", two));
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&first) },
                "position": { "line": 1, "character": 13 },
                "newName": "PowerCell",
            },
        }));
        let result = replies[0]["result"].clone();
        assert!(
            renamed(&result, &first)
                .is_some_and(|t| t.contains("P::") || t.contains("def PowerCell")),
            "the asked-for file changed: {result}"
        );
        assert_eq!(
            renamed(&result, &second),
            None,
            "the other file's same-named element was left alone: {result}"
        );
    }

    #[test]
    fn rename_through_a_reference_renames_the_element_it_binds_to() {
        let text = "package P {\n  part def Battery;\n  part def Car {\n    part battery : Battery;\n  }\n}\n";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        // Column 21 on line 3 is the type name `Battery`, not the declaration.
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                "position": { "line": 3, "character": 21 },
                "newName": "Accumulator",
            },
        }));
        let after = renamed(&replies[0]["result"], &path).expect("the file changed");
        assert!(
            after.contains("part def Accumulator;"),
            "the declaration moved: {after}"
        );
    }

    #[test]
    fn a_reserved_word_cannot_be_the_new_name() {
        let text = "package P {\n  part def Battery;\n}\n";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                "position": { "line": 1, "character": 13 },
                "newName": "frame",
            },
        }));
        let message = replies[0]["error"]["message"].as_str().unwrap_or_default();
        assert!(message.contains("reserved word"), "got {:?}", replies[0]);
    }

    #[test]
    fn a_bare_reserved_word_as_a_name_is_reported() {
        let items = {
            let dir = crate::diagnostics::scratch("reserved");
            let path = dir.join("m.sysml");
            std::fs::write(
                &path,
                "package P {\n  part def A {\n    part frame;\n  }\n}\n",
            )
            .expect("write");
            crate::diagnostics::Checked::project(&[path], &std::collections::BTreeMap::new(), None)
                .expect("loads")
                .diagnose()
        };
        let found: Vec<&crate::diagnostics::Diagnosis> =
            items.iter().filter(|d| d.code == "RES001").collect();
        assert_eq!(found.len(), 1, "one declaration, one report");
        assert!(
            found[0].message.contains("'frame'"),
            "the message says how to write it"
        );
    }

    #[test]
    fn a_quoted_reserved_word_is_a_legal_name() {
        let dir = crate::diagnostics::scratch("quoted");
        let path = dir.join("m.sysml");
        std::fs::write(
            &path,
            "package P {\n  part def A {\n    part 'frame';\n  }\n}\n",
        )
        .expect("write");
        let items =
            crate::diagnostics::Checked::project(&[path], &std::collections::BTreeMap::new(), None)
                .expect("loads")
                .diagnose();
        assert!(
            !items.iter().any(|d| d.code == "RES001"),
            "quoting is the language's way of allowing it: {:?}",
            items.iter().map(|d| d.code.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn formatting_replaces_the_whole_buffer() {
        let text = "package P {
part def A;
}
";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 30, "method": "textDocument/formatting",
            "params": { "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                        "options": { "tabSize": 4, "insertSpaces": true } },
        }));
        let edits = replies[0]["result"].as_array().expect("one edit");
        assert_eq!(edits.len(), 1, "one edit over the buffer");
        let new_text = edits[0]["newText"].as_str().unwrap_or_default();
        assert_eq!(
            new_text,
            "package P {
    part def A;
}
"
        );
    }

    #[test]
    fn a_formatted_file_gets_no_edits() {
        // The protocol reads an empty answer as "nothing to do", and an editor that received an
        // identical replacement would still mark the file modified.
        let text = "package P {
    part def A;
}
";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 31, "method": "textDocument/formatting",
            "params": { "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                        "options": { "tabSize": 4, "insertSpaces": true } },
        }));
        assert_eq!(replies[0]["result"], Value::Null, "nothing to change");
    }

    #[test]
    fn formatting_keeps_a_comment() {
        let text = "package P {
// keep me
part def A;
}
";
        let (mut server, path) = server_with(&[("m.sysml", text)]);
        let replies = server.handle(&json!({
            "jsonrpc": "2.0", "id": 32, "method": "textDocument/formatting",
            "params": { "textDocument": { "uri": diagnostics::uri_from_path(&path) },
                        "options": { "tabSize": 4, "insertSpaces": true } },
        }));
        let edits = replies[0]["result"].as_array().expect("one edit");
        let new_text = edits[0]["newText"].as_str().unwrap_or_default();
        assert!(new_text.contains("// keep me"), "{new_text}");
        assert!(new_text.contains("    // keep me"), "{new_text}");
    }
}
