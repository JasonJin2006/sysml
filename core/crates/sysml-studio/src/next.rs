//! What the model is waiting for.
//!
//! The contract said how to record a decision; it never said what to do next, so an agent built
//! whatever came to mind — usually twenty part definitions and nothing that a requirement could be
//! checked against. The order below is the systems-engineering one: requirements, then the
//! questions that block them, then allocation, parameters, interfaces, verification.
//!
//! Every exit criterion is a fact about the artefact, computed by the same code that draws the
//! picture and audits the ledger. None of it is prose an agent could claim to have followed, which
//! is the point: a rule that can be checked must not live in a prompt.
//!
//! Advisory, never blocking. `check` refuses a model that is wrong; the order a model was built in
//! is not wrongness. A tool that blocked on it would teach its user to work around the tool, or to
//! mark a stage done.

use std::fmt::Write as _;

use sysml_ledger::{Audit, Ledger, QuestionState};
use sysml_model::{Element, Kind};
use sysml_semantics::{Report, Verdict};

use crate::translate::{plural, text, tr, Lang};

/// Read the model for everything the ladder asks about.
///
/// The unmet requirements come from the same traceability projection the picture is drawn from, so
/// `next` and the canvas cannot disagree about which requirement nothing satisfies.
pub fn gather(
    roots: &[Element],
    model: &std::path::Path,
    ledger: &Ledger,
    audit: &Audit,
    semantic: &Report,
) -> Facts {
    // Every element by qualified name, and by bare name so a name the projection prints bare can be
    // turned back into the one the canvas keys its boxes by.
    //
    // The projection carries bare names — the traceability picture labels a requirement with what
    // the model wrote, not with its path — and the canvas keys a box by the qualified name. A jump
    // from the stage strip to a box needs the second, so the two are matched here. Two elements
    // sharing a bare name would map to the first of them; that is a wrong jump, not a wrong verdict,
    // and the corpus does not do it.
    let mut concerns: Vec<String> = Vec::new();
    let mut qualified: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    fn walk(
        el: &Element,
        path: &mut Vec<String>,
        concerns: &mut Vec<String>,
        qualified: &mut std::collections::HashMap<String, String>,
    ) {
        // An anonymous element is not a name, so it is not a jump target and does not become a
        // segment of one. The resolver states the same rule; joining every segment made a
        // `qualified` entry of `P::A::<anonymous>` that nothing could ever look up.
        let anonymous = el.name == "<anonymous>";
        if !anonymous {
            path.push(el.name.clone());
        }
        let full = path.join("::");
        if !anonymous {
            qualified
                .entry(el.name.clone())
                .or_insert_with(|| full.clone());
        }
        if el.kind == Kind::ConcernDef {
            concerns.push(full);
        }
        for child in &el.children {
            walk(child, path, concerns, qualified);
        }
        if !anonymous {
            path.pop();
        }
    }
    for root in roots {
        let mut path = Vec::new();
        walk(root, &mut path, &mut concerns, &mut qualified);
    }
    let qualify = |name: &str| {
        qualified
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    };

    // Both counts come from the traceability projection rather than a walk of the tree: the ladder
    // and the picture then agree about what a requirement is and which one nothing satisfies.
    let graph = sysml_view::project(
        roots,
        sysml_view::View::Traceability,
        sysml_view::Mode::Extended,
    );
    let requirements = graph
        .nodes
        .iter()
        .filter(|n| n.stereotype.starts_with("requirement"))
        .count();
    let geometry = sysml_view::traceability_geometry(&graph);
    let anchored = requirements_with_subject(roots);
    let unmet: Vec<String> = geometry
        .unmet
        .iter()
        .filter_map(|&node| graph.nodes.get(node).map(|n| n.bare.clone()))
        // A requirement that names its subject is anchored to it: `requirement R1 { subject sys :
        // PatrolSystem; require constraint { sys.… } }` is a statement about the system, and the
        // system is the thing that carries it. The traceability view has no edge for a subject, so
        // every such requirement reads as unmet — which would send a modeller to write `satisfy`
        // links that say nothing. Only a requirement with neither a subject nor a satisfier floats.
        //
        // Concerns are excluded for the same reason: an unaddressed concern is reported by its own
        // criterion, and listing it here as a floating requirement would name the wrong gap.
        .filter(|name| !anchored.contains(name) && !concerns.iter().any(|c| c.ends_with(name)))
        .map(|name| qualify(&name))
        .collect();
    // The derivations the model states, from the one implementation the view also draws from.
    let connected: std::collections::HashSet<String> = sysml_view::derivations(roots)
        .into_iter()
        .flat_map(|(original, derived)| [original, derived])
        .collect();

    Facts {
        model: model.display().to_string(),
        requirements,
        concerns: concerns.clone(),
        uncovered_concerns: concerns
            .iter()
            .filter(|name| {
                let bare = name.rsplit("::").next().unwrap_or(name);
                !connected.contains(bare)
            })
            .cloned()
            .collect(),
        literal_only: named(&semantic.issues, "SEM007"),
        open_questions: ledger
            .questions
            .iter()
            .filter(|q| q.state == QuestionState::Open)
            .map(|q| OpenQuestion {
                id: q.id.clone(),
                blocks: q.blocks.clone(),
            })
            .collect(),
        unmet,
        unaccounted_values: audit
            .accounted
            .iter()
            .filter(|(_, accounted)| !**accounted)
            .map(|(name, _)| name.clone())
            .collect(),
        unapproved: audit
            .issues
            .iter()
            .filter(|i| i.code == "PROV006")
            .map(|i| i.at.clone())
            .collect(),
        broken_connections: named(&semantic.issues, "SEM001"),
        undecided: semantic
            .verdicts
            .iter()
            .filter(|v| matches!(v.verdict, Verdict::Inconclusive(_) | Verdict::Error(_)))
            .map(|v| v.qualified.clone())
            .collect(),
    }
}

