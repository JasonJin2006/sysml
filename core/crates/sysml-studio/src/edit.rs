//! Turning an edit made on the canvas into a change in the source.
//!
//! The rule this file exists to enforce: **the canvas never becomes a second copy of the model.**
//! A person drags a box or types a number, and what lands on disk is a diff to the `.sysml` text
//! they already maintain. Nothing here writes a model of its own, and nothing here keeps state
//! between requests — it reads the buffers, applies a text edit, reparses, and either commits or
//! reports why it refused.
//!
//! Two consequences shape the code:
//!
//! * **Every edit is verified by reparsing.** A rename that leaves a reference dangling, a value
//!   that does not parse, an insertion in the wrong place: all of them are caught before the file
//!   is written, by running the same loader the CLI runs. An editor that trusts its own string
//!   surgery is an editor that corrupts a model sooner or later.
//! * **The failure mode is "nothing happened".** An edit is applied to an in-memory copy, checked,
//!   and only then written. A rejected edit leaves the file byte-identical.

use std::collections::BTreeSet;
use std::path::PathBuf;

use sysml_model::{Element, Kind, TextRange};
use sysml_syntax::{load_project, Loaded};

use crate::conflict;

/// The source files of one project, in the order the loader sees them.
#[derive(Debug, Clone)]
pub struct Sources {
    pub paths: Vec<PathBuf>,
    pub texts: Vec<String>,
    /// The digest each file had when it was read, parallel to `paths`.
    ///
    /// Carried on every buffer so the write can ask "is this still what I read?" rather than
    /// "did somebody write recently?". See `conflict` for why a digest and not an mtime.
    pub digests: Vec<conflict::Digest>,
}

impl Sources {
    pub fn read(paths: &[PathBuf]) -> Result<Self, String> {
        let mut texts = Vec::new();
        for path in paths {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            texts.push(text);
        }
        let digests = texts.iter().map(|t| conflict::Digest::of(t)).collect();
        Ok(Self {
            paths: paths.to_vec(),
            texts,
            digests,
        })
    }

    pub fn load(&self) -> Result<Loaded, String> {
        let refs: Vec<&str> = self.texts.iter().map(String::as_str).collect();
        load_project(&refs, true)
    }

    /// Apply one replacement per buffer, from the end backwards so earlier offsets stay valid.
    fn apply(&self, mut edits: Vec<Edit>) -> Result<Self, String> {
        if edits.is_empty() {
            return Ok(self.clone());
        }
        let mut texts = self.texts.clone();
        for file in 0..texts.len() {
            let mut mine: Vec<&Edit> = edits.iter().filter(|e| e.range.file == file).collect();
            if mine.is_empty() {
                continue;
            }
            // Overlapping edits mean the caller reasoned about a stale tree: two changes to one
            // region cannot both be what the author meant, and applying both silently depends on
            // the order they happened to arrive in.
            mine.sort_by_key(|e| e.range.start);
            for pair in mine.windows(2) {
                if pair[0].range.end > pair[1].range.start {
                    return Err(format!(
                        "two edits overlap in {}: [{}..{}] and [{}..{}]",
                        self.paths[file].display(),
                        pair[0].range.start,
                        pair[0].range.end,
                        pair[1].range.start,
                        pair[1].range.end
                    ));
                }
            }
            let text = &mut texts[file];
            for edit in mine.into_iter().rev() {
                text.replace_range(edit.range.start..edit.range.end, &edit.text);
            }
        }
        edits.clear();
        // The edited buffer has not been written yet, so its digests still describe the files as
        // they were read. That is what the write compares against.
        Ok(Self {
            paths: self.paths.clone(),
            texts,
            digests: self.digests.clone(),
        })
    }

