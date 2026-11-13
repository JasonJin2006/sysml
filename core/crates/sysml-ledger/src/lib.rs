//! The two ledgers a model needs beside it, and the gate that reads them.
//!
//! A model states numbers. It does not say where they came from, and that is the whole failure
//! this crate exists for: an experiment over five models found 59 decisions the user never
//! authorised, of which only 44 left a retrievable trace, and the gate in use could not tell a
//! specification value from a guess.
//!
//! So two files live beside the model.
//!
//! * The **question ledger** records what the model does not yet know: the question, which
//!   requirements it blocks, whether it was answered, and by whom. A question that blocks a
//!   requirement holds the whole model back — that is the point of recording it rather than
//!   carrying on.
//! * The **decision ledger** records where every number came from: `spec` (the requirement said
//!   so), `derived` (computed from named inputs), `assumed` (a judgement, with what would prove
//!   it wrong), or `asked` (a human was asked, and the question id is on record).
//!
//! `check` then refuses a model in which a number has no ledger entry, an assumed number has no
//! falsifier, a derived number names an input that is not itself accounted for, or an asked
//! number points at a question nobody answered.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use sysml_model::{Element, Kind};

pub mod clauses;

/// One decision about one value, and where it came from.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// The requirement, a standard, or a supplier document states this value.
    Spec,
    /// Computed from named inputs, all of which are themselves in this ledger.
    Derived,
    /// A judgement. Allowed, but it must say what would show it wrong.
    Assumed,
    /// A human was asked; the question id is recorded and the question is answered.
    Asked,
}

impl Origin {
    pub fn parse(text: &str) -> Option<Origin> {
        match text.trim().to_ascii_lowercase().as_str() {
            "spec" => Some(Origin::Spec),
            "derived" => Some(Origin::Derived),
            "assumed" => Some(Origin::Assumed),
            "asked" => Some(Origin::Asked),
            _ => None,
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            Origin::Spec => "spec",
            Origin::Derived => "derived",
            Origin::Assumed => "assumed",
            Origin::Asked => "asked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionState {
    Open,
    Answered,
    Deferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    /// Qualified name of the element the question is about.
    pub at: String,
    pub text: String,
    /// Qualified names of the requirements this question blocks. While any of these is
    /// non-empty and the question is open, those requirements cannot be called ready.
    #[serde(default)]
    pub blocks: Vec<String>,
    pub state: QuestionState,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub answered_by: Option<String>,
    /// Who raised it: an agent, or a person.
    pub asked_by: String,
    #[serde(default)]
    pub raised_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    /// Qualified name of the element that carries the value.
    pub element: String,
    /// The value as written in the model, for a reader who is comparing.
    pub value: String,
    pub origin: Origin,
    /// Why this value, in one sentence. Empty is allowed for `spec` only.
    #[serde(default)]
    pub why: String,
    /// For `assumed`: what observation would show the value is wrong. Required.
    #[serde(default)]
    pub falsifier: String,
    /// For `derived`: the qualified names it was computed from. Each must appear in this ledger
    /// or be a specification constant.
    #[serde(default)]
    pub inputs: Vec<String>,
    /// For `asked`: the question that settled it.
    #[serde(default)]
    pub question: Option<String>,
    /// `agent` or `human:<name>`. Who put the number in the model.
    pub decided_by: String,
    /// `human:<name>` when a person approved it, `null` when nobody did. An `assumed` value
    /// with no approver is exactly the case the whole ledger exists to make visible.
    #[serde(default)]
    pub approved_by: Option<String>,
    /// Sources this value was read out of: ids from the research ledger.
    ///
    /// A `spec` value with no source is an assertion about the world with nothing behind it —
    /// it reads exactly like a value copied from a standard, which is why the two have to be
    /// distinguishable. `assumed` and `derived` do not need one; they make no claim about
    /// anything outside the model.
    #[serde(default)]
    pub cites: Vec<String>,
}

/// One piece of research: where it came from, what it said, and how to check it again.
///
/// The point is not to collect links. A source is registered so a value can cite it, and so a
/// reader can decide whether the value was read correctly — which needs the locator, not just
/// the URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    /// What kind of material this is: `standard`, `datasheet`, `paper`, `product`, `regulation`,
    /// `expert`. Free text, but the distinctions matter to a reader judging the value.
    pub kind: String,
    pub title: String,
    /// Where it lives: URL, file path inside the workspace, or a document number.
    #[serde(default)]
    pub locator: String,
    /// The passage relied on, quoted. A source with no quote backs nothing.
    #[serde(default)]
    pub quote: String,
    /// When it was read. A number from a 2019 datasheet and one from this year are different
    /// facts, and only the date makes that visible.
    #[serde(default)]
    pub accessed: String,
    /// Which legal regime the material belongs to: `US`, `EU`, `CN`, `INTL`, or empty when the
    /// question does not apply (a material datasheet is not jurisdictional).
    ///
    /// This field exists because a ledger can record what was found but not where it was
    /// looked, so twenty-one US and EU regulations read exactly like "the whole world was
    /// searched and this is all there is". The flight deck cited seven US/EU instruments and
    /// nothing else, and no one could tell from the ledger that China had never been queried.
    #[serde(default)]
    pub jurisdiction: String,
    #[serde(default)]
    pub added_by: String,
}

/// One requirement judged against a specific state of the model.
///
/// A verdict is not a property of the requirement; it is a property of the requirement *and the
/// numbers it reads*. `endurance >= 20` passes or fails depending on what the battery weighs, and
/// that weight changes. Storing only the outcome produces a ledger that is a passing grade from
/// some earlier day — the failure mode `docs/proposal-v0.md` names as the reason to keep stale
/// state at all.
///
/// So each record carries the fingerprint of the model it was computed against. When the model
/// moves, the record is stale, and saying so costs nothing: the verdicts are recomputed on every
/// run anyway, and the only thing that was missing is the ability to tell "judged, and still
/// true" from "judged, but about an older model".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Judgement {
    pub id: String,
    /// The requirement this is about, by qualified name.
    pub requirement: String,
    /// `pass`, `fail`, `inconclusive` or `error`.
    pub verdict: String,
    /// What the expression evaluated to, or why nothing could be said. Empty for `pass`.
    #[serde(default)]
    pub detail: String,
    /// Fingerprint of the model this was computed against.
    pub fingerprint: String,
    /// Who ran it. `agent` unless a person did.
    #[serde(default)]
    pub judged_by: String,
    /// When, as the caller's own timestamp. Empty when they did not say — a fabricated time is
    /// worse than an absent one.
    #[serde(default)]
    pub at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub questions: Vec<Question>,
    #[serde(default)]
    pub decisions: Vec<Decision>,
    #[serde(default)]
    pub judgements: Vec<Judgement>,
    #[serde(default)]
    pub sources: Vec<Source>,
    /// Where this product is meant to be sold and flown. Empty means nobody has decided.
    ///
    /// The market has to be settled before a regulation can pick the load factors, because the
    /// same part is sized differently under Part 103, SC-VTOL and a consumer CE/FCC route. A
    /// model that cites regulations without stating a market has picked its own answer to a
    /// question the user was never asked.
    #[serde(default)]
    pub market: String,
    /// The legal regimes the research actually covered, comma separated: `US,EU` or `CN`.
    ///
    /// Separate from `market` because they answer different questions. `market` is what was
    /// decided; `jurisdictions` is what was looked at. A model can truthfully say it searched
    /// only the US — that is a fact, and stating it is not an error. Claiming completeness is.
    #[serde(default)]
    pub jurisdictions: String,
}

