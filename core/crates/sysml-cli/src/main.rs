use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

mod cache;
mod machine;
mod options;
mod review;

use serde_json;
use sysml_resolve::{Index, Resolution};
use sysml_view::{Mode, View};

/// Exit codes, so a caller can act on the difference between four different failures.
///
/// `check` used to answer 1 for everything: a syntax error, a ledger nobody has started, and a
/// misspelt flag all looked the same to a script. The distinction that matters most is the last
/// two — a model whose ledger is untouched is not wrong, it is unfinished, and the action is to
/// record a decision rather than to fix the model.
const EXIT_PASS: u8 = 0;
/// The model itself has a problem: syntax, a broken name, a violated rule, or a ledger entry
/// that contradicts the model.
const EXIT_MODEL: u8 = 1;
/// The call was wrong: an unknown command or option, a missing file, a bad value. Reported on
/// stderr before any model is read.
const EXIT_USAGE: u8 = 2;
/// The model is clean and its ledger has not been started. Nothing is broken; the next step is
/// to record where the numbers came from.
const EXIT_UNFINISHED: u8 = 3;

/// `sysml resolve <file.sysml> [...]` — the symbol table, then every written type reference
/// with the element it actually binds to, then what each import brought in. Unresolved names
/// are listed rather than dropped.
/// Which of the specification's own rules this kernel enforces, and which it does not.
///
/// The rule table is the metamodel's 180 `validate*` constraints. A model that reports "clean"
/// says nothing about the rules that were never applied, so the count has to be printed beside
/// the list, not instead of it.
fn conformance_command(options: &[(String, String)]) -> Result<ExitCode, String> {
    let cov = sysml_semantics::conformance::coverage();
    let done = sysml_semantics::conformance::implemented();
    let na = sysml_semantics::conformance::NOT_APPLICABLE;
    let conflict = sysml_semantics::conformance::CONFLICT;
    // The outstanding rules carry their blocker beside them, so a reader of the JSON can sort by
    // what the next layer supplies rather than by metaclass.
    let todo: Vec<_> = sysml_semantics::conformance::unimplemented()
        .into_iter()
        .filter(|r| !na.iter().any(|(name, _)| *name == r.name))
        .filter(|r| !conflict.iter().any(|(name, _)| *name == r.name))
        .collect();
    if option_value(options, "--json").is_some() {
        let enforced: Vec<serde_json::Value> = done
            .iter()
            .map(|(code, rule)| serde_json::json!({"code": code, "rule": rule}))
            .collect();
        let pending: Vec<serde_json::Value> = todo
            .iter()
            .map(|r| {
                serde_json::json!({
                    "rule": r.name,
                    "metaclass": r.metaclass,
                    "blockedBy": match sysml_semantics::conformance::status(r.name) {
                        sysml_semantics::conformance::Status::Blocked(what) => Some(what),
                        _ => None,
                    },
                    "says": r.says,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "specRules": cov.total,
                "enforcedRules": cov.enforced,
                "notApplicableRules": cov.not_applicable,
                "conflictRules": cov.conflict,
                "blockedRules": cov.blocked,
                "notApplicable": na
                    .iter()
                    .map(|(name, why)| serde_json::json!({
                        "rule": name,
                        "why": why,
                        "evidence": sysml_semantics::conformance::evidence_of(name)
                            .map(sysml_semantics::conformance::Evidence::as_str),
                    }))
                    .collect::<Vec<_>>(),
                "conflict": conflict
                    .iter()
                    .map(|(name, why)| serde_json::json!({"rule": name, "why": why}))
                    .collect::<Vec<_>>(),
                "enforced": enforced,
                "unimplemented": pending,
                // Rules the reference enforces that the specification does not state. Reported
                // under their own key, never folded into the counts above: a reader who adds
                // them to `enforcedRules` would be measuring this kernel against a rule set the
                // specification never wrote.
                "extendedRules": sysml_semantics::conformance::extended_coverage().total,
                "extendedComplete": sysml_semantics::conformance::extended_coverage().complete,
                "extended": sysml_semantics::conformance::EXTENDED
                    .iter()
                    .map(|r| serde_json::json!({
                        "rule": r.name,
                        "code": r.code,
                        "basis": r.basis,
                        "ifWrong": r.if_wrong,
                        "incomplete": if r.incomplete.is_empty() { None } else { Some(r.incomplete) },
                    }))
                    .collect::<Vec<_>>(),
            })
        );
        return Ok(ExitCode::from(EXIT_PASS));
    }
    println!(
        "specification rules {}   enforced {}   not applicable {}   conflict {}   blocked {}",
        cov.total, cov.enforced, cov.not_applicable, cov.conflict, cov.blocked
    );
    // The ratio is stated over the whole table, not over `total - not_applicable`. A denominator that
    // shrinks when a rule is labelled "not applicable" makes the percentage self-certifying, and an
    // audit of the 52 entries found one that was unimplemented work rather than an impossible rule
    // (`validateSubsettingFeaturingTypes`, now enforced). The denominator is the full table, so the
    // number cannot be raised by re-filing a rule.
    println!(
        "                         {:.1}% of the table enforced ({} of {})",
        100.0 * cov.enforced as f64 / cov.total as f64,
        cov.enforced,
        cov.total
    );
    // The specification's 180 are not the whole of what a conforming tool rejects. The reference
    // validator adds rules of its own — "a port must be typed by port definitions" and its
    // siblings, the inherited-duplicate warning, its positional parameter redefinition — tracked
    // separately so the 180 ledger stays the 180. The count comes from `EXTENDED`, which is the one
    // place that says, per rule, what it rests on and what breaks if it is wrong.
    let extended = sysml_semantics::conformance::extended_coverage();
    println!(
        "reference rules         {}   complete {}   incomplete {}",
        extended.total, extended.complete, extended.incomplete
    );
    // A third kind of check: the declared type of a metamodel property rather than a constraint.
    // Separate line, separate table, so the 180 stays the 180.
    let properties = sysml_semantics::conformance::PROPERTY_CHECKS.len();
    println!("metamodel properties    {properties}   checked {properties}");
    println!();
    println!("enforced");
    for (code, rule) in done {
        println!("  {code:8} {rule}");
    }
    println!();
    println!("not applicable ({})", na.len());
    for (name, why) in na {
        // The class is printed before the reason, because it says how much the reason is worth:
        // `grammar` and `metamodel` are text a reader can open, `construction` is this tree, and
        // `reference` is another program's behaviour.
        let class = sysml_semantics::conformance::evidence_of(name)
            .map(sysml_semantics::conformance::Evidence::as_str)
            .unwrap_or("unclassified");
        println!("  {name}  [{class}]");
        println!("    {why}");
    }
    println!();
    println!("conflict ({})", conflict.len());
    for (name, why) in conflict {
        println!("  {name}");
        println!("    {why}");
    }
    println!();
    // Grouped by what each rule is waiting on: the count per blocker is what tells the next layer
    // what it is worth. A blocker is a metaclass, a derived property or an element the rule reads
    // and the tree does not carry; the rule is decided once that is added, not when someone has
    // time for it.
    let blocked = sysml_semantics::conformance::BLOCKED_BY;
    println!(
        "blocked ({}) — each group is what its rules are waiting on",
        blocked.len()
    );
    let mut by_blocker: std::collections::BTreeMap<&str, Vec<&str>> =
        std::collections::BTreeMap::new();
    for (rule, blocker) in blocked {
        by_blocker.entry(blocker).or_default().push(rule);
    }
    for (blocker, rules) in &by_blocker {
        println!("  {blocker} ({})", rules.len());
        for rule in rules {
            println!("    {rule}");
        }
    }
    // The rules whose authority is the reference implementation. Printed after the 180's sections
    // because they are a different kind of claim, and each entry carries what it rests on: a
    // reader who disagrees with a rule needs the basis to check, not just the name.
    println!();
    println!("reference rules ({})", extended.total);
    for rule in sysml_semantics::conformance::EXTENDED {
        println!("  {}  {}", rule.code, rule.name);
        println!("    basis : {}", rule.basis);
        if !rule.incomplete.is_empty() {
            println!("    NOT   : {}", rule.incomplete);
        }
    }
    Ok(ExitCode::from(EXIT_PASS))
}

fn report_resolution(loaded: &sysml_syntax::Loaded) -> String {
    // The library is attached so imports can bind, but the symbol table lists project
    // elements only: library elements are not part of the model being resolved.
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    let library_size: usize = loaded.library.iter().map(sysml_model::Element::size).sum();
    let project_end = index.len() - library_size;
    let mut out = String::new();

    out.push_str("symbols\n");
    for id in 0..project_end {
        out.push_str(&format!(
            "  {:<40} {:<14} line {}\n",
            index.qualified[id],
            index.kind[id].keyword_as(index.is_definition[id]),
            index.line[id]
        ));
    }

    let mut resolved = Vec::new();
    let mut problems = Vec::new();
    for reference in index.references(&loaded.roots) {
        let from = &index.qualified[reference.from];
        match &reference.target {
            Resolution::Found(id) => resolved.push(format!(
                "  {from} : {} -> {}",
                reference.text, index.qualified[*id]
            )),
            Resolution::Ambiguous(ids) => problems.push(format!(
                "  {from} : {} -> ambiguous between {}",
                reference.text,
                ids.iter()
                    .map(|id| index.qualified[*id].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Resolution::Unresolved => {
                problems.push(format!("  {from} : {} -> unresolved", reference.text))
            }
            Resolution::NotAccessible => problems.push(format!(
                "  {from} : {} -> a feature reached with `::`; use `.` to nest into a feature",
                reference.text
            )),
        }
    }

    out.push_str("\nresolved\n");
    if resolved.is_empty() {
        out.push_str("  (none)\n");
    } else {
        out.push_str(&resolved.join("\n"));
        out.push('\n');
    }

    out.push_str("\nnot resolved\n");
    if problems.is_empty() {
        out.push_str("  (none)\n");
    } else {
        out.push_str(&problems.join("\n"));
        out.push('\n');
    }

    out.push_str(&report_imports(&index));
    out
}

/// One line per import: what it targeted, what it bound, and whether the target resolved.
fn report_imports(index: &Index) -> String {
    let mut out = String::new();
    out.push_str("\nimports\n");
    let entries = index.imports();
    if entries.is_empty() {
        out.push_str("  (none)\n");
        return out;
    }
    for entry in &entries {
        let owner = entry
            .owner
            .map(|id| index.qualified[id].as_str())
            .unwrap_or("<root>");
        let visibility = entry
            .visibility
            .map(|v| format!("{} ", v.keyword()))
            .unwrap_or_default();
        let text = format!(
            "import {}{}{}",
            visibility,
            index.name[entry.element],
            entry.shape.suffix()
        );
        match &entry.target {
            Resolution::Found(id) => {
                let names: Vec<&str> = entry.bound.iter().map(|(n, _)| n.as_str()).collect();
                if names.is_empty() {
                    out.push_str(&format!(
                        "  {owner} : {text} -> {} (nothing bound)\n",
                        index.qualified[*id]
                    ));
                } else {
                    out.push_str(&format!(
                        "  {owner} : {text} -> {} binds {}\n",
                        index.qualified[*id],
                        names.join(", ")
                    ));
                }
            }
            Resolution::Ambiguous(ids) => out.push_str(&format!(
                "  {owner} : {text} -> ambiguous between {}\n",
                ids.iter()
                    .map(|id| index.qualified[*id].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Resolution::Unresolved => out.push_str(&format!("  {owner} : {text} -> unresolved\n")),
            // An import's path is not a reference, so it never carries this verdict; printing it
            // like an unresolved name keeps the report total without inventing a new line shape.
            Resolution::NotAccessible => {
                out.push_str(&format!("  {owner} : {text} -> unresolved\n"))
            }
        }
    }
    out
}

/// `sysml check <file.sysml> [...]` — every unresolved or ambiguous reference, one per line,
/// with the line it was written on. Prints nothing and exits 0 when the model is clean, so the
/// command composes in a pipeline and its exit code is the verdict.
///
/// Names inside a requirement predicate are reported as errors like any other: a predicate
/// that names a feature the model does not declare is not a requirement, it is a sentence.
///
/// Constructs the front end parses but does not model are reported too, and they do not fail
/// the build. The distinction is deliberate: a state machine is not an error, but a tool that
/// accepts a model containing one and then calls the model clean is lying by omission. The
/// exit code stays for defects, the report line is for coverage.
fn report_problems(loaded: &sysml_syntax::Loaded) -> (String, usize, usize) {
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    let mut out = String::new();
    let mut count = 0;
    let mut unsupported = 0;
    for reference in index.references(&loaded.roots) {
        // A language function is not a model element the author forgot to declare. The semantic
        // layer already draws that line; the resolver's reference list does not, so the filter
        // belongs here rather than in the report.
        if sysml_semantics::is_builtin(&reference.text) {
            continue;
        }
        let problem = match &reference.target {
            Resolution::Found(_) => continue,
            Resolution::Ambiguous(ids) => format!(
                "ambiguous: {} (between {})",
                reference.text,
                ids.iter()
                    .map(|id| index.qualified[*id].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Resolution::Unresolved => format!("unresolved: {}", reference.text),
            Resolution::NotAccessible => format!(
                "not accessible: {} (a feature is reached with dot notation)",
                reference.text
            ),
        };
        count += 1;
        let where_ = if reference.in_predicate {
            "predicate"
        } else {
            "type"
        };
        out.push_str(&format!(
            "{}:{}: {} {}\n",
            index.qualified[reference.from], index.line[reference.from], where_, problem
        ));
    }
    // Semantic checks: values, connection ends, and whether a constraint actually holds. These
    // live in their own crate because a resolver can only answer "does this name exist", and
    // every one of these questions was previously answered by silence.
    let semantic = sysml_semantics::check(&loaded.roots, &loaded.library);
    for issue in &semantic.issues {
        if issue.severity == sysml_semantics::Severity::Error {
            count += 1;
        }
        out.push_str(&issue.render());
        out.push('\n');
    }
    // One line per requirement that did not come out `pass`. A requirement nobody can evaluate
    // is not a passing requirement; saying so is the difference between a green light and a
    // green light that means something.
    for verdict in &semantic.verdicts {
        if matches!(verdict.verdict, sysml_semantics::Verdict::Pass(_)) {
            continue;
        }
        let detail = match &verdict.verdict {
            sysml_semantics::Verdict::Fail(v) => format!("holds?  no (evaluates {})", v.render()),
            sysml_semantics::Verdict::Inconclusive(why) => format!("undecided: {why}"),
            sysml_semantics::Verdict::Error(why) => format!("broken: {why}"),
            sysml_semantics::Verdict::Pass(_) => unreachable!(),
        };
        out.push_str(&format!(
            "{}:{}: verdict[{}]: {}\n",
            verdict.qualified,
            verdict.line,
            verdict.verdict.keyword(),
            detail
        ));
    }
    // Coverage lines come after the defects so a reader scanning for errors stops at the first
    // block, and so their presence never changes the exit code.
    let mut skipped: Vec<String> = Vec::new();
    for id in 0..index.project_end() {
        if index.kind[id] != sysml_model::Kind::Unsupported {
            continue;
        }
        unsupported += 1;
        skipped.push(format!(
            "{}:{}: not modelled: {}",
            index.qualified[id], index.line[id], index.name[id]
        ));
    }
    if !skipped.is_empty() {
        skipped.sort();
        skipped.dedup();
        for line in &skipped {
            out.push_str(line);
            out.push('\n');
        }
    }
    (out, count, unsupported)
}

/// Every `.sysml` and `.kerml` file under `dir`, in sorted order.
fn collect_sources(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_sources(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("sysml") | Some("kerml")
        ) {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}

/// The value given for an option, if it was given.
fn option_value(options: &[(String, String)], key: &str) -> Option<String> {
    options
        .iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
}

/// Whether the caller asked for the machine-readable form.
fn wants_json(options: &[(String, String)]) -> bool {
    option_value(options, "--json").is_some()
}

/// Print one JSON document, keeping the failure mode honest.
///
/// A serialisation failure would silently produce half a document, so it is mapped to an error
/// rather than swallowed with `unwrap_or_default` — a caller parsing the result would otherwise
/// read a truncated object as a complete one.
fn print_json(value: &serde_json::Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| format!("cannot serialise report: {e}"))?
    );
    Ok(())
}

/// Every value given for a repeatable option, in order.
fn option_all(options: &[(String, String)], key: &str) -> Vec<String> {
    options
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .collect()
}

/// The recorded name closest to a name that was not found, if any is close enough to be worth
/// offering.
///
/// Two errors quoting the same list would drift: `--from` and `--cite` both name something the
/// caller has already recorded, and both used to say "not found" with no way forward. A typed
/// `S1` for `S001` is one character short of obvious, and matching on a leaf name catches it
/// without pulling in a spell checker.
fn nearest<'a>(wanted: &str, known: &[&'a str]) -> Option<&'a str> {
    let leaf = wanted.split("::").last().unwrap_or(wanted);
    // `S1` against `S001` is a dropped character, and no prefix test sees that: the two agree on
    // one character and then diverge. Subsequence covers every single-character mistake —
    // inserted, dropped, or replaced — because in each case the shorter one still appears in
    // order inside the longer one. It is not a general edit distance, and does not need to be:
    // the caller prints the whole list anyway, so a wrong guess costs one glance.
    known
        .iter()
        .filter(|candidate| {
            let c_leaf = candidate.split("::").last().unwrap_or(candidate);
            c_leaf.starts_with(leaf)
                || leaf.starts_with(c_leaf)
                || candidate.contains(leaf)
                || is_subsequence(leaf, c_leaf)
                || is_subsequence(c_leaf, leaf)
        })
        .min_by_key(|candidate| candidate.len().abs_diff(wanted.len()))
        .copied()
}

/// Whether `short` appears in `long` in order, allowing characters between.
fn is_subsequence(short: &str, long: &str) -> bool {
    if short.is_empty() || long.is_empty() {
        return false;
    }
    let mut rest = long.chars();
    short.chars().all(|c| rest.any(|x| x == c))
}

/// The command surface, printed by `sysml --help` and by any call with no command.
fn usage() -> String {
    let mut out = String::from(
        "usage: sysml <command> <file.sysml> [...]\n       sysml <command> [--project sysml.project.json]\n",
    );
    out.push_str("\n  A directory of files is a project when a sysml.project.json sits at or above the\n  working directory; the commands below then take no path at all.\n\n");
    out.push_str("  A view narrows to what an annotation says: `--filter \"@Safety\"` or\n  `--filter \"@Safety{level >= 2}\"`.\n\n");
    for (command, says) in options::COMMANDS {
        out.push_str(&format!("  {command:<14}{says}\n"));
    }
    out.push_str(
        "\n  exit 0 pass   1 the model has a problem   2 the call itself was wrong\n\
         \x20 \x20\x20\x20\x20 3 the model is fine but its ledger is not started\n",
    );
    out
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    // Commands whose exit code is a verdict — `check`, `prov`, `status` — return failure when
    // the model is wrong. The rest return success unless the tool itself could not run.
    //
    // An `Err` here means the call never got as far as judging the model, so it is always the
    // usage code: the model's own verdict is carried in the `Ok` value.
    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Returns the process exit code, which `check` and `prov` set from the model's state.
fn run(args: &[String]) -> Result<ExitCode, String> {
    let mut rest = args.iter();
    let Some(command) = rest.next() else {
        return Err(usage());
    };
    // `--help` used to be taken for a file name, so the one thing a caller does when it does
    // not know the surface answered "missing input file". It answers with the surface now,
    // before anything is opened.
    if command == "--version" || command == "-V" || command == "version" {
        println!("sysml {}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::from(EXIT_PASS));
    }
    if command == "--help" || command == "-h" || command == "help" {
        let subject = rest.next();
        return match subject {
            Some(name) => match options::usage_line(name) {
                Some(line) => {
                    println!("{line}");
                    Ok(ExitCode::from(EXIT_PASS))
                }
                None => Err(format!("unknown command: {name}\n\n{}", usage())),
            },
            None => {
                println!("{}", usage());
                Ok(ExitCode::from(EXIT_PASS))
            }
        };
    }
    let mut paths = Vec::new();
    let mut view = View::General;
    let mut mode = Mode::Extended;
    let mut out_path: Option<String> = None;
    let mut project_path: Option<String> = None;
    // Free-form options for the ledger verbs. A model path and an option value can both look
    // like positional arguments, so those verbs take everything after `--key` by name.
    let mut options: Vec<(String, String)> = Vec::new();
    while let Some(arg) = rest.next() {
        if !arg.starts_with("--") {
            paths.push(arg.to_string());
            continue;
        }
        // `--project` names the model itself, which every command that reads a model can take. It
        // is read here rather than listed per command, the way `--view` and `--out` are.
        if arg == "--project" {
            project_path = Some(rest.next().ok_or("--project needs a path")?.clone());
            continue;
        }
        // `--key=value` is the same option as `--key value`. It is split *before* the key is
        // validated, because the check below compares the whole token against the accepted list:
        // `--library=stdlib` was rejected as an unknown option named literally `--library=stdlib`,
        // even though `--library stdlib` was accepted. Both spellings are ordinary on a command line,
        // and a user who writes the `=` form should not be told the option does not exist.
        let (arg, inline_value) = match arg.split_once('=') {
            Some((key, value)) => (key.to_string(), Some(value.to_string())),
            None => (arg.clone(), None),
        };
        let key = arg.as_str();
        // Reject the key before reading its value. A key that is silently dropped takes its
        // value with it, and the command then runs against a different question than the one
        // that was asked.
        let accepted = options::accepted(command)
            .ok_or_else(|| format!("unknown command: {command}\n\n{}", usage()))?;
        if !accepted.iter().any(|a| a.trim_end_matches("...") == key) {
            return Err(options::unknown_option(command, key, accepted));
        }
        if options::is_switch(key) {
            options.push((key.to_string(), inline_value.unwrap_or_default()));
            continue;
        }
        let value = match inline_value {
            Some(value) => value,
            None => rest
                .next()
                .ok_or_else(|| format!("{key} needs a value"))?
                .clone(),
        };
        let value = &value;
        match key {
            "--view" => {
                view = match value.as_str() {
                    "iv" | "interconnection" => View::Interconnection,
                    "gv" | "general" => View::General,
                    "tv" | "trace" | "traceability" => View::Traceability,
                    "pv" | "packages" | "package" => View::Packages,
                    "stv" | "states" | "state" => View::StateTransition,
                    "afv" | "actions" | "action" => View::ActionFlow,
                    "sv" | "sequence" => View::Sequence,
                    other => return Err(format!("unknown view: {other}")),
                }
            }
            "--mode" => {
                mode = Mode::parse(value).ok_or_else(|| format!("unknown mode: {value}"))?;
            }
            "--out" => out_path = Some(value.clone()),
            _ => options.push((key.to_string(), value.clone())),
        }
    }
    // `conformance` reports which of the specification's rules the kernel enforces. It reads no
    // model, so it answers before an input file is required.
    if command == "conformance" {
        return conformance_command(&options);
    }
    // `diff` compares two models, so it cannot use the one model every other command loads.
    // It reads its two paths here, before the shared load, and answers on its own.
    if command == "diff" {
        if paths.len() != 2 {
            return Err(format!(
                "diff needs two models: sysml diff <before.sysml> <after.sysml> (got {})\n\
                 A one-path call cannot say what changed; it would be reporting on a model \
                 against nothing.",
                paths.len()
            ));
        }
        return diff_command(&paths[0], &paths[1], &options);
    }
    // Everything below reads one model. A command given no path used to reach `paths[0]` and
    // panic, which the shell reports as exit 101 with a Rust backtrace: the worst possible
    // answer to a typo. Name the missing argument instead.
    // A manifest is the other way to name a model: `--project <file>`, or the one at or above the
    // working directory when no file was named. Everything below reads `loaded`, so this is the
    // last point where the two spellings can become one.
    let project = match project_path {
        Some(path) => Some(sysml_syntax::project::Project::read(Path::new(&path))?),
        None if paths.is_empty() => {
            let here = std::env::current_dir().map_err(|e| format!("working directory: {e}"))?;
            match sysml_syntax::project::discover(&here) {
                Some(manifest) => Some(sysml_syntax::project::Project::read(&manifest)?),
                None => None,
            }
        }
        None => None,
    };
    if paths.is_empty() && project.is_none() {
        // An unknown command is a different mistake from a missing path, and saying "needs a
        // model" to `sysml bogus` hides the real one. The command table is the authority.
        let Some(usage_line) = options::usage_line(command) else {
            return Err(format!("unknown command: {command}\n\n{}", usage()));
        };
        return Err(format!(
            "{command} needs a model: sysml {command} <file.sysml> [...]\n{usage_line}"
        ));
    }
    // A project's ledger is the project's, not the first file's: a model split across ten files has
    // one set of decisions behind it, and a ledger per file would spread them until none of them
    // accounted for the model.
    let model_path = match &project {
        Some(project) => project.root.join(sysml_syntax::project::MANIFEST),
        None => Path::new(&paths[0]).to_path_buf(),
    };
    let ledger_path = match &project {
        Some(project) => project.ledger.clone(),
        None => sysml_ledger::Ledger::path_for(&model_path),
    };

    // The memo is consulted before the model is loaded: loading is most of what a repeat run would
    // otherwise spend, and a cache that still parses the project saves the rules and not the work.
    let memo = project
        .as_ref()
        .filter(|_| command == "check" && wants_json(&options))
        .filter(|_| !options.iter().any(|(k, _)| k == "--no-cache"))
        .map(|project| {
            let key = cache::key(&project.sources, &project.library, &ledger_path, true);
            (project, key)
        });
    if let Some((project, key)) = &memo {
        if let Some((text, code)) = cache::lookup(&project.root, *key) {
            println!("{text}");
            return Ok(ExitCode::from(code));
        }
    }
    let sources: Vec<String> = paths
        .iter()
        .map(|path| fs::read_to_string(path).map_err(|e| format!("{path}: {e}")))
        .collect::<Result<_, _>>()?;
    let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
    // `--library <file>` attaches another source file as a library: its names are reachable, but
    // it is not audited as part of the model. The official corpus needs this — one numbered
    // directory imports another, and loading a file alone makes those imports unresolvable.
    let extra_paths = option_all(&options, "--library");
    let loaded = if let Some(project) = &project {
        project.load()?
    } else if extra_paths.is_empty() {
        sysml_syntax::load_project(&refs, true)?
    } else {
        let extra: Vec<String> = extra_paths
            .iter()
            .flat_map(|p| {
                // A directory contributes every .sysml and .kerml file under it.
                let path = std::path::Path::new(p);
                if path.is_dir() {
                    let mut out = Vec::new();
                    collect_sources(path, &mut out);
                    out
                } else {
                    vec![p.clone()]
                }
            })
            .map(|p| fs::read_to_string(&p).unwrap_or_default())
            .collect();
        let extra_refs: Vec<&str> = extra.iter().map(String::as_str).collect();
        sysml_syntax::load_project_with_extra(&refs, &extra_refs)?
    };

    let title = match &project {
        Some(project) => project.name.clone(),
        None => std::path::Path::new(&paths[0])
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&paths[0])
            .to_string(),
    };

    // The ledger path always sits beside the model, so a model copied without its ledger is
    // visibly missing something rather than silently unaccounted.

    // A filter narrows what a view shows; `--json` reports the same narrowed model, so a script and
    // a picture say the same thing.
    let keep = match option_value(&options, "--filter") {
        Some(text) => Some(filter_ids(
            &loaded,
            &sysml_syntax::project::parse_filter(&text)?,
        )?),
        None => None,
    };

    match command.as_str() {
        "outline" => {
            // The pruned tree lives in `filtered`; taking a reference keeps the unfiltered case free
            // of a clone of the whole model.
            let filtered;
            let roots: &[sysml_model::Element] = match &keep {
                Some(keep) => {
                    filtered = sysml_view::keep_only(&loaded.roots, keep);
                    &filtered
                }
                None => &loaded.roots,
            };
            if wants_json(&options) {
                let index = Index::build(roots).with_library(&loaded.library);
                print_json(&machine::outline(roots, &index))?;
            } else {
                print!("{}", sysml_view::outline(roots));
            }
            Ok(ExitCode::from(EXIT_PASS))
        }
        "graph" => {
            let projected = match &keep {
                Some(keep) => sysml_view::project_filtered(&loaded.roots, view, mode, keep),
                None => sysml_view::project(&loaded.roots, view, mode),
            };
            if wants_json(&options) {
                print_json(&machine::graph(&projected))?;
            } else {
                for line in sysml_view::to_lines(&projected) {
                    println!("{line}");
                }
            }
            Ok(ExitCode::from(EXIT_PASS))
        }
        "resolve" => {
            if wants_json(&options) {
                let index = Index::build(&loaded.roots).with_library(&loaded.library);
                let report = machine::resolution(&loaded, &index);
                let code = match report["status"].as_str() {
                    Some("pass") => EXIT_PASS,
                    _ => EXIT_MODEL,
                };
                print_json(&report)?;
                return Ok(ExitCode::from(code));
            }
            print!("{}", report_resolution(&loaded));
            Ok(ExitCode::from(EXIT_PASS))
        }
        "render" => {
            let style = match option_value(&options, "--style").as_deref() {
                None | Some("nested") => sysml_view::layered::Style::Nested,
                Some("tree") => sysml_view::layered::Style::Tree,
                Some(other) => {
                    return Err(format!("unknown style `{other}`: use nested or tree"));
                }
            };
            let svg = match &keep {
                Some(keep) => {
                    let projected = sysml_view::project_filtered(&loaded.roots, view, mode, keep);
                    sysml_view::render_styled(&projected, &title, style)
                }
                None => sysml_view::render_view_styled(&loaded.roots, view, mode, &title, style),
            };
            match out_path {
                Some(target) => {
                    fs::write(&target, svg).map_err(|e| format!("{target}: {e}"))?;
                    println!("wrote {target}");
                }
                None => println!("{svg}"),
            }
            Ok(ExitCode::from(EXIT_PASS))
        }
        "check" => {
            // The machine answer is memoized; the human one is not. `--json` is one string and an
            // exit code, while the report a person reads interleaves stdout with notes it puts on
            // stderr, and replaying that from a file means reproducing two streams in the order a
            // terminal saw them. A second saved is not worth rearranging a reader's output.
            match check_command(&loaded, &ledger_path, &options)? {
                Some(report) => {
                    println!("{}", report.document);
                    if let Some((project, key)) = &memo {
                        cache::store(&project.root, *key, &report.document, report.code);
                    }
                    Ok(ExitCode::from(report.code))
                }
                None => {
                    let (report, count, unsupported) = report_problems(&loaded);
                    print!("{report}");
                    // Which library this run read changes what "clean" means: against the bundled
                    // stub a model can pass while its unit and quantity chain is missing, and the
                    // report used to say nothing about it. A caveat, not an error — the model is not
                    // wrong, the comparison is weaker than it looks.
                    let library_note = loaded.library_source.describe();
                    if loaded.library_source.is_stub() {
                        eprintln!("note: {library_note}");
                    }
                    if unsupported > 0 && count == 0 {
                        eprintln!(
                        "note: {unsupported} construct(s) parsed but not modelled (listed above)"
                    );
                    }
                    // A model whose semantics are clean but whose ledger is empty is not ready, and
                    // the most damaging thing this tool can do is say it is. `check` exited 0 on a
                    // model that `prov` failed with 213 errors, and the model had been written by
                    // an agent that ran `check` and reported success.
                    let ledger = sysml_ledger::Ledger::load(&ledger_path)?;
                    let audit = sysml_ledger::audit(&loaded.roots, &ledger);
                    // An untouched ledger is not a broken model. Saying so with the same code as a
                    // violated rule sends a caller to fix the wrong thing, and the shipped examples
                    // failed their own gate for exactly this reason. The distinction is only made
                    // when the model has no other problem: a model that is both wrong and
                    // unaccounted is wrong first.
                    if count == 0 && audit.not_started() {
                        // `count` is errors only. A model with warnings is not "clean", and printing
                        // that word above the warnings a reader is already looking at makes the tool
                        // sound as if it did not read its own output. The count decides which
                        // sentence; the warnings decide whether "no errors" or "clean" is true.
                        // "then" rather than "next": `sysml next` is a command now, and a line beginning
                        // `next:` reads as one — the reader would type it and get a stage report
                        // instead of the two commands the sentence is offering.
                        eprintln!(
                            "no errors; its ledger has not been started.\n\
                         {errors} value(s) and {parts} part definition(s) have nothing on record.\n\
                         then: `sysml scope {model} --jurisdictions ...` then `sysml decide \
                         {model} --element <name> --origin spec|derived|assumed`.\n\
                         run `sysml prov {model}` for the list of what is missing.",
                            errors = audit.error_count(),
                            parts = audit.unaccounted_parts.len(),
                            model = model_path.display(),
                        );
                        return Ok(ExitCode::from(EXIT_UNFINISHED));
                    }
                    if audit.error_count() > 0 {
                        eprintln!(
                            "{} provenance error(s); run `sysml prov {}` for the list",
                            audit.error_count(),
                            model_path.display()
                        );
                    }
                    if !audit.unaccounted_parts.is_empty() {
                        eprintln!(
                            "{} part definition(s) have no recorded reason to exist",
                            audit.unaccounted_parts.len()
                        );
                    }
                    let blocked = audit.blocking();
                    if blocked > 0 {
                        eprintln!("{blocked} requirement(s) held by an open question");
                    }
                    Ok(ExitCode::from(if count == 0 && audit.error_count() == 0 {
                        EXIT_PASS
                    } else {
                        EXIT_MODEL
                    }))
                }
            }
        }
        "ask" => {
            let mut ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let question =
                option_value(&options, "--question").ok_or("ask needs --question \"...\"")?;
            let at = option_value(&options, "--at").unwrap_or_else(|| "<model>".to_string());
            let blocks: Vec<String> = option_all(&options, "--blocks");
            // A question that claims to block a requirement nobody can find is worse than no
            // claim at all: readiness then looks satisfied for a reason that does not exist.
            let known = sysml_ledger::requirements(&loaded.roots);
            for block in &blocks {
                if !known.iter().any(|r| r == block) {
                    let leaf = block.split("::").last().unwrap_or(block);
                    let close: Vec<&String> = known
                        .iter()
                        .filter(|r| {
                            let r_leaf = r.split("::").last().unwrap_or(r);
                            r_leaf.starts_with(leaf) || leaf.starts_with(r_leaf) || r.contains(leaf)
                        })
                        .collect();
                    let hint = if close.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "; did you mean {}",
                            close
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(" or ")
                        )
                    };
                    return Err(format!(
                        "--blocks `{block}` is not a requirement in this model{hint}. Known requirements: {}",
                        if known.is_empty() { "none".to_string() } else { known.join(", ") }
                    ));
                }
            }
            let id = ledger.next_question_id();
            ledger.questions.push(sysml_ledger::Question {
                id: id.clone(),
                at,
                text: question,
                blocks,
                state: sysml_ledger::QuestionState::Open,
                answer: None,
                answered_by: None,
                asked_by: option_value(&options, "--by").unwrap_or_else(|| "agent".to_string()),
                raised_at: option_value(&options, "--at-time"),
            });
            ledger.model = title.clone();
            ledger.save(&ledger_path)?;
            println!("{id}");
            Ok(ExitCode::from(EXIT_PASS))
        }
        "answer" => {
            let mut ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let id = option_value(&options, "--id").ok_or("answer needs --id Q001")?;
            let text = option_value(&options, "--text").ok_or("answer needs --text \"...\"")?;
            let by = option_value(&options, "--by").unwrap_or_else(|| "human".to_string());
            let state = if option_value(&options, "--defer").is_some() {
                sysml_ledger::QuestionState::Deferred
            } else {
                sysml_ledger::QuestionState::Answered
            };
            let Some(question) = ledger.questions.iter_mut().find(|q| q.id == id) else {
                return Err(format!("no question {id} in {}", ledger_path.display()));
            };
            question.state = state;
            question.answer = Some(text);
            question.answered_by = Some(by);
            ledger.save(&ledger_path)?;
            println!(
                "{id} {}",
                if state == sysml_ledger::QuestionState::Answered {
                    "answered"
                } else {
                    "deferred"
                }
            );
            Ok(ExitCode::from(EXIT_PASS))
        }
        "decide" => {
            let mut ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let element = option_value(&options, "--element")
                .ok_or("decide needs --element <qualified name>")?;
            let origin_text = option_value(&options, "--origin")
                .ok_or("decide needs --origin spec|derived|assumed|asked")?;
            let origin = sysml_ledger::Origin::parse(&origin_text).ok_or_else(|| {
                format!("unknown origin `{origin_text}`; use spec, derived, assumed or asked")
            })?;
            // `--from` names the inputs a derivation was computed from. An input that does not
            // exist, or a misspelt one, makes the derivation look traceable while pointing at
            // nothing — the same defect as an assumption with no falsifier, one level up.
            let inputs: Vec<String> = option_all(&options, "--from");
            if !inputs.is_empty() {
                let known: Vec<String> = sysml_ledger::all_qualified(&loaded.roots);
                for input in &inputs {
                    if !known.iter().any(|k| k == input) {
                        let known_refs: Vec<&str> = known.iter().map(String::as_str).collect();
                        let hint = match nearest(input, &known_refs) {
                            Some(name) => format!("; did you mean `{name}`"),
                            None => "; the model declares no element by that name".to_string(),
                        };
                        return Err(format!(
                            "--from `{input}` is not an element in this model{hint}"
                        ));
                    }
                }
            }
            // A `spec` value claims something about the world, so it has to point at where that
            // claim came from. `assumed` and `derived` do not make such a claim and need no
            // source; requiring one there would push authors toward inventing citations.
            let cites: Vec<String> = option_all(&options, "--cite");
            if origin == sysml_ledger::Origin::Spec && cites.is_empty() {
                return Err(format!(
                    "a `spec` value needs --cite <source id>: without one it is indistinguishable from a guess. Register the material first with `sysml source`."
                ));
            }
            for cite in &cites {
                if ledger.source(cite).is_none() {
                    let known: Vec<&str> = ledger.sources.iter().map(|s| s.id.as_str()).collect();
                    let hint = match nearest(cite, &known) {
                        Some(id) => format!("; did you mean `{id}`"),
                        None => match known.is_empty() {
                            true => "; no material is registered yet — run `sysml source` first"
                                .to_string(),
                            false => format!("; registered ids are {}", known.join(", ")),
                        },
                    };
                    return Err(format!("--cite `{cite}` is not a registered source{hint}"));
                }
            }
            // An approver named here is held to the same rule as `approve`: a person, spelled
            // `human:<name>`. Letting this flag take any string made the separate verb optional
            // — an agent could write its own name into the field and the gate that reports
            // unapproved assumptions would stop reporting it.
            let approved_by = option_value(&options, "--approved-by");
            if let Some(who) = &approved_by {
                sysml_ledger::check_approver(who)?;
                if origin != sysml_ledger::Origin::Assumed {
                    return Err(format!(
                        "--approved-by applies to an assumption; `{origin_text}` is not one. \
                         An assumption is the only thing a person approves, because it is the \
                         only thing nobody has a source for."
                    ));
                }
            }
            // An assumption is the one origin with no source behind it, so the thing that would
            // show it wrong is what makes it checkable at all. The canvas refuses this too; the
            // command line used to accept it, which made `sysml decide` the quieter way to write
            // a guess nobody could ever falsify.
            let falsifier = option_value(&options, "--falsifier").unwrap_or_default();
            if origin == sysml_ledger::Origin::Assumed && falsifier.trim().is_empty() {
                return Err(
                    "an assumption needs --falsifier: what observation would show it wrong"
                        .to_string(),
                );
            }
            let id = ledger.next_decision_id();
            ledger.decisions.push(sysml_ledger::Decision {
                id: id.clone(),
                element,
                value: option_value(&options, "--value").unwrap_or_default(),
                origin,
                why: option_value(&options, "--why").unwrap_or_default(),
                falsifier,
                inputs,
                question: option_value(&options, "--question"),
                decided_by: option_value(&options, "--by").unwrap_or_else(|| "agent".to_string()),
                approved_by,
                cites,
            });
            ledger.model = title.clone();
            ledger.save(&ledger_path)?;
            println!("{id}");
            Ok(ExitCode::from(EXIT_PASS))
        }
        // Register a piece of research so values can cite it. A source carries the passage
        // relied on, because "I read the datasheet" is not a citation — a reader has to be able
        // to see whether the number was read correctly.
        "source" => {
            let mut ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let title = option_value(&options, "--title").ok_or("source needs --title \"...\"")?;
            let kind = option_value(&options, "--kind").unwrap_or_else(|| "other".to_string());
            let quote = option_value(&options, "--quote").unwrap_or_default();
            if quote.trim().is_empty() {
                return Err(
                    "source needs --quote: the passage relied on. A link alone does not let a reader judge whether the value was read correctly"
                        .to_string(),
                );
            }
            let id = ledger.next_source_id();
            ledger.sources.push(sysml_ledger::Source {
                id: id.clone(),
                kind,
                title,
                locator: option_value(&options, "--locator").unwrap_or_default(),
                quote,
                accessed: option_value(&options, "--accessed").unwrap_or_default(),
                jurisdiction: option_value(&options, "--jurisdiction").unwrap_or_default(),
                added_by: option_value(&options, "--by").unwrap_or_else(|| "agent".to_string()),
            });
            ledger.model = model_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            ledger.save(&ledger_path)?;
            println!("{id}");
            Ok(ExitCode::from(EXIT_PASS))
        }
        // Record what the research covered, as distinct from what it found. A ledger of
        // positive results cannot say where the search stopped, so a model citing only US
        // regulations looks identical to one that searched the world and found only those.
        "scope" => {
            let mut ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let market = option_value(&options, "--market");
            let jurisdictions = option_value(&options, "--jurisdictions");
            if market.is_none() && jurisdictions.is_none() {
                return Err("scope needs --market and/or --jurisdictions. \
                     --market is where the product is sold; --jurisdictions is what the \
                     research actually covered (comma separated, e.g. US,EU,CN)"
                    .to_string());
            }
            if let Some(m) = &market {
                ledger.market = m.clone();
            }
            if let Some(j) = &jurisdictions {
                ledger.jurisdictions = j.clone();
            }
            ledger.model = model_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            ledger.save(&ledger_path)?;
            println!(
                "market: {} | jurisdictions searched: {}",
                if ledger.market.is_empty() {
                    "<unset>"
                } else {
                    &ledger.market
                },
                if ledger.jurisdictions.is_empty() {
                    "<unset>"
                } else {
                    &ledger.jurisdictions
                }
            );
            Ok(ExitCode::from(EXIT_PASS))
        }
        // A person accepting an assumption. Separate from `decide` on purpose: the model author
        // must not be able to supply this field for themselves.
        "approve" => {
            let mut ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let id = option_value(&options, "--id").ok_or("approve needs --id D001")?;
            let by = option_value(&options, "--by").ok_or("approve needs --by \"human:<name>\"")?;
            if !by.starts_with("human:") {
                return Err(format!(
                    "approval must name a person as human:<name>; got `{by}`. \
                     An agent cannot approve its own assumption"
                ));
            }
            let decision = ledger.approve(&id, &by)?;
            let (element, value) = (decision.element.clone(), decision.value.clone());
            ledger.save(&ledger_path)?;
            println!("{id} approved by {by}: {element} = {value}");
            Ok(ExitCode::from(EXIT_PASS))
        }
        // Take back an entry that is simply wrong. Hand-editing the JSON is what happened before
        // this existed, and it is the one operation that can leave the file unreadable.
        "retract" => {
            let mut ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let id = option_value(&options, "--id").ok_or("retract needs --id D001")?;
            let removed = ledger
                .remove_decision(&id)
                .ok_or_else(|| format!("no decision {id} in {}", ledger_path.display()))?;
            ledger.save(&ledger_path)?;
            println!(
                "{} retracted: {} = {}",
                removed.id, removed.element, removed.value
            );
            Ok(ExitCode::from(EXIT_PASS))
        }
        "ledger" => {
            let ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            if option_value(&options, "--json").is_some() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&ledger)
                        .map_err(|e| format!("cannot serialise ledger: {e}"))?
                );
                return Ok(ExitCode::from(EXIT_PASS));
            }
            // Scope first: it qualifies everything below it. A reader who does not know the
            // search stopped at one jurisdiction will read the citations as exhaustive.
            println!(
                "market       {}",
                if ledger.market.is_empty() {
                    "<unset>"
                } else {
                    &ledger.market
                }
            );
            println!(
                "searched     {}",
                if ledger.jurisdictions.is_empty() {
                    "<unset>"
                } else {
                    &ledger.jurisdictions
                }
            );
            println!();
            println!("{} questions", ledger.questions.len());
            for q in &ledger.questions {
                let state = match q.state {
                    sysml_ledger::QuestionState::Open => "open",
                    sysml_ledger::QuestionState::Answered => "answered",
                    sysml_ledger::QuestionState::Deferred => "deferred",
                };
                println!("  {} [{}] {}", q.id, state, q.text);
                if !q.blocks.is_empty() {
                    println!("       blocks: {}", q.blocks.join(", "));
                }
                if let Some(answer) = &q.answer {
                    println!(
                        "       answer: {answer} ({})",
                        q.answered_by.as_deref().unwrap_or("unattributed")
                    );
                }
            }
            println!("{} sources", ledger.sources.len());
            for src in &ledger.sources {
                println!("  {} [{}] {}", src.id, src.kind, src.title);
                if !src.locator.is_empty() {
                    println!("       at: {}", src.locator);
                }
                println!("       \"{}\"", src.quote);
            }
            println!("{} decisions", ledger.decisions.len());
            for d in &ledger.decisions {
                let approval = match &d.approved_by {
                    Some(name) => format!("approved by {name}"),
                    None if d.origin == sysml_ledger::Origin::Assumed => "NOT APPROVED".to_string(),
                    None => String::new(),
                };
                println!(
                    "  {} [{}] {} = {}  {}",
                    d.id,
                    d.origin.keyword(),
                    d.element,
                    d.value,
                    approval
                );
                if !d.falsifier.is_empty() {
                    println!("       falsifier: {}", d.falsifier);
                }
            }
            // The judgements go last because they are the only section whose meaning depends
            // on the rest: a verdict is about the numbers recorded above it.
            println!("{} judgement(s)", ledger.judgements.len());
            for j in &ledger.judgements {
                println!(
                    "  {} [{}] {}  against {}",
                    j.id, j.verdict, j.requirement, j.fingerprint
                );
                if !j.detail.is_empty() {
                    println!("       {}", j.detail);
                }
            }
            Ok(ExitCode::from(EXIT_PASS))
        }
        "trace" => {
            let clauses_path = option_value(&options, "--clauses")
                .ok_or("trace needs --clauses <clauses.jsonl>")?;
            let text =
                fs::read_to_string(&clauses_path).map_err(|e| format!("{clauses_path}: {e}"))?;
            let ledger = sysml_ledger::clauses::read(&text)?;
            let report = sysml_ledger::clauses::trace(&loaded.roots, &ledger);
            if wants_json(&options) {
                print_json(&report.to_json())?;
            } else {
                print!("{}", report.render());
            }
            // A dangling citation is a wrong claim in the model, so it is a model error. An
            // uncovered normative clause is not: a model is not required to cover the whole
            // standard, and saying so with exit 1 would fail every partial model.
            Ok(ExitCode::from(if report.dangling.is_empty() {
                EXIT_PASS
            } else {
                EXIT_MODEL
            }))
        }
        "table" => {
            let matrix = options.iter().any(|(k, _)| k == "--matrix");
            let table = if matrix {
                sysml_view::table::requirement_matrix(&loaded.roots)
            } else {
                sysml_view::table::elements(&loaded.roots)
            };
            let csv = sysml_view::table::to_csv(&table);
            match option_value(&options, "--out") {
                Some(path) => {
                    fs::write(&path, &csv).map_err(|e| format!("{path}: {e}"))?;
                    println!("wrote {path} ({} rows)", table.row_count());
                }
                None => print!("{csv}"),
            }
            Ok(ExitCode::from(EXIT_PASS))
        }
        "units" => {
            let values = sysml_ledger::value_bearing(&loaded.roots);
            // What each value comes to, when it comes to anything. A written `Area(2.0, 3.0)` and a
            // written `6.0` are the same number, and the report exists to say what a number is.
            //
            // One checker answers for every value. Asking `unit_of` per value built a checker each
            // time, and a checker indexes the whole standard library: 19.6 seconds for the 92 values
            // of a 514-line model, against 0.6 for its `check`.
            let (computed, units) =
                sysml_semantics::values_and_units(&loaded.roots, &loaded.library);
            let mut readings = Vec::new();
            for (key, text, line) in &values {
                readings.push((key.clone(), text.clone(), *line, units.get(key).cloned()));
            }
            if option_value(&options, "--json").is_some() {
                let rows: Vec<serde_json::Value> = readings
                    .iter()
                    .map(|(key, text, line, reading)| {
                        serde_json::json!({
                            "element": key,
                            "value": text,
                            "line": line,
                            "unit": reading.as_ref().map(|r| r.symbol.clone()).unwrap_or_default(),
                            "dimension": reading.as_ref().and_then(|r| r.dimension.clone()),
                            "checked": reading.as_ref().map(|r| r.known).unwrap_or(false),
                        })
                    })
                    .collect();
                let unchecked = rows
                    .iter()
                    .filter(|r| r["checked"] == serde_json::Value::Bool(false) && r["unit"] != "")
                    .count();
                let doc = serde_json::json!({
                    "ok": true,
                    "values": rows,
                    "unchecked": unchecked,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&doc)
                        .map_err(|e| format!("cannot serialise report: {e}"))?
                );
            } else {
                for (key, text, _line, reading) in &readings {
                    let (unit, dimension, note) = match reading {
                        Some(r) if r.known => (
                            r.symbol.clone(),
                            r.dimension.clone().unwrap_or_default(),
                            String::new(),
                        ),
                        // A unit outside the table is not a bare number. Saying "unchecked" keeps
                        // the reader from reading the absence of a dimension as "dimensionless".
                        Some(r) if !r.symbol.is_empty() => (
                            r.symbol.clone(),
                            String::new(),
                            "  (unit not in the table; comparisons involving it stay undecided)"
                                .to_string(),
                        ),
                        _ => (String::new(), String::new(), String::new()),
                    };
                    // The arrow appears only when the number is not already what is written: `0.3`
                    // and `10.0 [N]` need no gloss, `Area(2.0, 3.0)` does. A written number with a
                    // unit bracket is compared with the bracket cut off, because the bracket is the
                    // unit's spelling rather than part of the value.
                    let written = text.split('[').next().unwrap_or(text).trim();
                    let computed = match computed.get(key) {
                        Some(value) if written.parse::<f64>().is_err() => {
                            format!("   -> {}", value.render())
                        }
                        _ => String::new(),
                    };
                    if unit.is_empty() {
                        println!("{key} = {text}{computed}");
                    } else {
                        println!("{key} = {text}{computed}   [{unit} = {dimension}]{note}");
                    }
                }
            }
            Ok(ExitCode::from(EXIT_PASS))
        }
        "prov" => {
            let ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let audit = sysml_ledger::audit(&loaded.roots, &ledger);
            let semantic = sysml_semantics::check(&loaded.roots, &loaded.library);
            if option_value(&options, "--json").is_some() {
                let issues: Vec<serde_json::Value> = audit
                    .issues
                    .iter()
                    .map(|i| {
                        serde_json::json!({
                            "code": i.code,
                            "severity": match i.severity {
                                sysml_ledger::Severity::Error => "error",
                                sysml_ledger::Severity::Warning => "warning",
                            },
                            "at": i.at,
                            "message": i.message,
                        })
                    })
                    .collect();
                let unaccounted: Vec<&String> = audit.unaccounted();
                let blocked: Vec<serde_json::Value> = audit
                    .readiness
                    .iter()
                    .filter(|r| !r.open_questions.is_empty())
                    .map(|r| {
                        serde_json::json!({
                            "requirement": r.requirement,
                            "openQuestions": r.open_questions,
                        })
                    })
                    .collect();
                let doc = serde_json::json!({
                    "ok": audit.error_count() == 0,
                    "errorCount": audit.error_count(),
                    "warningCount": audit
                        .issues
                        .iter()
                        .filter(|i| i.severity == sysml_ledger::Severity::Warning)
                        .count(),
                    "unaccounted": unaccounted,
                    "unaccountedParts": audit.unaccounted_parts,
                    "blocked": blocked,
                    "issues": issues,
                    "summary": sysml_ledger::summarise(&audit, &ledger, &semantic.verdicts),
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&doc)
                        .map_err(|e| format!("cannot serialise report: {e}"))?
                );
            } else {
                for issue in &audit.issues {
                    println!("{}", issue.render());
                }
                if audit.error_count() > 0 {
                    eprintln!(
                        "{} provenance error(s); the model's numbers are not accounted for",
                        audit.error_count()
                    );
                }
                // Part definitions get their own line. They fail for a different reason than a
                // number does — nobody recorded deciding the thing should exist — and burying
                // them in a count of number errors makes the distinction invisible.
                if !audit.unaccounted_parts.is_empty() {
                    eprintln!(
                        "{} part definition(s) have no recorded reason to exist; `sysml decide \
                         <model> --element <Part> --origin assumed --why ...` records one",
                        audit.unaccounted_parts.len()
                    );
                }
            }
            // The ledger gate has a third state, and `prov` is the command a caller runs after
            // `check` says the ledger is unfinished, so it has to answer the same way `check` did.
            Ok(ExitCode::from(if audit.error_count() == 0 {
                EXIT_PASS
            } else if audit.not_started() {
                EXIT_UNFINISHED
            } else {
                EXIT_MODEL
            }))
        }
        "next" => {
            let ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let audit = sysml_ledger::audit(&loaded.roots, &ledger);
            let semantic = sysml_semantics::check(&loaded.roots, &loaded.library);
            let facts =
                sysml_studio::next::gather(&loaded.roots, &model_path, &ledger, &audit, &semantic);
            let lang = match option_value(&options, "--lang") {
                None => sysml_studio::translate::Lang::En,
                Some(code) => sysml_studio::translate::Lang::parse(&code)
                    .ok_or_else(|| format!("unknown language {code}: use en or zh"))?,
            };
            let plan = sysml_studio::next::plan(&facts, lang);
            if wants_json(&options) {
                print_json(&machine::next(&plan, &facts))?;
            } else {
                print!("{}", sysml_studio::next::render(&plan));
            }
            // Nothing here is an error: a model mid-build is the normal state, and an exit code
            // that failed on it would make every build look broken.
            Ok(ExitCode::from(EXIT_PASS))
        }
        "status" => {
            let ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let audit = sysml_ledger::audit(&loaded.roots, &ledger);
            let semantic = sysml_semantics::check(&loaded.roots, &loaded.library);
            let summary = sysml_ledger::summarise(&audit, &ledger, &semantic.verdicts);
            if wants_json(&options) {
                let report = machine::status(&summary, &audit, semantic.error_count());
                let code = match report["status"].as_str() {
                    Some("pass") => EXIT_PASS,
                    Some("unfinished") => EXIT_UNFINISHED,
                    _ => EXIT_MODEL,
                };
                print_json(&report)?;
                return Ok(ExitCode::from(code));
            }
            print!("{}", summary);
            if option_value(&options, "--issues").is_some() {
                for issue in &audit.issues {
                    println!("{}", issue.render());
                }
                for issue in &semantic.issues {
                    println!("{}", issue.render());
                }
            }
            // `status` prints a summary, but a summary that reports 213 errors and then exits 0
            // is worse than no status command at all: anything reading the exit code concludes
            // the model is fine. It reports on both gates, so it fails if either one does.
            Ok(ExitCode::from(
                if audit.error_count() == 0 && semantic.error_count() == 0 {
                    EXIT_PASS
                } else if semantic.error_count() == 0 && audit.not_started() {
                    EXIT_UNFINISHED
                } else {
                    EXIT_MODEL
                },
            ))
        }
        "review" => {
            let ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let audit = sysml_ledger::audit(&loaded.roots, &ledger);
            let semantic = sysml_semantics::check(&loaded.roots, &loaded.library);
            let values = sysml_ledger::value_bearing(&loaded.roots);
            let diagram = sysml_view::render_view(&loaded.roots, view, mode, &title);
            let html = review::render(&title, &semantic, &ledger, &audit, &values, Some(&diagram));
            let target = out_path.unwrap_or_else(|| {
                model_path
                    .with_extension("review.html")
                    .to_string_lossy()
                    .into_owned()
            });
            fs::write(&target, html).map_err(|e| format!("{target}: {e}"))?;
            println!("wrote {target}");
            Ok(ExitCode::from(EXIT_PASS))
        }
        // The canvas is a server because the edit has to be verified by the same loader the CLI
        // uses, and that lives in this process. Serving one project on loopback keeps the
        // verification step in the loop instead of trusting the browser's copy of the model.
        "studio" => {
            let port = option_value(&options, "--port")
                .map(|p| p.parse::<u16>().map_err(|e| format!("--port: {e}")))
                .transpose()?
                .unwrap_or(7373);
            // A project is opened as the set of files it is. Handing the canvas its manifest would
            // open a JSON file as a model: the manifest is where the sources are listed, not one
            // of them.
            let open: Vec<std::path::PathBuf> = match &project {
                Some(project) => project.sources.clone(),
                None => paths.iter().map(std::path::PathBuf::from).collect(),
            };
            sysml_studio::serve(open, port)?;
            Ok(ExitCode::from(EXIT_PASS))
        }
        "impact" => {
            let element = option_value(&options, "--element")
                .ok_or("impact needs --element <qualified name>")?;
            let reached = sysml_diff::impact_in(&loaded.roots, &loaded.library, &element)?;
            if wants_json(&options) {
                let dependants: Vec<serde_json::Value> = reached
                    .iter()
                    .map(|d| {
                        serde_json::json!({
                            "qualified": d.qualified,
                            "kind": d.kind,
                            "because": d.because,
                            "distance": d.distance,
                        })
                    })
                    .collect();
                let doc = serde_json::json!({
                    "element": element,
                    "dependants": dependants,
                });
                print_json(&doc)?;
                return Ok(ExitCode::from(EXIT_PASS));
            }
            if reached.is_empty() {
                println!("nothing depends on {element}");
                return Ok(ExitCode::from(EXIT_PASS));
            }
            println!("{} element(s) depend on {element}", reached.len());
            for dependant in &reached {
                let hop = match dependant.distance {
                    1 => String::new(),
                    n => format!(" ({} hops)", n),
                };
                println!(
                    "  {}  {}{}  {}",
                    dependant.kind, dependant.qualified, hop, dependant.because
                );
            }
            Ok(ExitCode::from(EXIT_PASS))
        }
        "judge" => {
            let mut ledger = sysml_ledger::Ledger::load(&ledger_path)?;
            let semantic = sysml_semantics::check(&loaded.roots, &loaded.library);
            let current = sysml_ledger::fingerprint(&loaded.roots);
            // One requirement when `--requirement` names one; otherwise every requirement that
            // has a verdict. A judgement is about a requirement, so a model with none is not
            // something this command can do anything with.
            let only = option_value(&options, "--requirement");
            if semantic.verdicts.is_empty() {
                return Err(format!(
                    "no requirement in {model} has a predicate to judge; a judgement records \
                     what a requirement's constraint evaluated to",
                    model = model_path.display()
                ));
            }
            let mut written = 0;
            for verdict in &semantic.verdicts {
                if let Some(name) = &only {
                    if &verdict.qualified != name {
                        continue;
                    }
                }
                let detail = match &verdict.verdict {
                    sysml_semantics::Verdict::Pass(_) => String::new(),
                    sysml_semantics::Verdict::Fail(value) => {
                        format!("evaluates {}", value.render())
                    }
                    sysml_semantics::Verdict::Inconclusive(why) => why.clone(),
                    sysml_semantics::Verdict::Error(why) => why.clone(),
                };
                let id = ledger.next_judgement_id();
                ledger.judgements.push(sysml_ledger::Judgement {
                    id,
                    requirement: verdict.qualified.clone(),
                    verdict: verdict.verdict.keyword().to_string(),
                    detail,
                    fingerprint: current.clone(),
                    judged_by: option_value(&options, "--by")
                        .unwrap_or_else(|| "agent".to_string()),
                    at: option_value(&options, "--at-time").unwrap_or_default(),
                });
                written += 1;
            }
            if written == 0 {
                return Err(match only {
                    Some(name) => format!("no requirement `{name}` in {}", model_path.display()),
                    None => "nothing judged".to_string(),
                });
            }
            ledger.model = title.clone();
            ledger.save(&ledger_path)?;

            let stale = sysml_ledger::stale(&ledger, &current);
            if wants_json(&options) {
                let doc = serde_json::json!({
                    "written": written,
                    "fingerprint": current,
                    "stale": stale
                        .iter()
                        .map(|s| serde_json::json!({
                            "id": s.id,
                            "requirement": s.requirement,
                            "verdict": s.verdict,
                            "judgedAgainst": s.judged_against,
                            "current": s.current,
                        }))
                        .collect::<Vec<_>>(),
                });
                print_json(&doc)?;
                return Ok(ExitCode::from(EXIT_PASS));
            }
            println!("{written} judgement(s) recorded against model {current}");
            // Records made against an earlier model are reported rather than silently kept: a
            // verdict about numbers that have since moved is the thing this whole mechanism
            // exists to make visible.
            if !stale.is_empty() {
                println!("{} earlier judgement(s) are stale:", stale.len());
                for record in &stale {
                    println!(
                        "  {} {} was `{}` against {}; the model is now {}",
                        record.id,
                        record.requirement,
                        record.verdict,
                        record.judged_against,
                        record.current
                    );
                }
                println!(
                    "re-run `sysml judge {model}` to judge the current model",
                    model = model_path.display()
                );
            }
            Ok(ExitCode::from(EXIT_PASS))
        }
        other => Err(format!("unknown command: {other}")),
    }
}

/// What `check --json` decided, so the caller does not have to read its own output back.
struct JsonReport {
    document: String,
    /// The exit code, decided from the facts rather than from the printed text.
    code: u8,
}

/// `sysml diff <before> <after>` — what moved between two versions of a model.
///
/// The comparison is by qualified name, not by line, so moving a declaration does not register
/// and renaming one registers as a removal plus an addition. That is deliberate: the name is the
/// identity, and the model carries no record of a rename to follow.
fn diff_command(
    before: &str,
    after: &str,
    options: &[(String, String)],
) -> Result<ExitCode, String> {
    // The same `--library` the other commands take. A project split across files is the normal
    // case, and a `diff` that loaded one file per side answered about a model that is not the one
    // in the repository: references to a type in a sibling file resolved to nothing, so a change
    // there never reached the requirement it touches.
    let extra: Vec<String> = option_all(options, "--library")
        .iter()
        .flat_map(|p| {
            let path = std::path::Path::new(p);
            if path.is_dir() {
                let mut out = Vec::new();
                collect_sources(path, &mut out);
                out
            } else {
                vec![p.clone()]
            }
        })
        .map(|p| fs::read_to_string(&p).unwrap_or_default())
        .collect();
    let extra_refs: Vec<&str> = extra.iter().map(String::as_str).collect();
    let read = |path: &str| -> Result<sysml_syntax::Loaded, String> {
        let source = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        sysml_syntax::load_project_with_extra(&[&source], &extra_refs)
            .map_err(|e| format!("{path}: {e}"))
    };
    let old = read(before)?;
    let new = read(after)?;
    let delta = sysml_diff::delta(&old.roots, &new.roots);

    // A change that reaches no requirement is internal. Reporting which requirements it does
    // reach is the answer to "do I need to re-read anything", and it costs one more walk, so it
    // is always included rather than hidden behind a flag.
    let touched = delta.touched();
    let requirements = sysml_diff::affected_requirements(&new.roots, &touched);

    if wants_json(options) {
        let add: Vec<serde_json::Value> = delta
            .added
            .iter()
            .map(|s| serde_json::json!({"qualified": s.qualified, "kind": s.kind}))
            .collect();
        let remove: Vec<serde_json::Value> = delta
            .removed
            .iter()
            .map(|s| serde_json::json!({"qualified": s.qualified, "kind": s.kind}))
            .collect();
        let change: Vec<serde_json::Value> = delta
            .changed
            .iter()
            .map(|c| {
                serde_json::json!({
                    "qualified": c.qualified,
                    "kind": c.kind,
                    "fields": c.fields,
                    "before": {
                        "type": c.before.type_ref,
                        "supertypes": c.before.supertypes,
                        "value": c.before.value,
                        "children": c.before.children,
                    },
                    "after": {
                        "type": c.after.type_ref,
                        "supertypes": c.after.supertypes,
                        "value": c.after.value,
                        "children": c.after.children,
                    },
                })
            })
            .collect();
        let doc = serde_json::json!({
            "status": if delta.is_empty() { "pass" } else { "changed" },
            "added": add,
            "removed": remove,
            "changed": change,
            "requirements": requirements,
        });
        print_json(&doc)?;
        return Ok(ExitCode::from(EXIT_PASS));
    }

    if delta.is_empty() {
        println!(
            "no change: {} and {} declare the same elements",
            before, after
        );
        return Ok(ExitCode::from(EXIT_PASS));
    }
    section(
        &format!("added ({})", delta.added.len()),
        &delta
            .added
            .iter()
            .map(|s| format!("  {}  {}", s.kind, s.qualified))
            .collect::<Vec<_>>(),
    );
    section(
        &format!("removed ({})", delta.removed.len()),
        &delta
            .removed
            .iter()
            .map(|s| format!("  {}  {}", s.kind, s.qualified))
            .collect::<Vec<_>>(),
    );
    section(
        &format!("changed ({})", delta.changed.len()),
        &delta
            .changed
            .iter()
            .map(|c| {
                let detail = c
                    .fields
                    .iter()
                    .map(|f| match *f {
                        "type" => format!(
                            "type {} -> {}",
                            c.before.type_ref.as_deref().unwrap_or("(none)"),
                            c.after.type_ref.as_deref().unwrap_or("(none)")
                        ),
                        "children" => {
                            format!("children {} -> {}", c.before.children, c.after.children)
                        }
                        "value" => format!(
                            "value {} -> {}",
                            c.before.value.as_deref().unwrap_or("(none)"),
                            c.after.value.as_deref().unwrap_or("(none)")
                        ),
                        other => other.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("  {}  {} ({})", c.kind, c.qualified, detail)
            })
            .collect::<Vec<_>>(),
    );
    section(
        &format!("requirements reached ({})", requirements.len()),
        &requirements
            .iter()
            .map(|r| format!("  {r}"))
            .collect::<Vec<_>>(),
    );
    Ok(ExitCode::from(EXIT_PASS))
}

/// One heading and its lines, or a line saying there are none.
///
/// An empty section is printed rather than skipped: "no requirements were reached" is an answer,
/// and a reader who cannot tell it apart from "this section was not computed" will assume the
/// second.
fn section(heading: &str, lines: &[String]) {
    println!("\n{heading}");
    if lines.is_empty() {
        println!("  (none)");
    } else {
        for line in lines {
            println!("{line}");
        }
    }
}

/// The ids an annotation filter keeps.
///
/// The parse and the expression both come from `sysml-syntax`, which is where the model's own parser
/// lives: a filter typed on the command line and a filter typed into the canvas have to mean one
/// thing, and two copies of the parser would eventually mean two.
fn filter_ids(
    loaded: &sysml_syntax::Loaded,
    filter: &sysml_syntax::project::Filter,
) -> Result<std::collections::HashSet<usize>, String> {
    let constraint = match &filter.constraint {
        Some(text) => Some(sysml_syntax::project::constraint_expr(text)?),
        None => None,
    };
    Ok(sysml_semantics::annotated_by(
        &loaded.roots,
        &loaded.library,
        &filter.name,
        constraint.as_ref(),
    ))
}

/// `check --json` returns the machine-readable report; without the flag it returns `None` and
/// the caller prints the human form. Both come from the same facts.
fn check_command(
    loaded: &sysml_syntax::Loaded,
    ledger_path: &Path,
    options: &[(String, String)],
) -> Result<Option<JsonReport>, String> {
    if option_value(options, "--json").is_none() {
        return Ok(None);
    }
    let semantic = sysml_semantics::check(&loaded.roots, &loaded.library);
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    let mut issues: Vec<serde_json::Value> = Vec::new();
    let mut error_total = semantic
        .issues
        .iter()
        .filter(|i| i.severity == sysml_semantics::Severity::Error)
        .count();
    for reference in index.references(&loaded.roots) {
        let kind = match &reference.target {
            Resolution::Found(_) => continue,
            Resolution::Ambiguous(_) => "ambiguous",
            Resolution::Unresolved => "unresolved",
            Resolution::NotAccessible => "not accessible",
        };
        error_total += 1;
        issues.push(serde_json::json!({
            "code": if reference.in_predicate { "REF002" } else { "REF001" },
            "severity": "error",
            "at": index.qualified[reference.from],
            "line": index.line[reference.from],
            "message": format!("{kind}: {}", reference.text),
        }));
    }
    for issue in &semantic.issues {
        issues.push(serde_json::json!({
            "code": issue.code,
            "severity": match issue.severity {
                sysml_semantics::Severity::Error => "error",
                sysml_semantics::Severity::Warning => "warning",
            },
            "at": issue.at,
            "line": issue.line,
            "message": issue.message,
        }));
    }
    let verdicts: Vec<serde_json::Value> = semantic
        .verdicts
        .iter()
        .map(|v| {
            serde_json::json!({
                "requirement": v.qualified,
                "line": v.line,
                "verdict": v.verdict.keyword(),
                "detail": match &v.verdict {
                    sysml_semantics::Verdict::Pass(_) => String::new(),
                    sysml_semantics::Verdict::Fail(x) => format!("evaluates {}", x.render()),
                    sysml_semantics::Verdict::Inconclusive(why) => why.clone(),
                    sysml_semantics::Verdict::Error(why) => why.clone(),
                },
                "modelRefs": v.model_refs,
            })
        })
        .collect();
    let mut not_modelled: Vec<serde_json::Value> = Vec::new();
    // Project elements only. The standard library is not the model under review, and it is
    // written in KerML constructs this layer does not model, so counting its markers would fill
    // every report with hundreds of `succession` entries that nobody can act on — and would hide
    // the handful that really are in the project.
    let project_end = index.len() - index.library_len();
    for id in 0..project_end {
        if index.kind[id] != sysml_model::Kind::Unsupported {
            continue;
        }
        // A parse failure is an error, not an unmodelled construct, and `Checker::issues` already
        // reports it under `SYN001`. Listing it here as well counted one defect in two channels,
        // and the channel a reader consults for the verdict (`issues`) used to be the empty one.
        if index.name[id] == "syntax error" {
            continue;
        }
        not_modelled.push(serde_json::json!({
            "at": index.qualified[id],
            "line": index.line[id],
            "construct": index.name[id],
        }));
    }
    not_modelled.sort_by(|a, b| a["at"].as_str().cmp(&b["at"].as_str()));

    // The ledger is part of the verdict, not an annex to it. An agent reads `--json` and trusts
    // `ok`; when this field meant "semantics only" it reported ok on a model with 213 provenance
    // errors, which is the exact failure the whole crate exists to prevent.
    let ledger = sysml_ledger::Ledger::load(ledger_path)?;
    let audit = sysml_ledger::audit(&loaded.roots, &ledger);
    let provenance_errors = audit.error_count();
    let part_errors = audit.unaccounted_parts.len();
    let semantic_error_count = error_total;
    error_total += provenance_errors;
    // `status` carries the same four values a caller acts on, so `--json` can be read without
    // parsing the human sentence and without guessing what "ok" excluded.
    let status = if error_total == 0 {
        "pass"
    } else if semantic_error_count == 0 && audit.not_started() {
        "unfinished"
    } else {
        "failed"
    };

    let doc = serde_json::json!({
        "ok": error_total == 0,
        "status": status,
        "errorCount": error_total,
        "semanticErrors": semantic_error_count,
        "provenanceErrors": provenance_errors,
        "ledgerStarted": !audit.not_started(),
        "warningCount": semantic
            .issues
            .iter()
            .filter(|i| i.severity == sysml_semantics::Severity::Warning)
            .count(),
        "issues": issues,
        "provenanceIssues": audit
            .issues
            .iter()
            .filter(|i| i.severity == sysml_ledger::Severity::Error)
            .map(|i| serde_json::json!({
                "code": i.code,
                "at": i.at,
                "message": i.message,
            }))
            .collect::<Vec<_>>(),
        "unaccountedParts": audit.unaccounted_parts,
        "unaccountedPartCount": part_errors,
        "blocked": audit.blocking(),
        "verdicts": verdicts,
        "notModelled": not_modelled,
    });
    let code = match status {
        "pass" => EXIT_PASS,
        "unfinished" => EXIT_UNFINISHED,
        _ => EXIT_MODEL,
    };
    Ok(Some(JsonReport {
        document: serde_json::to_string_pretty(&doc)
            .map_err(|e| format!("cannot serialise report: {e}"))?,
        code,
    }))
}
