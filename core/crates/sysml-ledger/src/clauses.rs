//! What a model says about a standard's clauses, against the clauses themselves.
//!
//! A requirement cites the clause it was written from in its `doc`, in the form the source uses:
//!
//! ```text
//! requirement UeRegistersWhenServiceNeeded {
//!     doc /* TS 23.501 5.3.2.2.2: in the RM-DEREGISTERED state the UE shall … */
//! ```
//!
//! This module answers the two questions a reviewer asks of such a model. Does every clause it
//! names exist? And which normative clauses of the standard does it not name at all? The second
//! is not a defect — a model covers the part of the standard it is about — so it is reported and
//! not judged. The first is a defect: a citation that names a clause the source does not have
//! reads exactly like one that does.

use std::collections::{BTreeMap, BTreeSet};

use sysml_model::{Element, Kind};

/// One entry of a clause ledger, as `research/clauses/build_clauses.py` writes it.
#[derive(Debug, Clone)]
pub struct Clause {
    pub id: String,
    pub title: String,
    /// `normative` when the clause states a requirement, `descriptive` otherwise.
    pub kind: String,
}

impl Clause {
    pub fn is_normative(&self) -> bool {
        self.kind == "normative"
    }
}

/// A standard's clauses, by id.
#[derive(Debug, Clone, Default)]
pub struct ClauseLedger {
    clauses: BTreeMap<String, Clause>,
}

impl ClauseLedger {
    pub fn get(&self, id: &str) -> Option<&Clause> {
        self.clauses.get(id)
    }

    pub fn len(&self) -> usize {
        self.clauses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }

    pub fn normative(&self) -> impl Iterator<Item = &Clause> {
        self.clauses.values().filter(|c| c.is_normative())
    }
}

/// Read a clause ledger: one JSON object per line.
///
/// # Errors
///
/// Returns the line number and the parser's message when a line is not a JSON object with an
/// `id`. A ledger that silently skipped a line would under-report coverage, which is the one
/// number the report exists to give.
pub fn read(jsonl: &str) -> Result<ClauseLedger, String> {
    let mut clauses = BTreeMap::new();
    for (number, line) in jsonl.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", number + 1))?;
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("line {}: no `id`", number + 1))?;
        let title = value
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let kind = value
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        clauses.insert(
            id.to_string(),
            Clause {
                id: id.to_string(),
                title,
                kind,
            },
        );
    }
    Ok(ClauseLedger { clauses })
}

/// One citation: what the model says, and where.
#[derive(Debug, Clone)]
pub struct Citation {
    pub requirement: String,
    pub clause: String,
    pub line: u32,
}

/// What [`trace`] found.
#[derive(Debug, Clone, Default)]
pub struct TraceReport {
    /// Clauses the model names that the ledger does not have.
    pub dangling: Vec<Citation>,
    /// Citations of clauses the ledger has but does not call normative.
    pub not_normative: Vec<Citation>,
    /// Normative clauses no requirement names, by id.
    pub uncovered: Vec<Clause>,
    /// Every citation the model makes, in model order.
    pub cited: Vec<Citation>,
    /// How many normative clauses the ledger holds.
    pub normative_total: usize,
}

impl TraceReport {
    pub fn to_json(&self) -> serde_json::Value {
        let cite = |c: &Citation| {
            serde_json::json!({
                "requirement": c.requirement,
                "clause": c.clause,
                "line": c.line,
            })
        };
        serde_json::json!({
            "cited": self.cited.iter().map(cite).collect::<Vec<_>>(),
            "dangling": self.dangling.iter().map(cite).collect::<Vec<_>>(),
            "notNormative": self.not_normative.iter().map(cite).collect::<Vec<_>>(),
            "uncovered": self.uncovered.iter().map(|c| serde_json::json!({
                "clause": c.id,
                "title": c.title,
            })).collect::<Vec<_>>(),
            "counts": {
                "normative": self.normative_total,
                "cited": self.cited.len(),
                "dangling": self.dangling.len(),
                "notNormative": self.not_normative.len(),
                "uncovered": self.uncovered.len(),
            },
        })
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "clauses      {} normative, {} cited\n",
            self.normative_total,
            self.cited.len()
        ));
        if !self.dangling.is_empty() {
            out.push_str(&format!("\ndangling ({}):\n", self.dangling.len()));
            for c in &self.dangling {
                out.push_str(&format!(
                    "  {}:{} cites `{}`, which the ledger does not have\n",
                    c.requirement, c.line, c.clause
                ));
            }
        }
        if !self.not_normative.is_empty() {
            out.push_str(&format!(
                "\nnot normative ({}):\n",
                self.not_normative.len()
            ));
            for c in &self.not_normative {
                out.push_str(&format!(
                    "  {}:{} cites `{}`, which states no requirement\n",
                    c.requirement, c.line, c.clause
                ));
            }
        }
        out.push_str(&format!("\nuncovered ({}):\n", self.uncovered.len()));
        for c in &self.uncovered {
            out.push_str(&format!("  {}  {}\n", c.id, c.title));
        }
        out
    }
}

