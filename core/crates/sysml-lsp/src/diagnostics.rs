//! What is wrong with a model, in the form a language client draws.
//!
//! The same answers the command line gives, from the same sources: the parser's own failures
//! (`SYN001`), names nothing binds (`REF001`/`REF002`), the semantic checker's rules (`SEM…`), and
//! the ledger's provenance gate (`PROV…`). Anything this file computed itself would be a second
//! opinion about the model, and the editor and the command line would then disagree about the same
//! file — the one thing a language server must not do.
//!
//! Each diagnostic carries a file and a byte span, not just a line. A client that knows only the
//! line cannot put the caret on the name that is wrong, and in a multi-file project it cannot even
//! tell which file the problem is in.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sysml_model::{Element, TextRange};
use sysml_resolve::{Index, Resolution};
use sysml_syntax::Loaded;

/// One problem, addressed by file and byte span.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnosis {
    /// Index into the buffers the loader was handed.
    pub file: usize,
    pub severity: Severity,
    pub code: String,
    pub message: String,
    /// Byte range in that buffer. Absent when the source could not be read at all.
    pub span: Option<TextRange>,
    /// The line, for the sources that name one and no element (`PROV018` is about the ledger).
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    /// Something to know, not something to fix. The protocol calls this `Information`, and a
    /// client shows it without a squiggle.
    Info,
}

/// What the ledger says about the model, split by the two situations that need different answers.
enum Provenance {
    /// Nothing has been recorded: `values` account for no entry and `parts` have no decision.
    NotStarted { values: usize, parts: usize },
    /// The ledger exists, and these are the places it disagrees with the model.
    Started(Vec<Diagnosis>),
}

/// A checked project: the tree, the buffers it was built from, and the ledger that belongs to it.
pub struct Checked {
    pub loaded: Loaded,
    /// Parallel to the file index in every span: the loader numbers its buffers in the order it
    /// was handed them, so this order turns "file 2, byte 40" into a URI.
    pub paths: Vec<PathBuf>,
    /// The text of each buffer, as it was checked. The client's unsaved text, when it has one.
    pub texts: Vec<String>,
    pub ledger_path: PathBuf,
}

impl Checked {
    /// Read and check a set of files as one project.
    ///
    /// `overrides` supplies the text of buffers the client has open and has not saved: an editor
    /// check has to be about what the person is looking at, not about what is on disk. A path
    /// missing from it is read.
    ///
    /// # Errors
    ///
    /// Returns the operating system's or parser's message when a file cannot be read or a buffer
    /// cannot be parsed at all. A buffer the grammar merely rejects part of does not fail here:
    /// that arrives as `SYN001`, the way the command line reports it.
    pub fn project(
        sources: &[PathBuf],
        overrides: &BTreeMap<PathBuf, String>,
        ledger: Option<PathBuf>,
    ) -> Result<Self, String> {
        Self::project_against(sources, &[], overrides, None, false, ledger)
    }

    /// The same, with the library settings a project manifest can pin.
    pub fn project_against(
        sources: &[PathBuf],
        extra: &[PathBuf],
        overrides: &BTreeMap<PathBuf, String>,
        pinned_library: Option<&Path>,
        bundled: bool,
        ledger: Option<PathBuf>,
    ) -> Result<Self, String> {
        if sources.is_empty() {
            return Err("no file to check".to_string());
        }
        let texts: Vec<String> = sources
            .iter()
            .map(|path| match overrides.get(path) {
                Some(text) => Ok(text.clone()),
                None => {
                    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
                }
            })
            .collect::<Result<_, String>>()?;
        let extra_texts: Vec<String> = extra
            .iter()
            .map(|path| {
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
            })
            .collect::<Result<_, String>>()?;
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let extras: Vec<&str> = extra_texts.iter().map(String::as_str).collect();
        let loaded = if extras.is_empty() {
            sysml_syntax::load_project_at(&refs, !bundled, pinned_library)?
        } else {
            sysml_syntax::load_project_with_extra_at(&refs, &extras, pinned_library, bundled)?
        };
        let ledger_path = ledger.unwrap_or_else(|| sysml_ledger::Ledger::path_for(&sources[0]));
        Ok(Self {
            loaded,
            paths: sources.to_vec(),
            texts,
            ledger_path,
        })
    }

