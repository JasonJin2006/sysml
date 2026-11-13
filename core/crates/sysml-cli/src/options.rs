//! What each command accepts on the command line, and what it means to get it wrong.
//!
//! The parser used to collect any `--key value` pair into one bag and let each command read
//! what it recognised. A misspelt key (`--frrom`, `--whyy`) was therefore accepted, dropped,
//! and the command still succeeded — the decision was written with an empty field and the
//! caller had no way to know. For a person that is a wasted minute; for an agent it is a
//! record that says something the agent did not mean, and it will not look at the command line
//! again.
//!
//! So the set is closed per command. An option outside the set is an error, and the error
//! names the options the command does accept, which is what a typed `--frrom` needs.

/// The options a command accepts, in the order they should be offered in a usage line.
///
/// A key ending in `...` is repeatable (`--from` twice names two inputs). `--json` is a
/// switch and takes no value; everything else is followed by one.
///
/// `--view`, `--mode` and `--out` are parsed before the command is known, because they look
/// the same for every command that takes them. They appear here too, so that one check covers
/// all options: naming `--view` on `check` is the same kind of mistake as `--frrom`.
pub fn accepted(command: &str) -> Option<&'static [&'static str]> {
    Some(match command {
        // These read only the model.
        "outline" => &["--json", "--filter"],
        "resolve" => &["--json"],
        "graph" => &["--json", "--view", "--mode", "--filter"],
        "render" => &["--view", "--mode", "--out", "--filter", "--style"],
        "check" => &["--json", "--library...", "--no-cache"],
        "prov" => &["--json", "--library..."],
        "units" => &["--json", "--library..."],
        "next" => &["--json", "--lang"],
        "status" => &["--issues", "--json"],
        "review" => &["--view", "--mode", "--out"],
        "conformance" => &["--json"],
        "studio" => &["--port"],
        "ask" => &["--question", "--at", "--blocks..."],
        "answer" => &["--id", "--text", "--by", "--defer"],
        "decide" => &[
            "--element",
            "--origin",
            "--value",
            "--why",
            "--falsifier",
            "--from...",
            "--cite...",
            "--question",
            "--by",
            "--approved-by",
        ],
        "source" => &[
            "--title",
            "--kind",
            "--quote",
            "--locator",
            "--accessed",
            "--jurisdiction",
            "--by",
        ],
        "scope" => &["--market", "--jurisdictions"],
        "approve" => &["--id", "--by"],
        "retract" => &["--id"],
        "diff" => &["--json", "--library..."],
        "impact" => &["--element", "--json"],
        "judge" => &["--requirement", "--by", "--at-time", "--json"],
        "ledger" => &["--json"],
        "trace" => &["--clauses", "--json"],
        "table" => &["--matrix", "--out"],
        _ => return None,
    })
}

/// Whether an option takes no value.
pub fn is_switch(key: &str) -> bool {
    matches!(
        key,
        "--json" | "--issues" | "--defer" | "--no-cache" | "--matrix"
    )
}