/// Compare what a model cites against a clause ledger.
pub fn trace(roots: &[Element], ledger: &ClauseLedger) -> TraceReport {
    let mut cited = Vec::new();
    fn walk(
        el: &Element,
        path: &mut Vec<String>,
        ledger: &ClauseLedger,
        cited: &mut Vec<Citation>,
    ) {
        path.push(el.name.clone());
        let is_requirement = matches!(el.kind, Kind::RequirementDef | Kind::RequirementUsage);
        if is_requirement {
            if let Some(doc) = &el.doc {
                for clause in clause_ids(doc) {
                    cited.push(Citation {
                        requirement: path.join("::"),
                        clause,
                        line: el.line,
                    });
                }
            }
        }
        for child in &el.children {
            walk(child, path, ledger, cited);
        }
        path.pop();
    }
    for root in roots {
        walk(root, &mut Vec::new(), ledger, &mut cited);
    }

    let mut dangling = Vec::new();
    let mut not_normative = Vec::new();
    let mut covered: BTreeSet<&str> = BTreeSet::new();
    for c in &cited {
        match ledger.get(&c.clause) {
            None => dangling.push(c.clone()),
            Some(clause) => {
                covered.insert(clause.id.as_str());
                if !clause.is_normative() {
                    not_normative.push(c.clone());
                }
            }
        }
    }
    let uncovered: Vec<Clause> = ledger
        .normative()
        .filter(|c| !covered.contains(c.id.as_str()))
        .cloned()
        .collect();

    TraceReport {
        dangling,
        not_normative,
        uncovered,
        cited,
        normative_total: ledger.normative().count(),
    }
}

/// The clause ids a `doc` text names.
///
/// The source's own form is `TS 23.501 5.3.2.2.2`. A token counts as a clause id when it is
/// shaped like one — at least two dots (`5.3.2.2.2`, `A.1.2`) — and then it is looked up. Testing
/// membership instead of shape would make a dangling citation invisible: a number the ledger does
/// not have would read as prose, which is the one case the report exists to catch.
fn clause_ids(doc: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let tokens: Vec<&str> = doc.split_whitespace().collect();
    for (index, token) in tokens.iter().enumerate() {
        let candidate = token.trim_matches(|c: char| {
            c == ':' || c == ',' || c == ';' || c == '(' || c == ')' || c == '.'
        });
        if candidate.is_empty() {
            continue;
        }
        // `TS 23.501 5.3.2.2.2` names the standard and then the clause. `23.501` has the shape of
        // a clause number, so the word before it is checked: a `TS`, `TR` or `3GPP` prefix means
        // this number is the document's, not a clause of it.
        let after_standard = index > 0
            && matches!(
                tokens[index - 1].trim_end_matches(','),
                "TS" | "TR" | "3GPP"
            );
        if after_standard {
            continue;
        }
        // A clause id is digits with at least one dot (`4.1`, `5.3.2.2.2`), possibly starting with
        // an uppercase letter (`A.1.2`). One dot is enough: the ledger has clauses at every level,
        // and a model citing `4.1` is citing a clause. Requiring two dots silently dropped every
        // top-level citation.
        let numbered = candidate
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c.is_ascii_uppercase())
            && candidate.contains('.')
            && candidate.chars().any(|c| c.is_ascii_digit())
            && !candidate.starts_with('.')
            && !candidate.ends_with('.');
        // A trailing dot after a clause number (`5.3.2.` at the end of a sentence) is trimmed
        // above; the ledger's own ids never end in one.
        if numbered && seen.insert(candidate.to_string()) {
            out.push(candidate.to_string());
        }
    }
    out
}