    /// Everything wrong with the project, in the order a reader meets it: a parse failure, then a
    /// name that binds nothing, then a rule, then the ledger.
    ///
    /// An unstarted ledger is the one case this deliberately does not report in full. `sysml check`
    /// treats it as unfinished rather than broken — it exits 3 and says "no errors; its ledger has
    /// not been started" — and the editor has to mean the same thing by the same model. Publishing
    /// every hole as an error put thirty red crosses beside a clean model and buried the problems
    /// that really were problems. So the holes become one notice, and stay listed in `sysml prov`.
    ///
    /// A ledger that exists and disagrees keeps every error: a record does exist there, and it is
    /// wrong, which is a different situation and deserves a different signal.
    pub fn diagnose(&self) -> Vec<Diagnosis> {
        let mut out = Vec::new();
        self.reserved_names(&mut out);
        self.references(&mut out);
        self.semantics(&mut out);
        match self.provenance() {
            Provenance::NotStarted { values, parts } => out.push(Diagnosis {
                file: 0,
                severity: Severity::Info,
                code: "PROV000".to_string(),
                message: format!(
                    "Provenance ledger not started: {values} value(s) and {parts} part definition(s) \
                     have no decision records. This is not a SysML validation error. \
                     Use `sysml prov '{}'` to review missing records.",
                    self.paths[0].to_string_lossy().replace('\'', "'\\''")
                ),
                span: None,
                line: 1,
            }),
            Provenance::Started(issues) => out.extend(issues),
        }
        out
    }

    /// Declarations whose bare name is a word the grammar reserves.
    ///
    /// `part frame;` parses, because the lexer hands the parser a keyword and the parser takes it
    /// as a name. The specification spells a bare name as `UnrestrictedName`, which excludes every
    /// reserved word, and `part 'frame';` is how a model says it means the word. The loader saw
    /// the authored form and recorded it, so this reads that record rather than re-deriving it.
    fn reserved_names(&self, out: &mut Vec<Diagnosis>) {
        for reserved in &self.loaded.reserved_names {
            let word = &reserved.word;
            out.push(Diagnosis {
                file: reserved.file,
                severity: Severity::Error,
                code: "RES001".to_string(),
                message: format!(
                    "`{word}` is a reserved word and cannot be a name; write `'{word}'` to use it as one"
                ),
                span: Some(reserved.span),
                line: reserved.line,
            });
        }
    }