/// The error for an option no command here accepts, or one the given command does not.
///
/// The suggestion is a prefix match rather than an edit distance: `--frrom` and `--from` share
/// four leading characters, and every real key is a word, so a common prefix of three is
/// enough to pick the intended one without a spell-check dependency.
pub fn unknown_option(command: &str, key: &str, known_keys: &[&'static str]) -> String {
    let flat: Vec<&str> = known_keys
        .iter()
        .map(|k| k.trim_end_matches("..."))
        .collect();
    let close = closest(key, &flat);
    let hint = match close {
        Some(best) => format!("; did you mean `{best}`"),
        None => match known_keys.is_empty() {
            true => format!("; `{command}` takes no options"),
            false => format!(
                "; `{command}` accepts {}",
                flat.iter()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
    };
    format!("unknown option `{key}` for `{command}`{hint}")
}

/// The accepted key sharing the longest prefix with `key`, if any shares three characters.
///
/// The key being rejected is never offered back. `unknown_option` is only reached for keys the
/// command does not accept, but the prefix match does not know that — it compared against the
/// full list, so rejecting `--json` on a command that had no such option suggested `--json`.
fn closest<'a>(key: &str, candidates: &[&'a str]) -> Option<&'a str> {
    candidates
        .iter()
        .filter(|candidate| **candidate != key)
        .map(|candidate| (*candidate, common_prefix(key, candidate)))
        .filter(|&(_, shared)| shared >= 3)
        .max_by_key(|&(_, shared)| shared)
        .map(|(candidate, _)| candidate)
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// The line that says how to use one command, for `--help` and for the error that follows a
/// call with no arguments.
pub fn usage_line(command: &str) -> Option<String> {
    let keys = accepted(command)?;
    // `diff` compares two models. The generic form below names one file, and a usage line that
    // omits the second argument sends the caller into an error that reads like a bug in the tool.
    if command == "diff" {
        return Some("sysml diff <before.sysml> <after.sysml> [--json]".to_string());
    }
    let options = if keys.is_empty() {
        String::new()
    } else {
        format!(
            " [{}]",
            keys.iter()
                .map(|k| k.trim_end_matches("...").to_string())
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    Some(format!("sysml {command} <file.sysml>{options}"))
}

/// Every command, with the one-line description shown by `--help`.
pub const COMMANDS: &[(&str, &str)] = &[
    ("outline", "the model as an indented tree"),
    ("resolve", "every written name and what it binds to"),
    (
        "graph",
        "the model as nodes and edges (--view gv|iv|tv|pv|stv|afv|sv)",
    ),
    (
        "render",
        "the graph as SVG (--view, --style nested|tree, --out file.svg)",
    ),
    (
        "check",
        "syntax, names, values, constraints, and the ledger gate",
    ),
    (
        "prov",
        "every number must have a source; the ledger gate alone",
    ),
    (
        "units",
        "what kind of quantity each value is, and which units are unchecked",
    ),
    (
        "next",
        "the stage the model is in, and the one command to run",
    ),
    ("status", "one summary of both gates"),
    ("review", "a self-contained HTML report (--out file.html)"),
    (
        "conformance",
        "which specification rules this kernel enforces",
    ),
    ("studio", "an editable canvas over the model (--port)"),
    ("ask", "record an open question (--question)"),
    ("answer", "answer or defer a question (--id --text)"),
    (
        "decide",
        "record where a value came from (--element --origin)",
    ),
    (
        "source",
        "register material a value cites (--title --quote)",
    ),
    (
        "scope",
        "record what the research covered (--market --jurisdictions)",
    ),
    (
        "approve",
        "a person accepting an assumption (--id --by human:<name>)",
    ),
    ("retract", "take back a wrong entry (--id)"),
    ("diff", "what changed between two models (--json)"),
    ("impact", "what depends on an element (--element)"),
    ("judge", "record what a requirement evaluated to (--json)"),
    ("ledger", "everything on record for this model"),
    (
        "trace",
        "which standard clauses the model covers, and which it cites wrongly (--clauses)",
    ),
    (
        "table",
        "the model as a table, or its requirements as a matrix (--matrix, --out)",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_misspelt_option_names_the_one_that_was_meant() {
        let known = accepted("decide").expect("decide is a command");
        let message = unknown_option("decide", "--frrom", known);
        assert!(message.contains("did you mean `--from`"), "{message}");
    }

    #[test]
    fn an_option_no_command_shares_a_prefix_with_lists_what_is_accepted() {
        let known = accepted("approve").expect("approve is a command");
        let message = unknown_option("approve", "--nonsense", known);
        assert!(message.contains("unknown option `--nonsense`"), "{message}");
        assert!(message.contains("`--id`"), "{message}");
        assert!(message.contains("`--by`"), "{message}");
    }

    #[test]
    fn a_command_that_takes_nothing_says_so() {
        // `render` writes SVG to stdout or a file; it takes `--view`, `--mode` and `--out` but
        // no `--json`, because the answer is an image rather than a document.
        let known = accepted("render").expect("render is a command");
        let message = unknown_option("render", "--jsom", known);
        assert!(message.contains("unknown option `--jsom`"), "{message}");
        assert!(message.contains("`--view`"), "{message}");
    }

    #[test]
    fn the_suggestion_is_never_the_option_it_rejected() {
        // `outline` did not accept `--json` when this check was written, and the prefix match
        // offered `--json` back as its own correction. A suggestion identical to the input
        // reads as the tool arguing with itself.
        for command in ["outline", "resolve", "graph", "status", "check", "decide"] {
            let known = accepted(command).expect("a command");
            for key in ["--json", "--frrom", "--zzz"] {
                let message = unknown_option(command, key, known);
                let known_keys: Vec<&str> =
                    known.iter().map(|k| k.trim_end_matches("...")).collect();
                if known_keys.contains(&key) {
                    continue;
                }
                assert!(
                    !message.contains(&format!("did you mean `{key}`")),
                    "{command} rejected {key} and suggested it back: {message}"
                );
            }
        }
    }

    #[test]
    fn every_command_in_the_usage_list_has_an_accepted_set() {
        for (command, _) in COMMANDS {
            assert!(accepted(command).is_some(), "{command} has no option set");
        }
    }

    #[test]
    fn the_two_path_commands_say_so_in_their_usage_line() {
        // The generic form names one file. `diff` needs two, and a usage line that omits the
        // second sends the caller into an error that reads like a bug in the tool.
        let line = usage_line("diff").expect("diff is a command");
        assert!(line.contains("<before.sysml>"), "{line}");
        assert!(line.contains("<after.sysml>"), "{line}");
    }

    #[test]
    fn a_repeatable_option_is_spelled_once_in_the_usage_line() {
        let line = usage_line("decide").expect("decide is a command");
        assert!(line.contains("--from"), "{line}");
        assert!(!line.contains("--from..."), "{line}");
    }
}
