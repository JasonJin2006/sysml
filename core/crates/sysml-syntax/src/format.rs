//! Re-indenting a model without touching a single token.
//!
//! # Why this is not "parse, then render"
//!
//! A formatter that printed the tree would lose everything the tree does not carry: comments,
//! the blank lines a person used to group members, the line a `doc` starts on, the spelling of a
//! number. Every one of those is something the author wrote on purpose, and a formatter that
//! deletes them is a formatter nobody runs twice.
//!
//! So this works on the text: it reads the line structure, tracks how deep the braces are, and
//! rewrites the leading whitespace of each line. The tokens keep their exact spelling and order.
//! The only things that change are indentation, trailing spaces, and runs of blank lines.
//!
//! # What it does
//!
//! * **Indentation** is four spaces per brace depth, matching every file already in this
//!   repository and every example in the specification's own corpus.
//! * **A closing brace** goes back one level, so it lines up with the line that opened it.
//! * **Blank lines** collapse to at most one, and are removed inside an otherwise empty body.
//! * **Trailing whitespace** goes away, and the file ends with exactly one newline.
//!
//! # What it deliberately does not do
//!
//! It does not re-wrap long lines, re-space inside a line, reorder members, or split a one-line
//! declaration into several. Those are choices a person makes while writing, and a formatter that
//! imposes them turns every save into a diff about style nobody asked for.

/// Four spaces per level: the width every file in this repository already uses.
const INDENT: usize = 4;

/// Re-indent `source`.
///
/// Returns the text unchanged when it holds a brace that never closes, or a block comment that
/// never ends. A formatter that guessed at an unbalanced buffer would move code into the wrong
/// block, and the file would still parse — which is the one outcome worse than leaving it alone.
pub fn format(source: &str) -> String {
    let lines = scan(source);
    if !lines.balanced {
        return source.to_string();
    }
    let mut out: String = String::with_capacity(source.len());
    let mut depth = 0usize;
    let mut blank_pending = false;
    let mut wrote_any = false;
    // Whether the line just written opened a body. A blank line after it holds nothing, and a
    // blank line before a closer holds nothing either; both are rows the reader has to look at
    // and dismiss.
    let mut opened_body = false;
    for line in &lines.items {
        // A line inside a multi-line comment or string is prose: it keeps its own indentation and
        // its own place, because moving it would rewrap something the author laid out.
        if line.inside {
            if wrote_any && blank_pending {
                out.push('\n');
            }
            blank_pending = false;
            out.push_str(line.text.trim_end());
            out.push('\n');
            wrote_any = true;
            continue;
        }
        let trimmed = line.text.trim();
        if trimmed.is_empty() {
            // A blank line is remembered rather than written, so a run of five becomes one and a
            // run before a closing brace becomes none.
            blank_pending = wrote_any;
            continue;
        }
        let closes_first = starts_with_closing_brace(trimmed);
        let opens_minus_closes = net_depth_delta(trimmed);
        let own_depth = if closes_first {
            depth.saturating_sub(1)
        } else {
            depth
        };
        if blank_pending && wrote_any && !closes_first && !opened_body {
            out.push('\n');
        }
        blank_pending = false;
        for _ in 0..own_depth * INDENT {
            out.push(' ');
        }
        out.push_str(trimmed);
        out.push('\n');
        wrote_any = true;
        opened_body = trimmed.ends_with('{');
        // The next line's depth. `closes_first` already took one off, so the delta is applied to
        // the depth as it was *before* the closer, which keeps `}` and `{` on one line balanced.
        depth = (depth as isize + opens_minus_closes).max(0) as usize;
    }
    // Exactly one newline at the end; a file with none, or with five, is the same file.
    while out.ends_with("\n\n") {
        out.pop();
    }
    if out.is_empty() {
        // A file of nothing but blank lines becomes one newline rather than nothing: an editor
        // with an empty buffer shows a line, and a formatter that left zero bytes would move the
        // caret to nowhere.
        return if source.is_empty() {
            String::new()
        } else {
            "\n".to_string()
        };
    }
    out
}

/// A source split into lines, with what the scan learned about it.
struct Scan<'a> {
    items: Vec<Line<'a>>,
    /// False when a brace or a comment is unbalanced, in which case the caller leaves the file be.
    balanced: bool,
}

