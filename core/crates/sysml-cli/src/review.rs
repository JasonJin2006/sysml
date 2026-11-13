//! The page a person opens to audit a model.
//!
//! It lives in the CLI rather than a view crate because it needs all three layers at once —
//! the semantic verdicts, the provenance ledger and the rendered diagram — and none of them
//! should start depending on each other for a presentation concern.
//!
//! The section order is the order a reviewer asks in: what is this, what still blocks it,
//! which numbers did nobody authorise, then the detail. Everything is inline; a review page
//! that needs a server is a review page that gets skipped.

use std::collections::BTreeMap;

use sysml_ledger::{Decision, Ledger, Origin, QuestionState};
use sysml_semantics::{Report, Severity, Verdict};

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn verdict_label(verdict: &Verdict) -> (&'static str, &'static str) {
    match verdict {
        Verdict::Pass(_) => ("pass", "ok"),
        Verdict::Fail(_) => ("fail", "bad"),
        Verdict::Inconclusive(_) => ("undecided", "warn"),
        Verdict::Error(_) => ("broken", "bad"),
    }
}

fn origin_label(origin: Origin) -> (&'static str, &'static str) {
    match origin {
        Origin::Spec => ("spec", "ok"),
        Origin::Derived => ("derived", "ok"),
        Origin::Assumed => ("assumed", "warn"),
        Origin::Asked => ("asked", "ok"),
    }
}