impl Ledger {
    pub fn load(path: &Path) -> Result<Ledger, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ledger::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("{}: {e}", parent.display()))?;
            }
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| format!("cannot serialise ledger: {e}"))?;
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The next free question id.
    ///
    /// Counting entries is wrong as soon as one is removed: a ledger that once had Q001..Q003
    /// and lost Q002 would hand out `Q003` again and silently make every reference ambiguous.
    /// The highest number already in use is the only value that cannot collide.
    pub fn next_question_id(&self) -> String {
        format!(
            "Q{:03}",
            next_number(self.questions.iter().map(|q| q.id.as_str()))
        )
    }

    pub fn next_decision_id(&self) -> String {
        format!(
            "D{:03}",
            next_number(self.decisions.iter().map(|d| d.id.as_str()))
        )
    }

    pub fn next_source_id(&self) -> String {
        format!(
            "S{:03}",
            next_number(self.sources.iter().map(|s| s.id.as_str()))
        )
    }

    /// The next judgement id, `J001` onward.
    ///
    /// Reused numbers are refused for the same reason decisions refuse them: a retracted
    /// entry and its replacement would share an id, and a reader following an id would
    /// land on whichever one survived.
    pub fn next_judgement_id(&self) -> String {
        format!(
            "J{:03}",
            next_number(self.judgements.iter().map(|j| j.id.as_str()))
        )
    }

    pub fn source(&self, id: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    /// The ledger that belongs beside a model: same stem, `.ledger.json`.
    pub fn path_for(model: &Path) -> PathBuf {
        let stem = model
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("model");
        let parent = model.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!("{stem}.ledger.json"))
    }

    pub fn question(&self, id: &str) -> Option<&Question> {
        self.questions.iter().find(|q| q.id == id)
    }

    /// Record that a person accepted an assumption.
    ///
    /// Approval is the one field a model author cannot fill in for themselves: it is the
    /// difference between "the tool guessed and nobody noticed" and "the tool guessed and a
    /// person took the risk". So it is a separate verb, it names the person, and it refuses to
    /// overwrite an approval that already exists — a second name would erase who actually said
    /// yes.
    pub fn approve(&mut self, id: &str, who: &str) -> Result<&Decision, String> {
        check_approver(who)?;
        let decision = self
            .decisions
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| format!("no decision {id} in this ledger"))?;
        if decision.origin != Origin::Assumed {
            return Err(format!(
                "{} is `{}`, not an assumption; only an assumption needs approval",
                decision.id,
                decision.origin.keyword()
            ));
        }
        if let Some(existing) = &decision.approved_by {
            return Err(format!(
                "{} is already approved by {existing}; approvals are not overwritten",
                decision.id
            ));
        }
        decision.approved_by = Some(who.to_string());
        Ok(decision)
    }

    /// Remove a decision by id, returning what was removed.
    ///
    /// Needed because a ledger entry can be simply wrong — an agent that filed a value under the
    /// wrong element had no way to take it back, so it edited the JSON by hand. A verb is safer
    /// than hand-editing: it cannot leave the file half-valid.
    pub fn remove_decision(&mut self, id: &str) -> Option<Decision> {
        let at = self.decisions.iter().position(|d| d.id == id)?;
        Some(self.decisions.remove(at))
    }

    pub fn decisions_for<'a>(&'a self, element: &'a str) -> Vec<&'a Decision> {
        self.decisions
            .iter()
            .filter(|d| d.element == element)
            .collect()
    }
}