/// Split the source into lines, keeping each declaration's leading whitespace out of the way.
///
/// A `doc /* … */` written across several lines is one thing, not five lines of SysML: re-indenting
/// its inner lines would move prose that was laid out to be read, and counting its braces would
/// count prose. So a line that *opens* a multi-line comment or string keeps its own indentation as
/// written, and every line inside the run is passed through untouched — the item carries an `at`
/// offset and the formatter knows not to re-indent it.
fn scan(source: &str) -> Scan<'_> {
    let bytes = source.as_bytes();
    let mut items = Vec::new();
    let mut line_start = 0usize;
    let mut i = 0usize;
    let mut state = State::Code;
    let mut brace_depth = 0isize;
    let mut balanced = true;
    // Whether the line being read began in the middle of a multi-line token, in which case its
    // leading whitespace is part of the prose and is left alone.
    let mut inside = false;

    while i < bytes.len() {
        match state {
            State::Code => match bytes[i] {
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                    items.push(Line {
                        text: &source[line_start..i],
                        inside,
                    });
                    line_start = (i + 1).min(source.len());
                    inside = false;
                    i += 1;
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                    state = State::BlockComment(1);
                    i += 2;
                }
                b'"' => {
                    state = State::String;
                    i += 1;
                }
                b'\'' => {
                    state = State::QuotedName;
                    i += 1;
                }
                b'{' => {
                    brace_depth += 1;
                    i += 1;
                }
                b'}' => {
                    brace_depth -= 1;
                    if brace_depth < 0 {
                        balanced = false;
                    }
                    i += 1;
                }
                b'\n' => {
                    items.push(Line {
                        text: &source[line_start..i],
                        inside,
                    });
                    line_start = i + 1;
                    inside = false;
                    i += 1;
                }
                _ => i += 1,
            },
            State::String => match bytes[i] {
                b'\\' => i += 2,
                b'"' => {
                    state = State::Code;
                    i += 1;
                }
                b'\n' => {
                    // A string that reaches a newline is one token; the next line is inside it.
                    items.push(Line {
                        text: &source[line_start..i],
                        inside,
                    });
                    line_start = i + 1;
                    inside = true;
                    i += 1;
                }
                _ => i += 1,
            },
            State::QuotedName => match bytes[i] {
                b'\\' => i += 2,
                b'\'' => {
                    state = State::Code;
                    i += 1;
                }
                b'\n' => {
                    items.push(Line {
                        text: &source[line_start..i],
                        inside,
                    });
                    line_start = i + 1;
                    inside = true;
                    i += 1;
                }
                _ => i += 1,
            },
            State::BlockComment(depth) => {
                if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    state = State::BlockComment(depth + 1);
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    state = if depth == 1 {
                        State::Code
                    } else {
                        State::BlockComment(depth - 1)
                    };
                    i += 2;
                } else if bytes[i] == b'\n' {
                    items.push(Line {
                        text: &source[line_start..i],
                        inside,
                    });
                    line_start = i + 1;
                    inside = true;
                    i += 1;
                } else {
                    i += 1;
                }
            }
        }
    }
    if line_start < source.len() {
        items.push(Line {
            text: &source[line_start..],
            inside,
        });
    }
    if !matches!(state, State::Code) || brace_depth != 0 {
        balanced = false;
    }
    Scan { items, balanced }
}

/// One source line, and whether it began inside a multi-line token.
struct Line<'a> {
    text: &'a str,
    /// True when the line is the middle or the end of a `/* … */` or a long string. Its leading
    /// whitespace is the author's, and re-indenting it would move prose.
    inside: bool,
}

#[derive(Clone, Copy)]
enum State {
    Code,
    String,
    QuotedName,
    BlockComment(usize),
}

/// Whether `line` opens with a closing brace, ignoring a comment or a doc in front of it.
///
/// `} // end of package` and `/* note */ }` both put the closer first as far as the layout is
/// concerned: the brace is what has to line up with the line that opened the block.
fn starts_with_closing_brace(line: &str) -> bool {
    let mut rest = line.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("/*") {
            // A block comment that closes on this line is skipped whole; one that does not is the
            // start of a multi-line run, and the caller has already decided not to re-indent it.
            match after.find("*/") {
                Some(at) => rest = after[at + 2..].trim_start(),
                None => return false,
            }
            continue;
        }
        if let Some(after) = rest.strip_prefix("//") {
            let _ = after;
            return false;
        }
        break;
    }
    rest.starts_with('}')
}

/// Braces opened minus braces closed in `line`, ignoring those inside strings and comments.
fn net_depth_delta(line: &str) -> isize {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut delta = 0isize;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => return delta,
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                // A block comment that does not close on this line leaves the count where it is:
                // its braces are prose, and the next line continues the same comment.
                match line[i + 2..].find("*/") {
                    Some(at) => i += 2 + at + 2,
                    None => return delta,
                }
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'{' => {
                delta += 1;
                i += 1;
            }
            b'}' => {
                delta -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    delta
}