/// The requirements that name a subject, by name.
fn requirements_with_subject(roots: &[Element]) -> std::collections::HashSet<String> {
    fn walk(el: &Element, out: &mut std::collections::HashSet<String>) {
        if matches!(el.kind, Kind::RequirementDef | Kind::RequirementUsage)
            && el.children.iter().any(|child| child.kind == Kind::Subject)
        {
            out.insert(el.name.clone());
        }
        for child in &el.children {
            walk(child, out);
        }
    }
    let mut out = std::collections::HashSet::new();
    for root in roots {
        walk(root, &mut out);
    }
    out
}

/// The elements an issue of `code` names.
fn named(issues: &[sysml_semantics::Issue], code: &str) -> Vec<String> {
    issues
        .iter()
        .filter(|i| i.code == code)
        .map(|i| i.at.clone())
        .collect()
}

/// The plan as JSON, for `sysml next --json` and for the canvas.
///
/// One shape, two callers. `stage` is a key rather than the printed label so a caller can branch on
/// it without matching translated text; the counts let an agent tell "one value missing" from "ninety".
pub fn to_json(plan: &Plan, facts: &Facts) -> serde_json::Value {
    // The elements that hold the model at this stage, so the canvas can offer to jump to them
    // instead of printing their names and leaving the reader to find them. Only element names:
    // a question id and a decision id are not boxes, and a chip that goes nowhere is worse than
    // no chip.
    let blocking: &[String] = match plan.stage {
        Stage::Requirements => &facts.literal_only,
        Stage::Questions => &[],
        Stage::Allocation => {
            if facts.uncovered_concerns.is_empty() {
                &facts.unmet
            } else {
                &facts.uncovered_concerns
            }
        }
        Stage::Values => &facts.unaccounted_values,
        Stage::Interfaces => &facts.broken_connections,
        Stage::Verification => &facts.undecided,
        Stage::Ready => &[],
    };
    // Every element that holds the model, with why — not just the five the strip shows. The
    // inspector asks "is *this* box in the way", and a list capped at five would answer "no" for
    // the twenty-sixth unaccounted value.
    let mut blocking_map: std::collections::BTreeMap<&str, &str> =
        std::collections::BTreeMap::new();
    for (list, reason) in [
        (&facts.literal_only, "literal-only"),
        (&facts.unmet, "unmet"),
        (&facts.unaccounted_values, "unaccounted"),
        (&facts.broken_connections, "broken"),
        (&facts.undecided, "undecided"),
        (&facts.uncovered_concerns, "uncovered-concern"),
    ] {
        for name in list {
            blocking_map.entry(name.as_str()).or_insert(reason);
        }
    }
    serde_json::json!({
        "stage": plan.stage.key(),
        "number": plan.stage.number(),
        "label": plan.stage.label(plan.lang),
        "established": plan.established,
        "waiting": plan.waiting,
        "command": plan.command,
        "targets": blocking.iter().take(5).cloned().collect::<Vec<String>>(),
        "blocking": blocking_map,
        "counts": {
            "requirements": facts.requirements,
            "openQuestions": facts.open_questions.len(),
            "unmetRequirements": facts.unmet.len(),
            "unaccountedValues": facts.unaccounted_values.len(),
            "unapprovedAssumptions": facts.unapproved.len(),
            "brokenConnections": facts.broken_connections.len(),
            "undecidedRequirements": facts.undecided.len(),
            "concerns": facts.concerns.len(),
            "uncoveredConcerns": facts.uncovered_concerns.len(),
        },
    })
}