    /// Names the resolver could not bind.
    ///
    /// The command line reports these (`REF001` for a type, `REF002` for a predicate) and the
    /// language server did not, so a model that imported a namespace that is not there looked
    /// clean in the editor and failed on the command line. The exclusion is the command line's:
    /// a language function is not a declaration the author forgot.
    fn references(&self, out: &mut Vec<Diagnosis>) {
        let index = Index::build(&self.loaded.roots).with_library(&self.loaded.library);
        for reference in index.references(&self.loaded.roots) {
            let what = match &reference.target {
                Resolution::Found(_) => continue,
                Resolution::Ambiguous(ids) => {
                    let candidates: Vec<_> =
                        ids.iter().map(|id| index.qualified[*id].as_str()).collect();
                    let distinct: std::collections::BTreeSet<_> =
                        candidates.iter().copied().collect();
                    let hint = if distinct.len() == candidates.len() {
                        "Use a qualified name to select one."
                    } else {
                        "Rename the duplicate declarations to make the reference unambiguous."
                    };
                    format!(
                        "`{}` matches multiple elements: {}. {hint}",
                        reference.text,
                        distinct
                            .iter()
                            .map(|name| format!("`{name}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
                Resolution::Unresolved => {
                    format!("Cannot resolve `{}` in this scope.", reference.text)
                }
                Resolution::NotAccessible => format!(
                    "Cannot access `{}` through this path. Use dot notation to access a feature.",
                    reference.text
                ),
            };
            if sysml_semantics::is_builtin(&reference.text) {
                continue;
            }
            let code = if reference.in_predicate {
                "REF002"
            } else {
                "REF001"
            };
            // The span of the name as written, when the element kept one. A typing clause, both
            // ends of a `satisfy` and every `:>` target are in `ref_spans`; a predicate name is
            // not, because the expression tree carries no position, so those fall back to the
            // declaration's own name.
            let span = self.span_of(
                index.qualified.get(reference.from).map(String::as_str),
                &reference.text,
            );
            out.push(Diagnosis {
                file: span.map(|s| s.file).unwrap_or(0),
                severity: Severity::Error,
                code: code.to_string(),
                message: what,
                span,
                line: index.line.get(reference.from).copied().unwrap_or(1),
            });
        }
    }

    /// The rules in `sysml-semantics`.
    ///
    /// One of them is special. `SYN001` names no element — its `at` is literally `syntax error`,
    /// because the parser had nothing to name — so it is placed by its line, on the unsupported
    /// element the parser recorded there. That element's span is exactly the text it could not
    /// read, which is where the caret belongs.
    fn semantics(&self, out: &mut Vec<Diagnosis>) {
        let report = sysml_semantics::check(&self.loaded.roots, &self.loaded.library);
        for issue in &report.issues {
            let element = if issue.code == "SYN001" {
                unsupported_at(&self.loaded.roots, issue.line)
            } else {
                self.find(&issue.at)
            };
            // The span the element kept: its authored name, or — for a member the grammar
            // rejected — the text it stopped at. A value clause is the last resort, so a problem
            // about `attribute x = <expr>` lands on the expression rather than the declaration.
            let span = element.and_then(|el| el.name_span.or(el.value_span));
            out.push(Diagnosis {
                file: span.map(|s| s.file).unwrap_or(0),
                severity: match issue.severity {
                    sysml_semantics::Severity::Error => Severity::Error,
                    sysml_semantics::Severity::Warning => Severity::Warning,
                },
                code: issue.code.to_string(),
                message: issue.message.clone(),
                span,
                line: issue.line,
            });
        }
    }

    /// The provenance gate: what the ledger has nothing to say about.
    ///
    /// These are why a model passes `check` and then fails the gate. Only the canvas knew about
    /// them, so a person editing in an editor saw no problem and then found the model refused.
    /// An issue names a qualified name or the ledger itself; it is anchored to the element's name
    /// when there is one and to the first buffer's first line when there is not.
    ///
    /// The result separates the two situations a caller has to tell apart: nothing recorded yet, or
    /// a record that disagrees with the model. Only the second is a set of problems.
    fn provenance(&self) -> Provenance {
        let Ok(ledger) = sysml_ledger::Ledger::load(&self.ledger_path) else {
            return Provenance::Started(Vec::new());
        };
        let audit = sysml_ledger::audit(&self.loaded.roots, &ledger);
        if audit.not_started() {
            return Provenance::NotStarted {
                values: audit
                    .issues
                    .iter()
                    .filter(|issue| issue.code == "PROV001")
                    .count(),
                parts: audit.unaccounted_parts.len(),
            };
        }
        let issues = audit
            .issues
            .iter()
            .map(|issue| {
                let element = self.find(&issue.at);
                let span = element.and_then(|el| el.name_span);
                Diagnosis {
                    file: span.map(|s| s.file).unwrap_or(0),
                    severity: match issue.severity {
                        sysml_ledger::Severity::Error => Severity::Error,
                        sysml_ledger::Severity::Warning => Severity::Warning,
                    },
                    code: issue.code.to_string(),
                    message: issue.message.clone(),
                    span,
                    line: element.map(|el| el.line).unwrap_or(1),
                }
            })
            .collect();
        Provenance::Started(issues)
    }

    /// The element a qualified name denotes, when the project has one.
    pub fn find(&self, qualified: &str) -> Option<&Element> {
        fn walk<'a>(el: &'a Element, path: &mut Vec<&'a str>, want: &str) -> Option<&'a Element> {
            let anonymous = el.name == "<anonymous>";
            if !anonymous {
                path.push(&el.name);
            }
            if path.join("::") == want {
                return Some(el);
            }
            for child in &el.children {
                if let Some(found) = walk(child, path, want) {
                    return Some(found);
                }
            }
            if !anonymous {
                path.pop();
            }
            None
        }
        for root in &self.loaded.roots {
            if let Some(found) = walk(root, &mut Vec::new(), qualified) {
                return Some(found);
            }
        }
        None
    }

    /// The span of a reference named `text` inside the element known by `owner`.
    fn span_of(&self, owner: Option<&str>, text: &str) -> Option<TextRange> {
        let element = owner.and_then(|key| self.find(key))?;
        position_of(element, text).or(element.name_span)
    }
}

/// The element the parser recorded for a construct it could not read on `line`.
///
/// A member the grammar rejected lowers to a [`sysml_model::Kind::Unsupported`] element named
/// `syntax error`, carrying the parser's message in `constraint` and the rejected text in
/// `name_span`. There may be several on one line and they differ only by span, so the first is
/// taken: they are all equally the right place to point at.
fn unsupported_at(roots: &[Element], line: u32) -> Option<&Element> {
    fn walk<'a>(el: &'a Element, line: u32, found: &mut Option<&'a Element>) {
        if found.is_some() {
            return;
        }
        if el.kind == sysml_model::Kind::Unsupported && el.name == "syntax error" && el.line == line
        {
            *found = Some(el);
            return;
        }
        for child in &el.children {
            walk(child, line, found);
        }
    }
    let mut found = None;
    for root in roots {
        walk(root, line, &mut found);
    }
    found
}

/// The span a reference with this text occupies inside one element.
///
/// A qualified spelling (`SI::m`) is joined from segments in the tree, so it is looked up by its
/// first segment: that is the part a reader needs marked, and the rest is the same expression.
fn position_of(el: &Element, text: &str) -> Option<TextRange> {
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

/// The file a URI names, as a path this process can read.
pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    // The query and the fragment are not part of the path. A client that adds one — VS Code does
    // on some requests — would otherwise be answered about a file whose name has `?…` in it.
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    // The drive colon arrives percent-encoded from VS Code (`file:///c%3A/dir/a.sysml`), so the
    // Windows test has to run on the decoded text. Testing the raw bytes read `%3A` as anything but
    // a colon, decided the path was a POSIX one, and produced `/c:/...` — a path with no drive and
    // a leading slash, which every file lookup then failed to open. The outline was empty because
    // of this, and so was every other request that reads a file.
    let decoded = percent_decode(rest);
    // `file:///C:/dir/a.sysml` is Windows; `file:///home/a.sysml` is not. The difference is the
    // drive letter after the leading slash, and stripping one slash too many would turn the first
    // into a relative path.
    let bytes = decoded.as_bytes();
    let windows = bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':';
    let body = if windows { &decoded[1..] } else { &decoded[..] };
    Some(PathBuf::from(body))
}

/// The URI a path is addressed by, in the form a client compares.
pub fn uri_from_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let body = if text.starts_with('/') {
        text
    } else {
        format!("/{text}")
    };
    format!("file://{}", encode(&body))
}

/// Percent-encode what a URI cannot carry literally, leaving the syntax alone.
///
/// Encoding the whole path would turn `C:/a/b` into `C%3A/a/b`, which no client accepts for a
/// Windows drive. Only the characters a space or a non-ASCII byte would break are escaped.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // `i + 2 < len` left the escape at the very end of the string undecoded. A `%3A` that
        // happens to land there stays three literal characters, which is a different path.
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// The zero-based line and UTF-16 column a byte offset falls on, as the protocol counts them.
pub fn line_column(text: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(text.len());
    let mut line = 0u32;
    let mut start = 0usize;
    for (at, byte) in text.bytes().enumerate() {
        if at >= offset {
            break;
        }
        if byte == b'\n' {
            line += 1;
            start = at + 1;
        }
    }
    // Columns are UTF-16 code units, not bytes: a character outside the BMP counts twice, and an
    // editor that counted bytes would put the squiggle in the wrong place.
    let column = text[start..offset].encode_utf16().count() as u32;
    (line, column)
}

/// The byte offset a zero-based line and UTF-16 column name.
pub fn offset_of(text: &str, line: u32, column: u32) -> Option<usize> {
    // The start of `line`. Walking newlines counts them: line 0 begins at byte 0, and the first
    // newline ends line 0 rather than starting it.
    let mut at = 0usize;
    let mut remaining = line;
    while remaining > 0 {
        match text[at..].find('\n') {
            Some(offset) => at += offset + 1,
            None => return None,
        }
        remaining -= 1;
    }
    let mut units = 0u32;
    for (index, character) in text[at..].char_indices() {
        if character == '\n' || units >= column {
            return Some(at + index);
        }
        units += character.len_utf16() as u32;
    }
    Some(text.len())
}

/// A fresh directory for one test.
///
/// The suite runs in parallel, so two tests sharing a path race each other: the tag alone is not
/// unique, and one test's `remove_dir_all` deletes the model the other has just written.
///
/// Under the workspace's own `target/tmp` rather than the system temporary directory: this machine
/// refuses `create_dir_all` beneath `%TEMP%` (an access error even for the owner), and every test
/// that writes a file failed for it. The path is derived from this crate's manifest so a checkout
/// anywhere works.
#[cfg(test)]
pub(crate) fn scratch(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("tmp");
    let dir = base.join(format!(
        "sysml-lsp-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("temp dir {dir:?}: {e}"));
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(text: &str) -> Vec<Diagnosis> {
        let dir = scratch("diag");
        let path = dir.join("m.sysml");
        std::fs::write(&path, text).expect("write");
        Checked::project(&[path], &BTreeMap::new(), None)
            .expect("loads")
            .diagnose()
    }

    fn codes(items: &[Diagnosis]) -> Vec<&str> {
        items.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn a_clean_model_with_no_ledger_gets_one_notice_not_a_red_cross_per_value() {
        // The command line calls this situation "unfinished", not broken: it exits 3 and prints
        // "no errors; its ledger has not been started". The editor used to publish every hole as
        // an error, which put thirty red crosses on a model with nothing wrong with it.
        let items = check(
            "package P {\n  private import ScalarValues::*;\n  part def A { attribute x : Real = 1; }\n}\n",
        );
        assert_eq!(codes(&items), ["PROV000"], "{:?}", codes(&items));
        assert_eq!(items[0].severity, Severity::Info);
        assert!(items[0].message.starts_with(
            "Provenance ledger not started: 1 value(s) and 1 part definition(s) have no decision records."
        ), "{}", items[0].message);
        assert!(items[0].message.contains("not a SysML validation error"));
        assert!(!items[0].message.contains('…'));
    }

    #[test]
    fn the_ledger_notice_quotes_the_actual_model_path() {
        let dir = scratch("ledger-path");
        let path = dir.join("operator's model.sysml");
        std::fs::write(&path, "package P { part def A; }\n").expect("write");
        let checked = Checked::project(&[path.clone()], &BTreeMap::new(), None).expect("loads");
        let items = checked.diagnose();
        let notice = items.iter().find(|d| d.code == "PROV000").expect("notice");
        let command = format!(
            "`sysml prov '{}'`",
            path.to_string_lossy().replace('\'', "'\\''")
        );
        assert!(notice.message.contains(&command), "{}", notice.message);
        assert!(notice
            .message
            .contains("0 value(s) and 1 part definition(s)"));
    }

    #[test]
    fn a_ledger_that_exists_and_disagrees_still_reports_every_hole() {
        // The other situation, and the one that must not be softened: a record does exist, and it
        // is wrong. `not_started` is false as soon as any value is accounted for, so a ledger with
        // a scope and one decision still leaves the rest of the model unaccounted — as errors.
        let dir = scratch("ledger-started");
        let path = dir.join("m.sysml");
        let text = "package P {\n  private import ScalarValues::*;\n  part def A { attribute x : Real = 1; attribute y : Real = 2; }\n}\n";
        std::fs::write(&path, text).expect("write");
        // `decide` is the only way to account for a value, and it goes through the ledger's own
        // API rather than being hand-written here, so this test cannot drift from the file format.
        let ledger_path = sysml_ledger::Ledger::path_for(&path);
        let mut ledger = sysml_ledger::Ledger::default();
        ledger.jurisdictions = "US".to_string();
        ledger.decisions.push(sysml_ledger::Decision {
            id: ledger.next_decision_id(),
            element: "P::A::x".to_string(),
            value: "1".to_string(),
            origin: sysml_ledger::Origin::Spec,
            why: "a spec says so".to_string(),
            falsifier: String::new(),
            inputs: Vec::new(),
            question: None,
            decided_by: "agent".to_string(),
            approved_by: None,
            cites: Vec::new(),
        });
        ledger.save(&ledger_path).expect("write the ledger");
        let items = Checked::project(&[path], &BTreeMap::new(), None)
            .expect("loads")
            .diagnose();
        assert!(
            !items.iter().any(|d| d.code == "PROV000"),
            "the ledger has been started, so there is nothing to notice: {:?}",
            codes(&items)
        );
        let missing = items
            .iter()
            .find(|d| d.code == "PROV001")
            .expect("unrecorded value");
        assert_eq!(missing.severity, Severity::Error);
        assert!(!missing.message.contains("error[PROV"));
        assert!(!missing.message.starts_with("P::A::y:"));
        assert!(missing.span.is_some());
    }

    #[test]
    fn an_unresolved_type_is_reported_with_its_span() {
        let items = check("package P {\n  part def A { part b : Nowhere; }\n}\n");
        let found = items
            .iter()
            .find(|d| d.code == "REF001")
            .expect("the type nothing binds");
        assert_eq!(found.message, "Cannot resolve `Nowhere` in this scope.");
        let span = found.span.expect("a span on the name");
        assert_eq!(span.file, 0);
    }

    #[test]
    fn a_predicate_reference_uses_the_same_resolution_wording() {
        let items = check("package P { requirement def R { require constraint { missing > 0 } } }");
        let found = items
            .iter()
            .find(|d| d.code == "REF002")
            .expect("predicate reference");
        assert_eq!(found.message, "Cannot resolve `missing` in this scope.");
        assert_eq!(found.severity, Severity::Error);
    }

    #[test]
    fn an_ambiguous_reference_lists_the_resolvers_candidates() {
        let items = check("package P { part def Wheel; part def Wheel; part wheel : Wheel; }");
        let found = items
            .iter()
            .find(|d| d.code == "REF001")
            .expect("ambiguous type");
        assert_eq!(
            found.message,
            "`Wheel` matches multiple elements: `P::Wheel`. Rename the duplicate declarations to make the reference unambiguous."
        );
        assert_eq!(found.severity, Severity::Error);
    }

    #[test]
    fn every_parse_failure_is_reported_and_none_twice() {
        // The line holds two tokens the grammar cannot place, and the parser reports each. The
        // count the command line gives for this buffer is two, so two is what the editor gets: the
        // question is not "one squiggle" but "one squiggle per thing the parser rejected", with no
        // duplicate from the server reporting the same element a second time.
        let items = check("package P {\n  this is not sysml\n}\n");
        let syntax: Vec<&Diagnosis> = items.iter().filter(|d| d.code == "SYN001").collect();
        assert_eq!(syntax.len(), 2, "{:?}", codes(&items));
        assert_ne!(
            syntax[0].message, syntax[1].message,
            "two reports of one failure would read as two failures"
        );
        assert!(
            syntax.iter().all(|d| d.span.is_some()),
            "each one names its span"
        );
    }

    #[test]
    fn one_unreadable_token_is_one_diagnosis() {
        // A single token the grammar cannot place. `package P { @ }` has exactly one, which is
        // what makes this a test of duplication rather than of the parser's recovery count.
        let items = check("package P {\n  @\n}\n");
        let syntax: Vec<&Diagnosis> = items.iter().filter(|d| d.code == "SYN001").collect();
        assert_eq!(syntax.len(), 1, "{:?}", codes(&items));
    }

    #[test]
    fn provenance_is_reported_too() {
        let items = check(
            "package P {\n  private import ScalarValues::*;\n  part def A { attribute x : Real = 1; }\n}\n",
        );
        assert!(
            items.iter().any(|d| d.code.starts_with("PROV")),
            "a value with no ledger entry is a problem in the editor: {:?}",
            codes(&items)
        );
    }

    #[test]
    fn an_unsaved_buffer_is_what_gets_checked() {
        let dir = scratch("buf");
        let path = dir.join("b.sysml");
        std::fs::write(&path, "package P {\n}\n").expect("write");
        let mut overrides = BTreeMap::new();
        overrides.insert(
            path.clone(),
            "package P {\n  part def A { part b : Nowhere; }\n}\n".to_string(),
        );
        let items = Checked::project(&[path], &overrides, None)
            .expect("loads")
            .diagnose();
        assert!(
            items.iter().any(|d| d.code == "REF001"),
            "the unsaved text is checked: {:?}",
            codes(&items)
        );
    }

    #[test]
    fn a_uri_round_trips_through_a_path() {
        for uri in [
            "file:///C:/a%20b/c.sysml",
            "file:///home/me/a.sysml",
            // What VS Code actually sends on Windows: the drive colon percent-encoded.
            "file:///c%3A/a%20b/c.sysml",
        ] {
            let path = path_from_uri(uri).expect("a path");
            let back = uri_from_path(&path);
            assert_eq!(path_from_uri(&back).as_deref(), Some(path.as_path()));
        }
    }

    #[test]
    fn an_encoded_drive_colon_still_names_an_absolute_path() {
        // The shape VS Code sends. Read as POSIX, it becomes `/c:/x/a.sysml` — absolute, on no
        // drive, and unopenable, which is what emptied the outline in the editor.
        let path = path_from_uri("file:///c%3A/x/a.sysml").expect("a path");
        let text = path.to_string_lossy().replace('\\', "/");
        assert!(!text.starts_with('/'), "no leading slash: {path:?}");
        assert!(
            text.to_ascii_uppercase().starts_with("C:"),
            "the drive letter survives: {path:?}"
        );
    }

    #[test]
    fn a_query_after_the_path_is_not_part_of_it() {
        let path = path_from_uri("file:///c%3A/x/a.sysml?1").expect("a path");
        assert!(path.to_string_lossy().ends_with("a.sysml"), "{path:?}");
    }

    #[test]
    fn an_escape_at_the_very_end_is_still_decoded() {
        assert_eq!(percent_decode("a%3A"), "a:");
        assert_eq!(percent_decode("%20"), " ");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn a_windows_drive_keeps_its_letter() {
        let path = path_from_uri("file:///C:/x/a.sysml").expect("a path");
        assert!(path.to_string_lossy().ends_with("a.sysml"), "{path:?}");
        assert!(
            path.to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("C:"),
            "the drive letter survives: {path:?}"
        );
    }

    #[test]
    fn columns_count_utf16_units() {
        let (line, column) = line_column("温度 = 1;\n", "温度".len());
        assert_eq!(line, 0);
        assert_eq!(column, 2);
    }

    #[test]
    fn a_second_line_is_numbered() {
        let (line, column) = line_column("a;\nbc;\n", 4);
        assert_eq!(line, 1);
        assert_eq!(column, 1);
    }

    #[test]
    fn an_offset_round_trips_through_a_position() {
        for text in ["a;\nbc;\n", "温度 = 1;\npart def A;\n"] {
            // Every boundary, not a sample: the bug this pins was line 0 being read as the start
            // of line 1, which a sample of offsets skipped over.
            for offset in 0..=text.len() {
                if !text.is_char_boundary(offset) {
                    continue;
                }
                let (line, column) = line_column(text, offset);
                assert_eq!(
                    offset_of(text, line, column),
                    Some(offset),
                    "{text:?} at offset {offset} = line {line} column {column}"
                );
            }
        }
    }

    #[test]
    fn the_first_line_starts_at_byte_zero() {
        assert_eq!(offset_of("abc\ndef\n", 0, 0), Some(0));
        assert_eq!(offset_of("abc\ndef\n", 0, 2), Some(2));
        assert_eq!(offset_of("abc\ndef\n", 1, 0), Some(4));
    }
}