pub fn render(
    title: &str,
    report: &Report,
    ledger: &Ledger,
    audit: &sysml_ledger::Audit,
    values: &[(String, String, u32)],
    diagram: Option<&str>,
) -> String {
    let mut by_element: BTreeMap<&str, &Decision> = BTreeMap::new();
    for d in &ledger.decisions {
        by_element.insert(d.element.as_str(), d);
    }

    let open_questions: Vec<_> = ledger
        .questions
        .iter()
        .filter(|q| q.state == QuestionState::Open)
        .collect();
    let unapproved = ledger
        .decisions
        .iter()
        .filter(|d| d.origin == Origin::Assumed && d.approved_by.is_none())
        .count();
    let unaccounted = audit.unaccounted();
    let pass = report
        .verdicts
        .iter()
        .filter(|v| matches!(v.verdict, Verdict::Pass(_)))
        .count();
    let undecided = report
        .verdicts
        .iter()
        .filter(|v| matches!(v.verdict, Verdict::Inconclusive(_)))
        .count();
    let failed = report
        .verdicts
        .iter()
        .filter(|v| matches!(v.verdict, Verdict::Fail(_)))
        .count();
    let broken = report
        .verdicts
        .iter()
        .filter(|v| matches!(v.verdict, Verdict::Error(_)))
        .count();
    let errors = report
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count()
        + audit.error_count();

    let mut h = String::new();
    h.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
    h.push_str(&format!("<title>{}</title>", esc(title)));
    h.push_str(
        "<style>\
:root{--bg:#fbfbfa;--fg:#1c1c1a;--dim:#6b6b66;--line:#dededa;--ok:#2f6b4f;--warn:#8a6100;\
--bad:#a3342a;--chip:#f1f1ed}\
*{box-sizing:border-box}\
body{margin:0;padding:30px 38px 60px;background:var(--bg);color:var(--fg);\
font:14px/1.62 -apple-system,BlinkMacSystemFont,\"Segoe UI\",\"Noto Sans SC\",sans-serif}\
h1{font-size:19px;font-weight:600;margin:0 0 4px}\
h2{font-size:14px;font-weight:600;margin:34px 0 8px;padding-bottom:6px;border-bottom:1px solid var(--line)}\
.sub{color:var(--dim);font-size:13px;margin:0 0 22px}\
.tiles{display:flex;flex-wrap:wrap;gap:9px}\
.tile{background:var(--chip);border-radius:5px;padding:7px 13px;min-width:104px}\
.tile .n{font-size:18px;font-weight:600;font-variant-numeric:tabular-nums}\
.tile .k{font-size:12px;color:var(--dim)}\
.tile.bad .n{color:var(--bad)}.tile.warn .n{color:var(--warn)}.tile.ok .n{color:var(--ok)}\
table{width:100%;border-collapse:collapse;font-size:13px}\
th{text-align:left;font-weight:600;color:var(--dim);border-bottom:1px solid var(--line);\
padding:5px 9px 5px 0}\
td{vertical-align:top;border-bottom:1px solid var(--line);padding:6px 9px 6px 0}\
td.mono{font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-size:12px}\
.tag{display:inline-block;padding:0 6px;border-radius:3px;font-size:11px;background:var(--chip)}\
.tag.ok{color:var(--ok)}.tag.warn{color:var(--warn)}.tag.bad{color:var(--bad)}\
.empty{color:var(--dim);padding:8px 0}\
svg{max-width:100%;height:auto;border:1px solid var(--line);background:#fff}\
.foot{color:var(--dim);font-size:12px;margin-top:36px}\
</style></head><body>",
    );

    h.push_str(&format!("<h1>{}</h1>", esc(title)));
    h.push_str(&format!(
        "<p class=\"sub\">{} requirement(s) pass, {undecided} undecided, {} failing; {errors} error(s), \
{} open question(s), {unapproved} assumption(s) nobody approved.</p>",
        pass,
        failed + broken,
        open_questions.len()
    ));

    let tile = |n: usize, k: &str, cls: &str| {
        format!(
            "<div class=\"tile {cls}\"><div class=\"n\">{n}</div><div class=\"k\">{k}</div></div>"
        )
    };
    h.push_str("<div class=\"tiles\">");
    h.push_str(&tile(pass, "requirements pass", "ok"));
    h.push_str(&tile(undecided, "requirements undecided", "warn"));
    h.push_str(&tile(failed + broken, "requirements failing", "bad"));
    h.push_str(&tile(
        open_questions.len(),
        "open questions",
        if open_questions.is_empty() {
            "ok"
        } else {
            "warn"
        },
    ));
    h.push_str(&tile(
        unapproved,
        "unapproved assumptions",
        if unapproved == 0 { "ok" } else { "warn" },
    ));
    h.push_str(&tile(
        unaccounted.len(),
        "values unaccounted",
        if unaccounted.is_empty() { "ok" } else { "bad" },
    ));
    h.push_str("</div>");

    // 1. What holds the model back.
    h.push_str("<h2>Open questions</h2>");
    if open_questions.is_empty() {
        h.push_str("<p class=\"empty\">No open questions.</p>");
    } else {
        h.push_str("<table><tr><th>id</th><th>question</th><th>blocks</th><th>raised by</th></tr>");
        for q in &open_questions {
            h.push_str(&format!(
                "<tr><td class=\"mono\">{}</td><td>{}</td><td class=\"mono\">{}</td><td>{}</td></tr>",
                esc(&q.id),
                esc(&q.text),
                q.blocks
                    .iter()
                    .map(|b| esc(b))
                    .collect::<Vec<_>>()
                    .join("<br>"),
                esc(&q.asked_by)
            ));
        }
        h.push_str("</table>");
    }

    // 2. Requirements and their verdicts.
    h.push_str("<h2>Requirements</h2>");
    if report.verdicts.is_empty() {
        h.push_str("<p class=\"empty\">The model declares no requirements.</p>");
    } else {
        h.push_str(
            "<table><tr><th>requirement</th><th>verdict</th><th>basis</th><th>line</th></tr>",
        );
        for v in &report.verdicts {
            let (label, cls) = verdict_label(&v.verdict);
            let detail = match &v.verdict {
                Verdict::Pass(_) => "constraint holds".to_string(),
                Verdict::Fail(x) => format!("constraint fails, evaluates to {}", x.render()),
                Verdict::Inconclusive(why) => why.clone(),
                Verdict::Error(why) => why.clone(),
            };
            h.push_str(&format!(
                "<tr><td class=\"mono\">{}</td><td><span class=\"tag {cls}\">{label}</span></td>\
<td>{}</td><td class=\"mono\">{}</td></tr>",
                esc(&v.qualified),
                esc(&detail),
                v.line
            ));
        }
        h.push_str("</table>");
    }

    // 3. Every number and where it came from.
    h.push_str("<h2>Where the numbers come from</h2>");
    if ledger.decisions.is_empty() {
        h.push_str(
            "<p class=\"empty\">The ledger records no decisions. Every valued attribute in the model needs one.</p>",
        );
    } else {
        h.push_str(
            "<table><tr><th>element</th><th>value</th><th>origin</th><th>why</th>\
<th>what would prove it wrong</th><th>decided by</th><th>approved by</th></tr>",
        );
        for d in &ledger.decisions {
            let (label, cls) = origin_label(d.origin);
            let falsifier = if d.falsifier.trim().is_empty() {
                "—".to_string()
            } else {
                esc(&d.falsifier)
            };
            let approver = match &d.approved_by {
                Some(name) => format!("<span class=\"tag ok\">{}</span>", esc(name)),
                None if d.origin == Origin::Assumed => {
                    "<span class=\"tag warn\">nobody</span>".to_string()
                }
                None => "—".to_string(),
            };
            h.push_str(&format!(
                "<tr><td class=\"mono\">{}</td><td class=\"mono\">{}</td>\
<td><span class=\"tag {cls}\">{label}</span></td><td>{}</td><td>{}</td>\
<td class=\"mono\">{}</td><td>{}</td></tr>",
                esc(&d.element),
                esc(&d.value),
                esc(&d.why),
                falsifier,
                esc(&d.decided_by),
                approver
            ));
        }
        h.push_str("</table>");
    }

    // 4. Values in the model with no ledger row, with the value shown so the reader does not
    //    have to open the file to know what is at stake.
    if !unaccounted.is_empty() {
        h.push_str("<h2>Values with no source</h2>");
        h.push_str("<table><tr><th>element</th><th>value</th><th>line</th></tr>");
        let lookup: BTreeMap<&str, (&str, u32)> = values
            .iter()
            .map(|(name, text, line)| (name.as_str(), (text.as_str(), *line)))
            .collect();
        for name in &unaccounted {
            let (text, line) = lookup.get(name.as_str()).copied().unwrap_or(("—", 0));
            h.push_str(&format!(
                "<tr><td class=\"mono\">{}</td><td class=\"mono\">{}</td><td class=\"mono\">{}</td></tr>",
                esc(name),
                esc(text),
                line
            ));
        }
        h.push_str("</table>");
    }

    // 5. The checks in full, so a reader can disagree with the tool instead of trusting it.
    h.push_str("<h2>Findings</h2>");
    if report.issues.is_empty() && audit.issues.is_empty() {
        h.push_str("<p class=\"empty\">No findings.</p>");
    } else {
        h.push_str("<table><tr><th>location</th><th>level</th><th>detail</th></tr>");
        let mut rows: Vec<(String, &'static str, String)> = Vec::new();
        for issue in &report.issues {
            rows.push((
                format!("{}:{}", issue.at, issue.line),
                match issue.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                },
                format!("[{}] {}", issue.code, issue.message),
            ));
        }
        for issue in &audit.issues {
            rows.push((
                issue.at.clone(),
                match issue.severity {
                    sysml_ledger::Severity::Error => "error",
                    sysml_ledger::Severity::Warning => "warning",
                },
                format!("[{}] {}", issue.code, issue.message),
            ));
        }
        for (at, level, message) in rows {
            let cls = if level == "error" { "bad" } else { "warn" };
            h.push_str(&format!(
                "<tr><td class=\"mono\">{}</td><td><span class=\"tag {cls}\">{level}</span></td>\
<td>{}</td></tr>",
                esc(&at),
                esc(&message)
            ));
        }
        h.push_str("</table>");
    }

    if let Some(svg) = diagram {
        h.push_str("<h2>Structure</h2>");
        h.push_str(svg);
    }

    h.push_str(&format!(
        "<p class=\"foot\">Generated by sysml review. The ledger holds {} decision(s) and {} question(s). \
Every valued attribute in the model should have a row under \"Where the numbers come from\".</p>",
        ledger.decisions.len(),
        ledger.questions.len()
    ));
    h.push_str("</body></html>");
    h
}