/// The plan as a person reads it.
pub fn render(plan: &Plan) -> String {
    let lang = plan.lang;
    let mut out = String::new();
    let _ = writeln!(out, "{:<10}{}", text(lang, "stage"), plan.stage.label(lang));
    let _ = writeln!(out, "{:<10}{}", text(lang, "done"), plan.established);
    for (i, line) in plan.waiting.iter().enumerate() {
        let label = if i == 0 {
            text(lang, "waiting")
        } else {
            String::new()
        };
        let _ = writeln!(out, "{:<10}{}", label, line);
    }
    if let Some(command) = &plan.command {
        let _ = writeln!(out, "{:<10}{}", text(lang, "next"), command);
    }
    out
}

/// One unanswered question, as the ledger holds it.
pub struct OpenQuestion {
    pub id: String,
    /// Qualified names of the requirements it holds up.
    pub blocks: Vec<String>,
}

/// Everything the ladder needs, gathered by the caller.
///
/// Counts and names rather than elements: the ladder is about what is missing, and a name is what a
/// person acts on.
#[derive(Default)]
pub struct Facts {
    pub model: String,
    pub requirements: usize,
    /// Requirements whose constraint is only literals (`{1 > 0}`) — a placeholder, not a constraint.
    pub literal_only: Vec<String>,
    pub open_questions: Vec<OpenQuestion>,
    /// Requirements nothing satisfies.
    pub unmet: Vec<String>,
    /// Attributes with no ledger entry.
    pub unaccounted_values: Vec<String>,
    /// Decisions that are `assumed` and that no person has approved, by id.
    pub unapproved: Vec<String>,
    /// Connections with an end that does not exist.
    pub broken_connections: Vec<String>,
    /// Requirements whose constraint came out inconclusive or broken.
    pub undecided: Vec<String>,
    /// Concerns the model declares — what a stakeholder says they care about.
    pub concerns: Vec<String>,
    /// Concerns no requirement is derived from.
    pub uncovered_concerns: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Requirements,
    Questions,
    Allocation,
    Values,
    Interfaces,
    Verification,
    Ready,
}

impl Stage {
    /// The stage's number and name, in `lang`.
    pub fn label(self, lang: Lang) -> String {
        let english = match self {
            Stage::Requirements => "1/6 requirements",
            Stage::Questions => "2/6 questions",
            Stage::Allocation => "3/6 allocation",
            Stage::Values => "4/6 values",
            Stage::Interfaces => "5/6 interfaces",
            Stage::Verification => "6/6 verification",
            Stage::Ready => "complete",
        };
        text(lang, english)
    }

    /// Where this stage sits in the ladder, 1-based. `Ready` is 7 — one past the last gate.
    ///
    /// The canvas shows `3/6` rather than parsing it back out of the label, which is translated and
    /// would have to be matched as text.
    pub fn number(self) -> usize {
        match self {
            Stage::Requirements => 1,
            Stage::Questions => 2,
            Stage::Allocation => 3,
            Stage::Values => 4,
            Stage::Interfaces => 5,
            Stage::Verification => 6,
            Stage::Ready => 7,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Stage::Requirements => "requirements",
            Stage::Questions => "questions",
            Stage::Allocation => "allocation",
            Stage::Values => "values",
            Stage::Interfaces => "interfaces",
            Stage::Verification => "verification",
            Stage::Ready => "ready",
        }
    }
}