    /// Write every buffer, refusing if a file changed since it was read.
    ///
    /// The check runs for every file *before* the first write, so a conflict on the second file
    /// cannot leave the first one already overwritten: the failure mode has to stay "nothing
    /// happened", not "half happened".
    fn write(&self) -> Result<(), String> {
        for (path, digest) in self.paths.iter().zip(&self.digests) {
            match conflict::Fingerprint::now(path) {
                Some(now) if now == *digest => {}
                Some(_) => {
                    return Err(conflict::Conflict::Changed { path: path.clone() }.message());
                }
                None => {
                    return Err(conflict::Conflict::Missing { path: path.clone() }.message());
                }
            }
        }
        for (path, text) in self.paths.iter().zip(&self.texts) {
            std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Edit {
    pub range: TextRange,
    pub text: String,
}

impl Edit {
    pub fn replace(range: TextRange, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }

    /// Insert `text` immediately after `range`.
    pub fn after(range: TextRange, text: impl Into<String>) -> Self {
        Self {
            range: TextRange {
                start: range.end,
                ..range
            },
            text: text.into(),
        }
    }
}

/// A change the canvas asked for, and what came of it.
pub struct Outcome {
    /// The buffer as it was before, for an undo the caller can offer.
    pub before: Sources,
    pub after: Sources,
    /// Qualified names of elements the edit touched, so a caller can keep a selection.
    pub touched: Vec<String>,
}

/// The result of a refused edit, phrased for the person who made it.
///
/// `Debug` prints the message rather than the tuple: a failing test should show what the model
/// said, not `Refused("...")`, and the message is the whole value here.
#[derive(Clone, PartialEq, Eq)]
pub struct Refused(pub String);

impl std::fmt::Debug for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Replace one whole buffer, as an editor does when the page sends its text back.
///
/// The editor cannot express its change as an element edit: it holds the file, not the tree, and a
/// person may add a declaration, delete one, or reorder them. So the change is one edit covering
/// the buffer and the new text. Everything else matches `rename`: the result is parsed before
/// anything is written, so a buffer that does not parse is refused rather than saved.
///
/// # Errors
///
/// Returns [`Refused`] when the file index is out of range, or when the new text does not parse
/// into a model.
pub fn replace_buffer(
    sources: &Sources,
    file: usize,
    text: impl Into<String>,
) -> Result<Outcome, Refused> {
    let text = text.into();
    let Some(current) = sources.texts.get(file) else {
        return Err(Refused(format!(
            "no file at index {file}; the model has {}",
            sources.texts.len()
        )));
    };
    let edit = Edit::replace(
        TextRange {
            file,
            start: 0,
            end: current.len(),
        },
        text,
    );
    let candidate = sources.apply(vec![edit]).map_err(Refused)?;
    // Parse before answering, and refuse a buffer that does not parse.
    //
    // `load` does not fail on a syntax error: it records one `Kind::Unsupported` element named
    // `syntax error` and carries on, because a checker has to be able to report the error. That
    // tolerance is right for reading and wrong for writing — a buffer that does not parse must
    // stay in the editor, where its author can see the problem, and never reach the disk.
    let loaded = candidate.load().map_err(Refused)?;
    if let Some(bad) = first_syntax_error(&loaded.roots) {
        return Err(Refused(format!("line {}: {}", bad.0, bad.1)));
    }
    Ok(Outcome {
        before: sources.clone(),
        after: candidate,
        touched: Vec::new(),
    })
}

/// The first `syntax error` element in the tree, as its line and message.
fn first_syntax_error(roots: &[Element]) -> Option<(u32, String)> {
    fn walk(el: &Element) -> Option<(u32, String)> {
        if el.kind == Kind::Unsupported && el.name == "syntax error" {
            return Some((el.line, el.constraint.clone().unwrap_or_default()));
        }
        el.children.iter().find_map(walk)
    }
    roots.iter().find_map(walk)
}

/// Every element, with the qualified name a request can address it by.
struct Flat {
    elements: Vec<(String, Element)>,
}

impl Flat {
    fn build(roots: &[Element]) -> Self {
        fn walk(el: &Element, path: &mut Vec<String>, out: &mut Vec<(String, Element)>) {
            path.push(el.name.clone());
            out.push((path.join("::"), el.clone()));
            for child in &el.children {
                walk(child, path, out);
            }
            path.pop();
        }
        let mut elements = Vec::new();
        for root in roots {
            let mut path = Vec::new();
            walk(root, &mut path, &mut elements);
        }
        Self { elements }
    }

    fn by_qualified(&self, name: &str) -> Option<&Element> {
        self.elements
            .iter()
            .find(|(q, _)| q == name)
            .map(|(_, el)| el)
    }
}

/// Rename one element, everywhere its old name is a reference to it.
///
/// The reference sweep is deliberately conservative: a token is rewritten only when it is a
/// single-segment name that the resolver binds to exactly this element from the scope it was
/// written in. Renaming a name by text substitution would also hit a same-named feature of an
/// unrelated type, and the model would still parse — which is the worst kind of wrong.
///
/// # Errors
///
/// Returns a [`Refused`] message when the new name is empty, is not a valid identifier, is
/// already used in the same scope, or when no element has this qualified name.
pub fn rename(sources: &Sources, qualified: &str, new_name: &str) -> Result<Outcome, Refused> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err(Refused("a name cannot be empty".to_string()));
    }
    if !new_name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
    {
        return Err(Refused(format!(
            "`{new_name}` is not a plain name; use letters, digits, underscore or spaces"
        )));
    }
    let loaded = sources.load().map_err(Refused)?;
    let flat = Flat::build(&loaded.roots);
    let (old_qualified, target) = flat
        .elements
        .iter()
        .find(|(q, _)| q == qualified)
        .map(|(q, el)| (q.clone(), el.clone()))
        .ok_or_else(|| Refused(format!("no element named `{qualified}`")))?;
    let Some(span) = target.name_span else {
        return Err(Refused(format!(
            "`{qualified}` was not lowered with a name span, so it cannot be renamed safely"
        )));
    };
    let old_name = target.name.clone();
    if old_name == new_name {
        return Ok(Outcome {
            before: sources.clone(),
            after: sources.clone(),
            touched: Vec::new(),
        });
    }

    let mut edits = vec![Edit::replace(span, new_name)];

    // Rewrite the references that resolve to this element. The resolver is run on the *unedited*
    // tree, where each reference still binds, and every typing clause whose target is the renamed
    // element gets its authored range replaced. Doing it by resolved target rather than by text
    // is what keeps `part a : Battery` in one package from being rewritten when a different
    // `Battery` is the one being renamed.
    let index = sysml_resolve::Index::build(&loaded.roots);
    let target_id = target.id;
    for (_, element) in &flat.elements {
        for (text, span) in &element.ref_spans {
            // A dotted or qualified reference names the element by a path; rewriting only the
            // leaf is correct because the rename changes the leaf and nothing else.
            if !resolves_to(&index, element.id, text, target_id) {
                continue;
            }
            let leaf = text.rsplit("::").next().unwrap_or(text).to_string();
            if let Some(range) = leaf_range(sources, *span, &leaf) {
                edits.push(Edit::replace(range, new_name));
            }
        }
    }

    let candidate = sources.apply(edits).map_err(Refused)?;
    let reloaded = candidate.load().map_err(Refused)?;
    let after = Flat::build(&reloaded.roots);
    let renamed_key = new_qualified(&old_qualified, &old_name, new_name);
    if after.by_qualified(&renamed_key).is_none() {
        return Err(Refused(format!(
            "renaming `{old_name}` to `{new_name}` did not produce an element with that name"
        )));
    }
    // A rename that leaves a reference dangling is refused. This is the check that makes the
    // whole sweep safe: if some reference still says the old name and no longer resolves, the
    // person is told before the file changes rather than discovering it in `check` later.
    let after_index = sysml_resolve::Index::build(&reloaded.roots);
    let mut dangling = Vec::new();
    for reference in after_index.references(&reloaded.roots) {
        if matches!(reference.target, sysml_resolve::Resolution::Unresolved) {
            if reference.text.split("::").last() == Some(old_name.as_str())
                || reference.text == old_name
            {
                dangling.push(reference.text.clone());
            }
        }
    }
    if !dangling.is_empty() {
        dangling.sort();
        dangling.dedup();
        return Err(Refused(format!(
            "`{old_name}` is still referenced as {}; renaming would leave that unresolved",
            dangling.join(", ")
        )));
    }
    let touched = vec![renamed_key];
    Ok(Outcome {
        before: sources.clone(),
        after: candidate,
        touched,
    })
}

/// Whether the reference `name` written inside element `from` binds to `target`.
fn resolves_to(index: &sysml_resolve::Index, from: usize, name: &str, target: usize) -> bool {
    // A typed feature's reference may be qualified (`A::B`); the resolver walks it whole. A
    // feature may also be typed by an element whose name it only reaches through a supertype,
    // which the resolver already handles, so a plain resolve is the right question to ask.
    match index.resolve(from, name) {
        sysml_resolve::Resolution::Found(id) => id == target,
        _ => false,
    }
}

/// The sub-range of a typing clause that holds the leaf segment.
///
/// `type_span` covers the whole authored reference. The leaf starts after the last `::` in the
/// written text, which is what a rename replaces: a person renaming `MassValue` should not turn
/// `ISQ::MassValue` into `ISQ::Weight` on the wrong segment, nor drop the qualifier.
fn leaf_range(
    sources: &Sources,
    span: sysml_model::TextRange,
    leaf: &str,
) -> Option<sysml_model::TextRange> {
    let text = sources.texts.get(span.file)?.get(span.start..span.end)?;
    let at = text.rfind("::").map(|i| i + 2).unwrap_or(0);
    let leaf_text = &text[at..];
    if leaf_text.trim() != leaf {
        return None;
    }
    // Preserve the quotes an unrestricted name carries by replacing only the inner token.
    let quoted = leaf_text.starts_with('\'') && leaf_text.ends_with('\'');
    let offset = at + if quoted { 1 } else { 0 };
    let length = if quoted {
        leaf_text.len() - 2
    } else {
        leaf_text.len()
    };
    Some(sysml_model::TextRange {
        file: span.file,
        start: span.start + offset,
        end: span.start + offset + length,
    })
}

fn new_qualified(old_qualified: &str, old_name: &str, new_name: &str) -> String {
    match old_qualified.rsplit_once("::") {
        Some((prefix, _)) => format!("{prefix}::{new_name}"),
        None => {
            debug_assert_eq!(old_qualified, old_name);
            new_name.to_string()
        }
    }
}

/// Change the value clause of one feature.
///
/// Replacing the whole `= expr` rather than splicing the number is what keeps the edit legible in
/// a diff: one range, one replacement, and the type and any trailing semicolon are untouched.
///
/// # Errors
///
/// Returns a [`Refused`] message when the value is empty, when no element has this qualified
/// name, when the element has no `= expr` clause to replace, or when the written value would not
/// parse as an expression.
pub fn set_value(sources: &Sources, qualified: &str, value: &str) -> Result<Outcome, Refused> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Refused("a value cannot be empty".to_string()));
    }
    let loaded = sources.load().map_err(Refused)?;
    let flat = Flat::build(&loaded.roots);
    let target = flat
        .by_qualified(qualified)
        .ok_or_else(|| Refused(format!("no element named `{qualified}`")))?;
    let Some(span) = target.value_span else {
        return Err(Refused(format!(
            "`{qualified}` has no value clause to change"
        )));
    };
    let edit = Edit::replace(span, format!("= {value}"));
    let candidate = sources.apply(vec![edit]).map_err(Refused)?;
    let reloaded = candidate.load().map_err(Refused)?;
    let after = Flat::build(&reloaded.roots);
    let written = after
        .by_qualified(qualified)
        .and_then(|el| el.value_text.clone());
    // The value is checked by re-reading what the loader says the value now is. Comparing text
    // alone would accept `= 30 [min]` written into a `Real` feature that renders as `30`, where
    // the author's unit silently became part of nothing.
    match written {
        Some(text) if normalise(&text) == normalise(value) => {}
        Some(text) => {
            return Err(Refused(format!(
                "the model re-read that value as `{text}`, not `{value}`"
            )))
        }
        None => {
            return Err(Refused(format!(
                "`{qualified}` still has no value after the edit"
            )))
        }
    }
    Ok(Outcome {
        before: sources.clone(),
        after: candidate,
        touched: vec![qualified.to_string()],
    })
}