/// Check that an approval names a person.
///
/// Approval is the one field a model author cannot supply for themselves — that is the whole
/// point of [`Ledger::approve`] being a separate verb. Enforcing it in the CLI alone left a
/// second way to write the same field: `decide --approved-by`, which took any string and then
/// silenced the gate that reports unapproved assumptions. A single check here reaches every
/// writer, including a future one.
///
/// # Errors
///
/// Returns a message that names the accepted form when `who` does not start with `human:`, or is
/// only `human:` with no name after it.
pub fn check_approver(who: &str) -> Result<(), String> {
    if who.starts_with("human:") && who.len() > "human:".len() {
        Ok(())
    } else if who.starts_with("human:") {
        Err("approval must name a person as human:<name>; `human:` alone names nobody".to_string())
    } else {
        Err(format!(
            "approval must name a person as human:<name>; got `{who}`. \
             An agent cannot approve its own assumption"
        ))
    }
}

/// What the gate found.
#[derive(Debug, Clone)]
pub struct LedgerIssue {
    pub code: &'static str,
    pub severity: Severity,
    pub at: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl LedgerIssue {
    pub fn render(&self) -> String {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        format!("{}: {}[{}]: {}", self.at, level, self.code, self.message)
    }
}

/// A requirement's readiness: whether anything blocking it is still open.
#[derive(Debug, Clone)]
pub struct Readiness {
    pub requirement: String,
    pub open_questions: Vec<String>,
}

pub struct Audit {
    pub issues: Vec<LedgerIssue>,
    /// Every value-bearing attribute the model declares, with whether the ledger accounts for it.
    pub accounted: BTreeMap<String, bool>,
    pub readiness: Vec<Readiness>,
    /// Part definitions with no decision of their own, in model order.
    pub unaccounted_parts: Vec<String>,
}

impl Audit {
    pub fn error_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count()
    }

    /// Whether the ledger has not been started at all.
    ///
    /// A model with values and no ledger produces the same error count as a model whose ledger
    /// contradicts it, so a bare count cannot tell a caller which situation it is in. It matters
    /// because the two need different actions: an empty ledger needs the first entry written, a
    /// contradictory one needs an entry fixed.
    ///
    /// The test is deliberately narrow. Every error must be one of the codes that mean "no
    /// record exists": an unaccounted value (`PROV001`), a part nothing decided to include
    /// (`PROV017`), or research that never said what it covered (`PROV018` — a ledger with no
    /// scope has not begun its own account any more than a ledger with no decisions has). A
    /// single error from any other code means a record exists and disagrees, and then the ledger
    /// *has* been started and must be read before it is extended.
    pub fn not_started(&self) -> bool {
        let started = self.accounted.values().any(|ok| *ok);
        let only_missing = self.issues.iter().all(|i| {
            i.severity != Severity::Error || matches!(i.code, "PROV001" | "PROV017" | "PROV018")
        });
        !started && only_missing
    }

    pub fn blocking(&self) -> usize {
        self.readiness
            .iter()
            .filter(|r| !r.open_questions.is_empty())
            .count()
    }

    pub fn unaccounted(&self) -> Vec<&String> {
        self.accounted
            .iter()
            .filter(|(_, ok)| !**ok)
            .map(|(name, _)| name)
            .collect()
    }
}