pub struct Plan {
    pub stage: Stage,
    /// The language `established`, `waiting` and `command` are written in.
    pub lang: Lang,
    /// What the model already has, in one line — so the answer does not read as "you have nothing".
    pub established: String,
    /// What holds the model at this stage.
    pub waiting: Vec<String>,
    /// The one command worth running, when there is one. A structural gap has no command: it is an
    /// edit to the model, and inventing a command for it would be pretending the tool can do it.
    pub command: Option<String>,
}

/// The earliest stage whose exit criterion the model does not meet, written in `lang`.
pub fn plan(facts: &Facts, lang: Lang) -> Plan {
    let model = facts.model.as_str();

    if facts.requirements == 0 {
        return Plan {
            stage: Stage::Requirements,
            lang,
            established: text(lang, "the model reads and checks clean"),
            waiting: vec![text(
                lang,
                "the model has no requirement: with no constraint, nothing downstream has anything to check against",
            )],
            command: None,
        };
    }
    if !facts.literal_only.is_empty() {
        return Plan {
            stage: Stage::Requirements,
            lang,
            established: tr(
                lang,
                plural(facts.requirements, "{} requirement", "{} requirements"),
                &[facts.requirements.to_string()],
            ),
            waiting: vec![tr(
                lang,
                "{} has a constraint made only of literals (`{1 > 0}`), so it constrains nothing",
                &[list(lang, &facts.literal_only)],
            )],
            command: None,
        };
    }
    if let Some(question) = facts.open_questions.first() {
        let open = facts.open_questions.len();
        let mut waiting = vec![tr(
            lang,
            plural(
                open,
                "{} open question nobody has answered; the first is {}",
                "{} open questions nobody has answered; the first is {}",
            ),
            &[open.to_string(), question.id.clone()],
        )];
        if !question.blocks.is_empty() {
            waiting.push(tr(lang, "it blocks: {}", &[list(lang, &question.blocks)]));
        }
        return Plan {
            stage: Stage::Questions,
            lang,
            established: tr(
                lang,
                plural(
                    facts.requirements,
                    "{} requirement, all constraints decidable",
                    "{} requirements, all constraints decidable",
                ),
                &[facts.requirements.to_string()],
            ),
            waiting,
            command: Some(tr(
                lang,
                "sysml answer {} --id {} --text \"...\" --by \"human:<name>\"",
                &[model.to_string(), question.id.clone()],
            )),
        };
    }
    if !facts.uncovered_concerns.is_empty() {
        return Plan {
            stage: Stage::Allocation,
            lang,
            established: tr(
                lang,
                plural(
                    facts.concerns.len(),
                    "{} concern, what a stakeholder cares about",
                    "{} concerns, what stakeholders care about",
                ),
                &[facts.concerns.len().to_string()],
            ),
            waiting: vec![
                tr(
                    lang,
                    "no requirement carries {}: the model names these concerns, but no requirement is derived from them",
                    &[list(lang, &facts.uncovered_concerns)],
                ),
                text(
                    lang,
                    "write the derivation with a library connection: `connection d : Derivation connect <concern> to <requirement>;`",
                ),
            ],
            command: None,
        };
    }
    if !facts.unmet.is_empty() {
        return Plan {
            stage: Stage::Allocation,
            lang,
            established: text(lang, "requirements complete, no open questions"),
            waiting: vec![tr(
                lang,
                "no subject or satisfy link reaches {}: the requirement hangs free, and no structure carries it",
                &[list(lang, &facts.unmet)],
            )],
            command: Some(tr(lang, "sysml render {} --view tv", &[model.to_string()])),
        };
    }
    if !facts.unaccounted_values.is_empty() {
        let first = facts.unaccounted_values[0].clone();
        let count = facts.unaccounted_values.len();
        return Plan {
            stage: Stage::Values,
            lang,
            established: text(lang, "every requirement has structure that carries it"),
            waiting: vec![tr(
                lang,
                plural(
                    count,
                    "{} value has no ledger entry (the first is {}); every number in the model needs a source",
                    "{} values have no ledger entry (the first is {}); every number in the model needs a source",
                ),
                &[count.to_string(), first.clone()],
            )],
            command: Some(tr(
                lang,
                "sysml decide {} --element \"{}\" --value \"...\" --origin spec|derived|assumed|asked --why \"...\"",
                &[model.to_string(), first],
            )),
        };
    }
    if let Some(id) = facts.unapproved.first() {
        let count = facts.unapproved.len();
        return Plan {
            stage: Stage::Values,
            lang,
            established: text(lang, "every value has a source record"),
            waiting: vec![tr(
                lang,
                plural(
                    count,
                    "{} assumption nobody has approved ({}): an assumption needs a person to accept the risk",
                    "{} assumptions nobody has approved ({}): an assumption needs a person to accept the risk",
                ),
                &[count.to_string(), list(lang, &facts.unapproved)],
            )],
            command: Some(tr(
                lang,
                "sysml approve {} --id {} --by \"human:<name>\"",
                &[model.to_string(), id.clone()],
            )),
        };
    }
    if !facts.broken_connections.is_empty() {
        let count = facts.broken_connections.len();
        return Plan {
            stage: Stage::Interfaces,
            lang,
            established: text(lang, "values and sources complete"),
            waiting: vec![tr(
                lang,
                plural(
                    count,
                    "{} connection has an end that does not exist: the interface names one end, and the other is missing",
                    "{} connections have an end that does not exist: the interface names one end, and the other is missing",
                ),
                &[count.to_string()],
            )],
            command: None,
        };
    }
    if !facts.undecided.is_empty() {
        return Plan {
            stage: Stage::Verification,
            lang,
            established: text(lang, "structure, values and interfaces complete"),
            waiting: vec![tr(
                lang,
                "no verdict yet for {}: a value or unit is missing, so the result is neither pass nor fail",
                &[list(lang, &facts.undecided)],
            )],
            command: Some(tr(lang, "sysml judge {}", &[model.to_string()])),
        };
    }
    Plan {
        stage: Stage::Ready,
        lang,
        established: text(lang, "all six gates pass"),
        waiting: Vec::new(),
        command: Some(tr(
            lang,
            "sysml review {} --out review.html",
            &[model.to_string()],
        )),
    }
}

