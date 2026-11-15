//! Fixes a client can offer beside a diagnostic.
//!
//! A quick fix has one requirement the rest of this server does not: it must produce text that
//! still compiles. A rename or an edit is verified by reparsing before it is applied; a code action
//! hands the client a text edit and trusts it, so the edit has to be right by construction.
//!
//! # What is offered
//!
//! Only what can be computed exactly from what the diagnostic already says:
//!
//! * **A reserved word used as a name** (`RES001`) becomes a quoted name. The quoted spelling is
//!   the language's own answer and is legal wherever the bare word was not.
//! * **A missing visibility on an import** (`SYN001`) gets `private` in front of it, which is the
//!   visibility the language assigns when the keyword is absent.
//!
//! Nothing else is offered. A "fix" that guessed at intent — rename this to something else, split
//! this declaration — would be a suggestion the author then has to check, and a suggestion that
//! needs checking is slower than typing.
//!
//! # Why the words come from the diagnostic
//!
//! The range the client sends back is its own view of the diagnostic. A fix that recomputed the
//! range from the model could land one character off after an edit the client has already sent,
//! and the result would be a file that parses but says something else. So the text of the fix is
//! derived from the diagnostic's own words, and the range is the one the client named.

/// One edit a client may apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Start and end, as UTF-16 columns in a zero-based line, which is the protocol's unit.
    pub start: (u32, u32),
    pub end: (u32, u32),
    /// The text to put there.
    pub text: String,
}

/// One fix: what to call it, and what it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub title: String,
    pub edit: Edit,
}

/// The fixes that apply to one diagnostic, if any.
///
/// `message` and `code` come from the diagnostic the client reported; `range` is where the client
/// drew it. Returning `None` is the common case and is not a failure — most diagnostics are things
/// a person has to decide about, not things a machine can rewrite.
pub fn fixes(code: &str, message: &str, start: (u32, u32), end: (u32, u32)) -> Vec<Fix> {
    match code {
        "RES001" => reserved_name(message, start, end).into_iter().collect(),
        "SYN001" => missing_visibility(message, start).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// `frame` → `'frame'`.
///
/// The word is read out of the diagnostic's own sentence rather than from the buffer, so the fix
/// describes the same token the message does. A diagnostic that names no word gets no fix, which
/// is the honest answer rather than a guess at which word it meant.
fn reserved_name(message: &str, start: (u32, u32), end: (u32, u32)) -> Option<Fix> {
    let word = between(message, "`", "` is a reserved word")?;
    if word.is_empty() {
        return None;
    }
    Some(Fix {
        title: format!("Write `'{word}'` instead"),
        edit: Edit {
            start,
            end,
            text: format!("'{word}'"),
        },
    })
}

/// `import X::*` → `private import X::*`.
///
/// The insert goes at the start of the range the client drew, which for this diagnostic is the
/// whole line: the parser stopped at `import`, so the line is where the problem is and the word
/// belongs before it.
fn missing_visibility(message: &str, start: (u32, u32)) -> Option<Fix> {
    if !message.contains("an `import` needs a visibility keyword") {
        return None;
    }
    Some(Fix {
        title: "Make the import private".to_string(),
        edit: Edit {
            start,
            end: start,
            text: "private ".to_string(),
        },
    })
}

/// The text between the first `open` and the `close` that follows it.
fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(close)?;
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reserved_word_is_quoted() {
        let fixes = fixes(
            "RES001",
            "`frame` is a reserved word and cannot be a name; write `'frame'` to use it as one",
            (4, 9),
            (4, 14),
        );
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].edit.text, "'frame'");
        assert_eq!(fixes[0].edit.start, (4, 9));
        assert_eq!(fixes[0].edit.end, (4, 14));
        assert!(fixes[0].title.contains("'frame'"), "{}", fixes[0].title);
    }

    #[test]
    fn the_quoted_word_is_the_one_the_diagnostic_names() {
        // The fix reads the word out of the message rather than out of the buffer: a range the
        // client computed and a word this side recomputed could disagree after an edit.
        let fixes = fixes(
            "RES001",
            "`bind` is a reserved word and cannot be a name; write `'bind'` to use it as one",
            (0, 0),
            (0, 4),
        );
        assert_eq!(fixes[0].edit.text, "'bind'");
    }

    #[test]
    fn a_missing_import_visibility_gets_the_word_in_front() {
        let fixes = fixes(
            "SYN001",
            "an `import` needs a visibility keyword before it, found `import` (line 2)",
            (2, 0),
            (2, 0),
        );
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].edit.text, "private ");
        // A zero-width edit at the start of the line, so nothing the author wrote is replaced.
        assert_eq!(fixes[0].edit.start, (2, 0));
        assert_eq!(fixes[0].edit.start, fixes[0].edit.end);
    }

    #[test]
    fn a_syntax_error_that_is_not_about_visibility_gets_no_fix() {
        // Most parse failures are not one keyword away from being right, and a fix that inserted
        // `private` into one of them would be worse than no fix at all.
        let fixes = fixes(
            "SYN001",
            "expected `;` or a body to end the declaration, found `is` (line 2)",
            (1, 0),
            (1, 9),
        );
        assert!(fixes.is_empty(), "{fixes:?}");
    }

    #[test]
    fn a_diagnostic_with_no_mechanical_fix_gets_none() {
        // A name nothing binds is a person's decision: the fix could be a typo, a missing import,
        // or an element that should exist and does not.
        let fixes = fixes("REF001", "unresolved: Engine", (3, 10), (3, 16));
        assert!(fixes.is_empty(), "{fixes:?}");
    }

    #[test]
    fn a_reserved_word_diagnostic_with_no_word_gets_no_fix() {
        let fixes = fixes("RES001", "something else entirely", (0, 0), (0, 1));
        assert!(fixes.is_empty(), "{fixes:?}");
    }
}