/// Walk the tree, calling `visit` with each element's qualified name.
///
/// The name follows one rule: an anonymous element is not a name, so it does not become a
/// segment of one. Joining every segment gave `P::A::<anonymous>` for an unnamed attribute, while
/// `sysml resolve` answers `P::A` for the same element — so a name copied out of `sysml prov`
/// could not be passed to `sysml impact`. Four functions here walked the tree by hand and each
/// had the same spelling.
fn walk_qualified(roots: &[Element], mut visit: impl FnMut(&Element, &str)) {
    fn go(el: &Element, path: &mut Vec<String>, visit: &mut impl FnMut(&Element, &str)) {
        let anonymous = el.name == "<anonymous>";
        if !anonymous {
            path.push(el.name.clone());
        }
        visit(el, &path.join("::"));
        for child in &el.children {
            go(child, path, visit);
        }
        if !anonymous {
            path.pop();
        }
    }
    for root in roots {
        go(root, &mut Vec::new(), &mut visit);
    }
}

/// Values worth accounting for: every attribute a model gives a literal or an expression.
///
/// A port or a part declares no value of its own, so it needs no row. Attributes without a
/// value clause are inputs whose value is supplied at run time; they are reported by the
/// semantics layer as `inconclusive`, not here.
pub fn value_bearing(roots: &[Element]) -> Vec<(String, String, u32)> {
    let mut out = Vec::new();
    walk_qualified(roots, |el, name| {
        if el.kind == Kind::AttributeUsage {
            if let Some(text) = &el.value_text {
                out.push((name.to_string(), text.clone(), el.line));
            }
        }
    });
    out
}

/// Every element in the model, as a qualified name.
///
/// Used to check that a ledger entry points at something real: `--from` and `--blocks` both
/// name elements, and a name that resolves to nothing makes a record look traceable when it is
/// not.
pub fn all_qualified(roots: &[Element]) -> Vec<String> {
    let mut out = Vec::new();
    walk_qualified(roots, |_, name| out.push(name.to_string()));
    out
}

/// Whether a ledger value and a model value say the same thing.
///
/// Compared after removing whitespace and wrapping parentheses, because the renderer and the
/// author spell the same expression differently (`20 [min]` vs `20[min]`, `(a and b)` vs
/// `a and b`). A normalisation that went further would start accepting different values, which
/// is the opposite of the point.
fn same_value(recorded: &str, written: &str) -> bool {
    fn norm(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for ch in s.chars() {
            if ch.is_whitespace() || ch == '(' || ch == ')' {
                continue;
            }
            out.push(ch);
        }
        out
    }
    norm(recorded) == norm(written)
}

/// Requirements in the model, as qualified names.
pub fn requirements(roots: &[Element]) -> Vec<String> {
    let mut out = Vec::new();
    walk_qualified(roots, |el, name| {
        if matches!(el.kind, Kind::RequirementDef | Kind::RequirementUsage) {
            out.push(name.to_string());
        }
    });
    out
}

/// Structural building blocks in the model, as qualified names.
///
/// A `part def` is where the model claims a kind of thing exists. That claim needs provenance
/// exactly as much as a number does: a value with no ledger entry is a guess wearing an equals
/// sign, and a part with no ledger entry is a decision nobody recorded making.
pub fn part_defs(roots: &[Element]) -> Vec<String> {
    let mut out = Vec::new();
    walk_qualified(roots, |el, name| {
        if el.kind == Kind::PartDef {
            out.push(name.to_string());
        }
    });
    out
}