/// Up to three names, then how many more: a list of ninety names is not something a person reads.
fn list(lang: Lang, names: &[String]) -> String {
    let shown = names.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    if names.len() > 3 {
        let more = tr(lang, " and {} more", &[(names.len() - 3).to_string()]);
        format!("{shown}{more}")
    } else {
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plan in English, the default. The Chinese cases call `super::plan` directly.
    fn plan(facts: &Facts) -> Plan {
        super::plan(facts, Lang::En)
    }

    fn facts() -> Facts {
        Facts {
            model: "m.sysml".to_string(),
            requirements: 3,
            ..Facts::default()
        }
    }

    #[test]
    fn a_model_without_requirements_is_at_the_first_stage() {
        let plan = plan(&Facts {
            model: "m.sysml".to_string(),
            ..Facts::default()
        });
        assert_eq!(plan.stage, Stage::Requirements);
        assert!(
            plan.command.is_none(),
            "there is no command that writes a requirement"
        );
    }

    #[test]
    fn a_placeholder_constraint_is_not_a_requirement() {
        let mut f = facts();
        f.literal_only = vec!["P::R1".to_string()];
        let plan = plan(&f);
        assert_eq!(plan.stage, Stage::Requirements);
        assert!(plan.waiting[0].contains("P::R1"), "{:?}", plan.waiting);
    }

    #[test]
    fn an_open_question_comes_before_everything_it_blocks() {
        // The contract tells the modeller to ask and then stop. A ladder that walked past an open
        // question would teach the opposite.
        let mut f = facts();
        f.open_questions = vec![OpenQuestion {
            id: "Q002".to_string(),
            blocks: vec!["P::CoverageReq".to_string()],
        }];
        f.unmet = vec!["P::CoverageReq".to_string()];
        let plan = plan(&f);
        assert_eq!(plan.stage, Stage::Questions);
        assert!(plan.command.unwrap().contains("--id Q002"));
    }

    #[test]
    fn an_unsatisfied_requirement_is_an_edit_not_a_command() {
        let mut f = facts();
        f.unmet = vec!["P::CoverageReq".to_string()];
        let plan = plan(&f);
        assert_eq!(plan.stage, Stage::Allocation);
        assert!(plan.waiting[0].contains("P::CoverageReq"));
        // The command draws the picture that shows the gap; it cannot close it.
        assert!(plan.command.unwrap().contains("--view tv"));
    }

    #[test]
    fn an_unaccounted_value_asks_for_a_decision_naming_that_value() {
        let mut f = facts();
        f.unaccounted_values = vec!["P::Drone::mass".to_string()];
        let plan = plan(&f);
        assert_eq!(plan.stage, Stage::Values);
        let command = plan.command.unwrap();
        assert!(command.contains("decide"));
        assert!(command.contains("P::Drone::mass"));
    }

    #[test]
    fn an_unapproved_assumption_asks_a_person_not_a_command() {
        let mut f = facts();
        f.unapproved = vec!["D003".to_string()];
        let plan = plan(&f);
        assert_eq!(plan.stage, Stage::Values);
        let command = plan.command.unwrap();
        assert!(command.contains("approve"));
        assert!(
            command.contains("human:"),
            "only a person can accept the risk"
        );
    }

    #[test]
    fn the_ladder_reports_the_earliest_gap_only() {
        // Everything is wrong at once; the answer is the first thing to fix, not a list of ten.
        let mut f = facts();
        f.unmet = vec!["P::R1".to_string()];
        f.unaccounted_values = vec!["P::x".to_string()];
        f.broken_connections = vec!["P::c".to_string()];
        f.undecided = vec!["P::R1".to_string()];
        assert_eq!(plan(&f).stage, Stage::Allocation);

        f.unmet.clear();
        assert_eq!(plan(&f).stage, Stage::Values);

        f.unaccounted_values.clear();
        assert_eq!(plan(&f).stage, Stage::Interfaces);

        f.broken_connections.clear();
        assert_eq!(plan(&f).stage, Stage::Verification);

        f.undecided.clear();
        assert_eq!(plan(&f).stage, Stage::Ready);
    }

    #[test]
    fn a_concern_no_requirement_derives_from_is_a_gap() {
        // The stakeholder half of the trace: a concern nobody wrote a requirement for is the
        // cheapest place for a model to be quietly incomplete, because nothing downstream fails.
        let mut f = facts();
        f.concerns = vec!["CoverageConcern".to_string()];
        f.uncovered_concerns = vec!["CoverageConcern".to_string()];
        let plan = plan(&f);
        assert_eq!(plan.stage, Stage::Allocation);
        assert!(
            plan.waiting[0].contains("CoverageConcern"),
            "{:?}",
            plan.waiting
        );
        // The next line teaches the library's own construct, because there is no command for it.
        assert!(plan.waiting[1].contains("Derivation"), "{:?}", plan.waiting);
        assert!(plan.command.is_none());
    }

    #[test]
    fn a_covered_concern_does_not_hold_the_model() {
        let mut f = facts();
        f.concerns = vec!["CoverageConcern".to_string()];
        f.uncovered_concerns = Vec::new();
        f.unmet = vec!["P::R1".to_string()];
        let plan = plan(&f);
        assert_eq!(plan.stage, Stage::Allocation);
        assert!(
            plan.waiting[0].contains("P::R1"),
            "the requirement gap should be reported, not the covered concern: {:?}",
            plan.waiting
        );
    }

    #[test]
    fn a_model_without_concerns_is_unaffected() {
        let mut f = facts();
        f.unmet = vec!["P::R1".to_string()];
        assert_eq!(plan(&f).stage, Stage::Allocation);
        assert_eq!(
            plan(&f).waiting.len(),
            1,
            "no concern line when there are no concerns"
        );
    }

    #[test]
    fn a_long_list_is_summarised() {
        let mut f = facts();
        f.unmet = (0..9).map(|i| format!("P::R{i}")).collect();
        let plan = plan(&f);
        assert!(plan.waiting[0].contains("and 6 more"), "{:?}", plan.waiting);
    }

    #[test]
    fn the_same_plan_reads_in_chinese_when_asked() {
        let mut f = facts();
        f.unmet = (0..9).map(|i| format!("P::R{i}")).collect();
        let zh = super::plan(&f, Lang::Zh);
        assert_eq!(zh.stage.label(Lang::Zh), "3/6 分配");
        assert!(zh.waiting[0].contains("既没有 subject"), "{:?}", zh.waiting);
        assert!(zh.waiting[0].contains("等另外 6 项"), "{:?}", zh.waiting);
    }

    #[test]
    fn english_is_the_default_language() {
        assert_eq!(Lang::default(), Lang::En);
        assert_eq!(Lang::parse("zh"), Some(Lang::Zh));
        assert_eq!(Lang::parse("fr"), None);
    }
}