/// Whitespace-insensitive comparison, so `30[min]` and `30 [min]` are the same value.
fn normalise(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Add a declaration to the body of an existing element.
///
/// The insertion point is the closing brace of the owner's body, found by scanning the source of
/// the owner's declaration rather than by trusting an offset: the loader does not keep an
/// end-of-element span, and a synthesised insertion point that is a few bytes off corrupts the
/// file. The scan is bounded and its result is verified by reparsing, so a wrong guess is caught.
pub fn add_member(
    sources: &Sources,
    owner: &str,
    kind: Kind,
    name: &str,
) -> Result<Outcome, Refused> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Refused("a name cannot be empty".to_string()));
    }
    let loaded = sources.load().map_err(Refused)?;
    let flat = Flat::build(&loaded.roots);
    let target = flat
        .by_qualified(owner)
        .ok_or_else(|| Refused(format!("no element named `{owner}`")))?;
    let Some(name_span) = target.name_span else {
        return Err(Refused(format!(
            "`{owner}` has no name span, so its body cannot be located"
        )));
    };
    let text = sources
        .texts
        .get(name_span.file)
        .map(|s| s.as_str())
        .unwrap_or("");
    let Some(body_close) = find_body_end(text, name_span.end) else {
        return Err(Refused(format!(
            "`{owner}` has no body to add to; only a declaration with `{{ ... }}` can hold members"
        )));
    };
    // The insertion point is the byte before the closing brace. Whatever whitespace already sits
    // between the last member and that brace belongs to the brace's own line, so the new member
    // goes *before* that whitespace and then restates the closing line's indentation. Inserting
    // after it would append the statement to the brace line and leave the brace unindented.
    let indent = line_indent(text, name_span.start);
    let child_indent = format!("{indent}    ");
    let line_start = text[..body_close].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let only_space = text[line_start..body_close]
        .chars()
        .all(|c| c == ' ' || c == '\t');
    if only_space {
        // The brace sits alone on its line, indented by the whitespace between `line_start` and
        // it. Insert the member before that whitespace and do not restate the indentation: the
        // whitespace and the brace are already there, so repeating it would push the brace one
        // level deeper on every insert.
        let statement = format!("{child_indent}{} {name};\n", kind.keyword());
        let edit = Edit {
            range: TextRange {
                file: name_span.file,
                start: line_start,
                end: line_start,
            },
            text: statement,
        };
        return add_member_verified(sources, owner, name, kind, edit);
    }
    // The brace is on the same line as content (`part x { }`). Insert before it, indented one
    // level deeper than the declaration.
    let statement = format!("\n{child_indent}{} {name};\n{indent}", kind.keyword());
    let edit = Edit {
        range: TextRange {
            file: name_span.file,
            start: body_close,
            end: body_close,
        },
        text: statement,
    };
    add_member_verified(sources, owner, name, kind, edit)
}