/// What any ledger row has to satisfy, whatever it is about.
///
/// Shared by value decisions and part-existence decisions. It has to be shared: PROV017 first
/// accepted any row at all, so `decide --element Part --origin assumed --why "" --falsifier ""`
/// silenced it. A gate that a blank record satisfies is a gate that only checks whether someone
/// typed the element's name, which is not the thing anyone wanted to know.
fn check_origin(
    row: &Decision,
    element: &str,
    ledger: &Ledger,
    values: &[(String, String, u32)],
    issues: &mut Vec<LedgerIssue>,
    is_part: bool,
) {
    match row.origin {
        Origin::Spec => {
            if row.why.trim().is_empty() {
                issues.push(LedgerIssue {
                    code: "PROV002",
                    severity: Severity::Warning,
                    at: element.to_string(),
                    message: format!("{} is labelled `spec` without naming the source", row.id),
                });
            }
            // A specification value asserts something about the world. Without a source it
            // is a guess wearing a better label, and the whole point of the ledger is that
            // those two are distinguishable.
            if row.cites.is_empty() {
                issues.push(LedgerIssue {
                    code: "PROV012",
                    severity: Severity::Error,
                    at: element.to_string(),
                    message: format!(
                        "{} is labelled `spec` but cites nothing; register the material with \
                         `sysml source` and point at it with --cite",
                        row.id
                    ),
                });
            }
            for cite in &row.cites {
                match ledger.source(cite) {
                    None => issues.push(LedgerIssue {
                        code: "PROV013",
                        severity: Severity::Error,
                        at: element.to_string(),
                        message: format!(
                            "{} cites `{cite}`, which is not a registered source",
                            row.id
                        ),
                    }),
                    Some(src) if src.quote.trim().is_empty() => issues.push(LedgerIssue {
                        code: "PROV014",
                        severity: Severity::Error,
                        at: element.to_string(),
                        message: format!(
                            "{} cites `{cite}`, which carries no quoted passage",
                            row.id
                        ),
                    }),
                    Some(_) => {}
                }
            }
        }
        Origin::Derived => {
            if row.inputs.is_empty() {
                issues.push(LedgerIssue {
                    code: "PROV003",
                    severity: Severity::Error,
                    at: element.to_string(),
                    message: format!(
                        "{} is labelled `derived` but names no inputs; a derivation that \
                         cannot point at what it derived from is a guess with better manners",
                        row.id
                    ),
                });
            }
            // A part derived from a requirement is the strongest form this row takes: the thing
            // exists because something demands it. Any other element name is accepted, because
            // the alternative — resolving against the model tree here — duplicates what the CLI
            // already refuses at authoring time.
            for input in &row.inputs {
                let known = ledger.decisions.iter().any(|d| &d.element == input)
                    || values.iter().any(|(name, _, _)| name == input);
                // A derivation is only as good as its weakest input. If the thing this was
                // computed from is itself an assumption, the result is an assumption too,
                // wearing a label that exempts it from needing a falsifier.
                if let Some(upstream) = ledger
                    .decisions
                    .iter()
                    .find(|d| &d.element == input && d.origin == Origin::Assumed)
                {
                    issues.push(LedgerIssue {
                        code: "PROV015",
                        severity: Severity::Error,
                        at: element.to_string(),
                        message: format!(
                            "{} is labelled `derived` but its input `{input}` is an assumption ({})",
                            row.id, upstream.id
                        ),
                    });
                }
                if !known && !is_part {
                    issues.push(LedgerIssue {
                        code: "PROV004",
                        severity: Severity::Error,
                        at: element.to_string(),
                        message: format!(
                            "{} derives from `{input}`, which is neither a model value nor a \
                             ledger entry",
                            row.id
                        ),
                    });
                }
            }
        }
        Origin::Assumed => {
            if row.falsifier.trim().is_empty() {
                issues.push(LedgerIssue {
                    code: "PROV005",
                    severity: Severity::Error,
                    at: element.to_string(),
                    message: format!(
                        "{} is an assumption with no falsifier: nothing says what would show \
                         it wrong, so nobody can check it",
                        row.id
                    ),
                });
            }
            if row.approved_by.is_none() {
                issues.push(LedgerIssue {
                    // An error. An unapproved assumption is the exact failure this ledger
                    // exists to expose: the tool guessed, nobody noticed, and the number
                    // now reads as the user's own decision. Warning left that condition
                    // passing the gate.
                    code: "PROV006",
                    severity: Severity::Error,
                    at: element.to_string(),
                    message: format!(
                        "{} is an assumption no person has approved; it will read as a \
                         decision the user made",
                        row.id
                    ),
                });
            }
        }
        Origin::Asked => {
            let Some(qid) = &row.question else {
                issues.push(LedgerIssue {
                    code: "PROV007",
                    severity: Severity::Error,
                    at: element.to_string(),
                    message: format!(
                        "{} claims a person was asked, but names no question",
                        row.id
                    ),
                });
                return;
            };
            match ledger.question(qid) {
                None => issues.push(LedgerIssue {
                    code: "PROV008",
                    severity: Severity::Error,
                    at: element.to_string(),
                    message: format!(
                        "{} points at question `{qid}`, which is not on record",
                        row.id
                    ),
                }),
                Some(q) if q.state != QuestionState::Answered => issues.push(LedgerIssue {
                    code: "PROV009",
                    severity: Severity::Error,
                    at: element.to_string(),
                    message: format!(
                        "{} says a person answered `{qid}`, but that question is {:?}",
                        row.id, q.state
                    ),
                }),
                Some(_) => {}
            }
        }
    }
}