/// Apply an insertion and confirm the member appeared.
///
/// Split out because the two insertion shapes above differ only in where the text goes; the
/// check that the result is what was asked for has to be the same for both, or one of them can
/// produce a body that parses but does not contain the member.
fn add_member_verified(
    sources: &Sources,
    owner: &str,
    name: &str,
    kind: Kind,
    edit: Edit,
) -> Result<Outcome, Refused> {
    let candidate = sources.apply(vec![edit]).map_err(Refused)?;
    let reloaded = candidate.load().map_err(Refused)?;
    let after = Flat::build(&reloaded.roots);
    if after.by_qualified(&format!("{owner}::{name}")).is_none() {
        return Err(Refused(format!(
            "adding `{} {name}` to `{owner}` did not produce that member",
            kind.keyword()
        )));
    }
    Ok(Outcome {
        before: sources.clone(),
        after: candidate,
        touched: vec![format!("{owner}::{name}")],
    })
}

/// The byte offset of the `}` that closes the body starting after `from`.
///
/// Brace counting over source text skips string literals and comments: a `doc /* } */` would
/// otherwise close the body early and put the insertion inside a comment.
fn find_body_end(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = from;
    let mut depth = 0i32;
    let mut opened = false;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            b'}' => {
                if opened && depth == 1 {
                    return Some(i);
                }
                depth -= 1;
                i += 1;
            }
            b'{' => {
                opened = true;
                depth += 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

fn line_indent(text: &str, at: usize) -> String {
    let start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    text[start..at]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// Nothing here is used by the server directly; the functions above are the surface.
///
/// # Errors
///
/// Returns the operating system's message when the edited source cannot be written back.
pub fn commit(outcome: &Outcome) -> Result<(), String> {
    outcome.after.write()
}

/// The set of qualified names in a loaded model, for pruning a layout.
pub fn live_names(loaded: &Loaded) -> Vec<String> {
    let flat = Flat::build(&loaded.roots);
    let mut names: BTreeSet<String> = BTreeSet::new();
    for (qualified, _) in &flat.elements {
        names.insert(qualified.clone());
    }
    names.into_iter().collect()
}

#[cfg(test)]
mod buffer_tests {
    use super::*;
    use std::path::PathBuf;

    fn sources_with(text: &str) -> (PathBuf, Sources) {
        let dir = std::env::temp_dir().join(format!(
            "sysml-buffer-{}-{}",
            std::process::id(),
            text.len()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.sysml");
        std::fs::write(&path, text).expect("write");
        let sources = Sources::read(&[path.clone()]).expect("read");
        (path, sources)
    }

    #[test]
    fn a_buffer_that_parses_is_accepted_and_its_text_replaces_the_file() {
        let (_path, sources) = sources_with("package P { part def A; }\n");
        let outcome = replace_buffer(&sources, 0, "package P { part def B; }\n").expect("accepted");
        assert_eq!(outcome.after.texts[0], "package P { part def B; }\n");
        // Nothing is written until `commit`, so the refusal path and the accepted path share the
        // same "nothing happened yet" state.
        assert_eq!(
            std::fs::read_to_string(&sources.paths[0]).expect("read"),
            "package P { part def A; }\n"
        );
    }

    #[test]
    fn a_buffer_that_does_not_parse_is_refused() {
        // `load` records a syntax error as an element instead of failing, so a caller that only
        // checked `load`'s result would have written this to disk.
        let (_path, sources) = sources_with("package P { part def A; }\n");
        let refused = replace_buffer(&sources, 0, "this is not SysML {{{");
        assert!(refused.is_err(), "a broken buffer must not be saved");
        match refused {
            Ok(_) => panic!("a broken buffer must not be saved"),
            Err(message) => assert!(message.0.contains("line 1"), "{}", message.0),
        }
    }

    #[test]
    fn an_index_past_the_end_is_refused_by_name() {
        let (_path, sources) = sources_with("package P;\n");
        let refused = replace_buffer(&sources, 7, "package Q;\n");
        match refused {
            Ok(_) => panic!("an out-of-range index must be refused"),
            Err(message) => assert!(message.0.contains("no file at index 7"), "{}", message.0),
        }
    }
}