/// A fingerprint of the facts a verdict depends on: every element's name, kind and written
/// value, in tree order.
///
/// Not a cryptographic hash and not a commit id. A commit id changes when a comment does, which
/// would mark every judgement stale for an edit that cannot change any verdict; a cryptographic
/// hash would need a dependency to answer a question that only needs to be stable within one
/// file. This is the FNV-1a of the same strings the checker reads, so an edit to a number, a
/// name, or a typing moves it, and a comment does not.
pub fn fingerprint(roots: &[Element]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |text: &str| {
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // A separator, so `ab` + `c` cannot hash the same as `a` + `bc`.
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    fn walk(el: &Element, feed: &mut impl FnMut(&str)) {
        feed(&el.kind.keyword());
        feed(&el.name);
        if let Some(text) = &el.type_ref {
            feed(text);
        }
        if let Some(constraint) = &el.constraint {
            feed(constraint);
        }
        // The written value, not just the typing. A verdict depends on the numbers, and
        // `attribute e : Real = 5` and `= 1` differ only here — a fingerprint over the type
        // alone said the model had not moved when the one fact that decides the verdict had.
        if let Some(text) = &el.value_text {
            feed(text);
        }
        for child in &el.children {
            walk(child, feed);
        }
    }
    for root in roots {
        walk(root, &mut feed);
    }
    format!("{hash:016x}")
}

/// A judgement that no longer describes the model it was made against.
#[derive(Debug, Clone)]
pub struct StaleJudgement {
    pub id: String,
    pub requirement: String,
    pub verdict: String,
    /// The fingerprint the record carries, and the one the model has now.
    pub judged_against: String,
    pub current: String,
}

/// Every recorded judgement whose model has since moved.
pub fn stale(ledger: &Ledger, current: &str) -> Vec<StaleJudgement> {
    // A judgement is superseded by a later judgement of the same requirement against the
    // current model. Without that rule, re-judging never clears the warning — the old
    // record is still there, correctly, as audit history — and a warning that cannot be
    // cleared by doing the thing it asks for is one people learn to ignore.
    let superseded: std::collections::BTreeSet<&str> = ledger
        .judgements
        .iter()
        .filter(|j| j.fingerprint == current)
        .map(|j| j.requirement.as_str())
        .collect();
    ledger
        .judgements
        .iter()
        .filter(|j| j.fingerprint != current)
        .filter(|j| !superseded.contains(j.requirement.as_str()))
        .map(|j| StaleJudgement {
            id: j.id.clone(),
            requirement: j.requirement.clone(),
            verdict: j.verdict.clone(),
            judged_against: j.fingerprint.clone(),
            current: current.to_string(),
        })
        .collect()
}

/// Compare a model against its ledger.
pub fn audit(roots: &[Element], ledger: &Ledger) -> Audit {
    let mut issues = Vec::new();
    let mut accounted = BTreeMap::new();

    let values = value_bearing(roots);
    let mut by_element: BTreeMap<&str, Vec<&Decision>> = BTreeMap::new();
    for d in &ledger.decisions {
        by_element.entry(d.element.as_str()).or_default().push(d);
    }

    // A decision that names an element the model no longer has.
    //
    // Writing one is checked at the command line, but a rename happens afterwards: the record was
    // true when it was written and describes a version of the model nobody can open now. Worse than
    // useless — a ledger full of decisions *looks* like coverage, and the count of them is what a
    // reader uses to judge how much thought went in.
    //
    // Found by a real run: an agent renamed `ipRating` to `dockIpRating` and left three decisions
    // behind (D080–D082). `prov` reported nothing, and the model passed the gate.
    let known: std::collections::HashSet<String> = all_qualified(roots).into_iter().collect();
    for d in &ledger.decisions {
        if !known.contains(&d.element) {
            issues.push(LedgerIssue {
                code: "PROV021",
                severity: Severity::Error,
                at: d.element.clone(),
                message: format!(
                    "decision `{}` names an element this model does not contain: the record \
                     outlived a rename, or was written for something never modelled. Retract it \
                     (`sysml retract <model> --id {}`) or restore the element.",
                    d.id, d.id
                ),
            });
        }
    }

    for (element, text, line) in &values {
        let rows = by_element
            .get(element.as_str())
            .cloned()
            .unwrap_or_default();
        if rows.is_empty() {
            accounted.insert(element.clone(), false);
            issues.push(LedgerIssue {
                code: "PROV001",
                severity: Severity::Error,
                at: element.clone(),
                message: format!(
                    "value `{text}` has no ledger entry: nothing records whether it came from a \
                     specification, a derivation, or a guess (model line {line})"
                ),
            });
            continue;
        }
        accounted.insert(element.clone(), true);
        // The ledger says what the value is; the model is what runs. If they disagree, the
        // ledger describes a version of the model that no longer exists — and every claim
        // downstream of it (derivations, approvals) is about the old number.
        let row = rows.last().expect("non-empty");
        // A derived value records the number that came out; the model holds the expression that
        // produced it, so the two can never match as text. The comparison is meaningful only
        // for a value written literally.
        let literal = row.origin != Origin::Derived;
        if literal && !row.value.trim().is_empty() && !same_value(&row.value, text) {
            issues.push(LedgerIssue {
                code: "PROV016",
                severity: Severity::Error,
                at: element.clone(),
                message: format!(
                    "{} records `{}` but the model says `{}`; the ledger is about a value that is no longer there",
                    row.id, row.value, text
                ),
            });
        }
        // A later row supersedes an earlier one; the ledger is append-only in practice, and the
        // last statement about an element is the one that stands.
        let row = rows.last().expect("non-empty");
        check_origin(row, element, ledger, &values, &mut issues, false);
    }

    // Every `part def` is a claim that a kind of thing exists in this design. A number with no
    // ledger entry is a guess wearing an equals sign; a part with no ledger entry is a decision
    // nobody recorded making, and that is strictly worse because the part then grows a whole
    // tree of downstream values each of which *does* have a reason. The flight deck made this
    // concrete: 219 decisions, 17 of them about the seat mast's diameter, wall thickness and
    // stress, and not one of them about why there is a seat. `check` passed.
    //
    // The discriminator is that a value decision names the attribute (`Part::mass`), while a
    // decision about the part's existence names the part (`Part`). Requiring the second kind
    // costs one ledger row per part and cannot be satisfied by adding reasons to numbers.
    let mut unaccounted_parts = Vec::new();
    for part in part_defs(roots) {
        let rows: Vec<&Decision> = ledger
            .decisions
            .iter()
            .filter(|d| d.element == part)
            .collect();
        let Some(row) = rows.last() else {
            unaccounted_parts.push(part.clone());
            issues.push(LedgerIssue {
                code: "PROV017",
                severity: Severity::Error,
                at: part.clone(),
                message: "this part definition has no decision of its own: nothing records why \
                          the design contains this kind of thing, only what its numbers are"
                    .to_string(),
            });
            continue;
        };
        // The row has to meet the same bar a value's row does. Without this, PROV017 was
        // satisfied by `--origin assumed --why "" --falsifier ""`, which is a check on whether
        // the element's name was typed, not on whether anyone decided anything.
        check_origin(row, &part, ledger, &values, &mut issues, true);
    }

    // Questions that block requirements must be answered before those requirements can be
    // called ready — and a question nobody has closed is the visible form "the user has not
    // decided this yet".
    let mut readiness = Vec::new();
    for requirement in requirements(roots) {
        let open: Vec<String> = ledger
            .questions
            .iter()
            .filter(|q| {
                q.state == QuestionState::Open && q.blocks.iter().any(|b| b == &requirement)
            })
            .map(|q| q.id.clone())
            .collect();
        readiness.push(Readiness {
            requirement,
            open_questions: open,
        });
    }
    for r in &readiness {
        if !r.open_questions.is_empty() {
            issues.push(LedgerIssue {
                code: "PROV010",
                severity: Severity::Warning,
                at: r.requirement.clone(),
                message: format!(
                    "blocked by open question(s) {}: this requirement is not ready to be built \
                     against",
                    r.open_questions.join(", ")
                ),
            });
        }
    }

    // Duplicate ids make every reference ambiguous, so they are an error rather than a warning.
    let mut seen = BTreeSet::new();
    for q in &ledger.questions {
        if !seen.insert(q.id.clone()) {
            issues.push(LedgerIssue {
                code: "PROV011",
                severity: Severity::Error,
                at: q.id.clone(),
                message: "two questions share this id".to_string(),
            });
        }
    }

    // Which legal regimes the citations actually reach.
    //
    // A ledger records what was found and has no way to say where it looked, so a model citing
    // seven US and EU instruments is indistinguishable from one that searched the world. The
    // flight deck did exactly this and then reasoned about posture from two US products; a
    // standing personal aircraft was already in production in Hangzhou at the time.
    let cited_jurisdictions: BTreeSet<String> = ledger
        .decisions
        .iter()
        .flat_map(|d| d.cites.iter())
        .filter_map(|id| ledger.source(id))
        .map(|s| s.jurisdiction.trim().to_ascii_uppercase())
        .filter(|j| !j.is_empty())
        .collect();

    // Stating the scope is enough. Covering only the US is a fact about the work, not a defect;
    // what the gate refuses is the silence that makes one jurisdiction read as all of them.
    if ledger.jurisdictions.trim().is_empty() && cited_jurisdictions.len() <= 1 {
        let scope = if cited_jurisdictions.is_empty() {
            "no jurisdiction at all".to_string()
        } else {
            format!(
                "only {}",
                cited_jurisdictions
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        issues.push(LedgerIssue {
            code: "PROV018",
            severity: Severity::Error,
            at: "<ledger>".to_string(),
            message: format!(
                "the research cites {scope}, and the ledger does not say so; set the scope with \
                 `sysml scope <model> --jurisdictions ...` so a reader can tell a narrow search \
                 from an exhaustive one"
            ),
        });
    }

    // Regulations pick the load factors, so which regulation applies is decided by the market.
    // A model that cites jurisdictional material without naming a market has answered that
    // question silently — and every part sized from those load factors inherits the guess.
    if ledger.market.trim().is_empty() && !cited_jurisdictions.is_empty() {
        issues.push(LedgerIssue {
            code: "PROV019",
            severity: Severity::Error,
            at: "<ledger>".to_string(),
            message: format!(
                "load factors and safety requirements are cited from {} but the model does not \
                 say where the product is sold; set `sysml scope <model> --market ...`",
                cited_jurisdictions
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    // A judgement recorded against an earlier model is not a judgement of this one.
    //
    // The verdicts are recomputed on every run, so the *current* answer is always available —
    // but the recorded one is what a reader is told somebody checked. When the numbers move
    // under it, leaving it standing is the failure this whole ledger exists to prevent: a
    // passing grade from an earlier day, indistinguishable from a passing grade from today.
    //
    // Warning, not error, and deliberately so. The model may well still be fine, and a gate
    // that fails on "somebody should re-run the judge" teaches people to re-run the judge
    // without looking. What must not happen is silence: before this, a model edited after a
    // judgement reported `gate 0 error(s), 0 warning(s)`.
    let current = fingerprint(roots);
    for record in stale(ledger, &current) {
        issues.push(LedgerIssue {
            code: "PROV020",
            severity: Severity::Warning,
            at: record.requirement.clone(),
            message: format!(
                "{} judged `{}` against model {}; the model is now {}. The recorded verdict \
                 describes numbers that have since changed — re-run `sysml judge` to judge \
                 this model",
                record.id, record.verdict, record.judged_against, record.current
            ),
        });
    }

    Audit {
        issues,
        accounted,
        readiness,
        unaccounted_parts,
    }
}

/// One more than the highest number in a sequence of `PREFIXnnn` ids.
///
/// Non-numeric ids are ignored rather than rejected: a hand-edited ledger may carry a readable
/// id like `Q-temp`, and that should not stop the tool from issuing the next numbered one.
fn next_number<'a>(ids: impl Iterator<Item = &'a str>) -> usize {
    ids.filter_map(|id| {
        id.trim_start_matches(|c: char| c.is_ascii_alphabetic())
            .parse::<usize>()
            .ok()
    })
    .max()
    .unwrap_or(0)
        + 1
}

/// A one-screen summary for a person, which is the form this data is actually read in.
pub fn summarise(
    audit: &Audit,
    ledger: &Ledger,
    verdicts: &[sysml_semantics::RequirementVerdict],
) -> String {
    let mut out = String::new();
    let assumed = ledger
        .decisions
        .iter()
        .filter(|d| d.origin == Origin::Assumed)
        .count();
    let unapproved = ledger
        .decisions
        .iter()
        .filter(|d| d.origin == Origin::Assumed && d.approved_by.is_none())
        .count();
    let open = ledger
        .questions
        .iter()
        .filter(|q| q.state == QuestionState::Open)
        .count();
    let pass = verdicts
        .iter()
        .filter(|v| matches!(v.verdict, sysml_semantics::Verdict::Pass(_)))
        .count();
    let inconclusive = verdicts
        .iter()
        .filter(|v| matches!(v.verdict, sysml_semantics::Verdict::Inconclusive(_)))
        .count();
    let failed = verdicts
        .iter()
        .filter(|v| matches!(v.verdict, sysml_semantics::Verdict::Fail(_)))
        .count();
    let broken = verdicts
        .iter()
        .filter(|v| matches!(v.verdict, sysml_semantics::Verdict::Error(_)))
        .count();

    out.push_str(&format!(
        "values        {} accounted, {} without a ledger entry\n",
        audit.accounted.values().filter(|ok| **ok).count(),
        audit.unaccounted().len()
    ));
    out.push_str(&format!(
        "assumptions   {assumed} declared, {unapproved} never approved by a person\n"
    ));
    out.push_str(&format!(
        "questions     {open} open, {} answered, {} deferred\n",
        ledger
            .questions
            .iter()
            .filter(|q| q.state == QuestionState::Answered)
            .count(),
        ledger
            .questions
            .iter()
            .filter(|q| q.state == QuestionState::Deferred)
            .count()
    ));
    out.push_str(&format!(
        "requirements  {pass} pass, {inconclusive} undecided, {failed} fail, {broken} broken\n"
    ));
    out.push_str(&format!(
        "blocked       {} requirement(s) held by an open question\n",
        audit.blocking()
    ));
    out.push_str(&format!(
        "gate          {} error(s), {} warning(s)\n",
        audit.error_count(),
        audit
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count()
    ));
    out
}
