//! The canvas document: one model, one view, one arrangement, plus what is wrong with it.
//!
//! Everything the browser draws comes from here, and nothing here is invented — the geometry is
//! the renderer's, the diagnostics are the checker's, the values and their provenance are the
//! ledger's. The studio adds exactly one thing of its own: where a person put each box.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;
use sysml_model::{Element, Kind};
use sysml_view::graph::Graph;
use sysml_view::{Mode as ViewMode, View};

use crate::conflict;
use crate::edit::{self, Sources};
use crate::layout::{self, Layout};

/// One box, ready to draw.
#[derive(Debug, Clone, Serialize)]
pub struct CanvasNode {
    /// Stable identity across reparses: the qualified name. A box keeps its saved position
    /// through an unrelated edit because this does not shift the way an element index does.
    pub key: String,
    /// Index in the projection, which is what edges refer to.
    pub index: usize,
    pub stereotype: String,
    pub label: String,
    /// Definitions draw square-cornered, usages round-cornered.
    pub is_def: bool,
    /// True for a node drawn as a frame (a package, a state machine). Its rectangle is the
    /// frame, not a box, so the canvas draws it from `frames` and skips it here.
    pub is_frame: bool,
    /// True for a requirement that nothing states it satisfies, in the traceability view.
    ///
    /// This is the fact that view exists to show, so it travels with the node rather than being
    /// inferred in the browser from edge counts: the canvas would have to know which stereotype
    /// counts as a requirement and which edge direction counts as a satisfaction, and that is the
    /// projection's job, not the renderer's.
    pub unmet: bool,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Whether a person moved this box, so the canvas can offer "reset this one".
    pub placed: bool,
    /// Compartment rows: attributes, and a requirement's predicate in the general view.
    pub rows: Vec<String>,
    /// Border ports, positioned relative to the box.
    pub ports: Vec<CanvasPort>,
    pub parent: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CanvasPort {
    pub name: String,
    pub direction: Option<String>,
    /// `left` or `right`.
    pub side: String,
}

/// One package container: a rectangle with a title band, drawn behind its members.
#[derive(Debug, Clone, Serialize)]
pub struct CanvasFrame {
    /// Qualified name of the package, so a click can select it.
    pub key: String,
    pub stereotype: String,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Height of the title band, so the canvas draws the separator where the export does.
    pub title: f32,
}

/// One relationship, as a polyline the canvas can hit-test and draw symbols on.
#[derive(Debug, Clone, Serialize)]
pub struct CanvasEdge {
    pub index: usize,
    pub from: usize,
    pub to: usize,
    /// `composite` / `typing` / `connector` / `satisfy`.
    pub arrow: String,
    pub label: Option<String>,
    pub points: Vec<[f32; 2]>,
    /// Points where this edge arcs over another, computed by `sysml_view::jumps` so the canvas
    /// draws the same hops the SVG export does.
    pub hops: Vec<[f32; 2]>,
    /// Qualified names of both ends, so a click can open the right sheet.
    pub from_key: String,
    pub to_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CanvasIssue {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub at: String,
    pub line: u32,
}

/// One value-bearing feature with whatever the ledger says about it.
#[derive(Debug, Clone, Serialize)]
pub struct CanvasValue {
    pub key: String,
    pub text: String,
    pub line: u32,
    pub origin: Option<String>,
    pub why: Option<String>,
    pub falsifier: Option<String>,
    pub approved_by: Option<String>,
    pub cites: Vec<String>,
    pub decision_id: Option<String>,
    /// The unit as written (`Wh`), empty when the value states none.
    ///
    /// The unit lives only in the written text; the evaluated number is in base units and has lost
    /// the spelling the model used. Carrying it here is what lets the panel say "this is a
    /// length/time" instead of showing a number with no meaning.
    pub unit: String,
    /// The dimension when the unit table knows the unit (`mass`, `length/time`), else empty.
    pub dimension: String,
    /// What the value comes to, when it comes to anything: `Area(2.0, 3.0)` is 6.
    ///
    /// The panel showed the written text and the unit, so a parametric model read as a page of
    /// formulas with no answers. A value whose written text is already a number has nothing here —
    /// repeating `0.3` beside `0.3` is noise, and noise is what hides the one line that changed.
    pub computed: Option<String>,
    /// Whether a comparison involving this value can be decided at all.
    ///
    /// False for a unit outside the table. That is not the same as "no unit": a bare number
    /// compares with anything, and saying the two are alike is how a reader concludes a quantity
    /// was checked when nothing looked at it.
    pub unit_known: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CanvasQuestion {
    pub id: String,
    pub question: String,
    pub at: String,
    pub blocks: Vec<String>,
    pub state: String,
}

/// One requirement verdict, as the ledger records it.
///
/// A verdict is about a requirement *and the model state it read*, so the record carries the
/// fingerprint it was judged against. Sending it without `stale` would let the canvas draw a green
/// tick over a number that has since changed, which is the one claim this product must never make.
#[derive(Debug, Clone, Serialize)]
pub struct CanvasJudgement {
    /// Qualified name of the requirement.
    pub key: String,
    /// `pass`, `fail`, `inconclusive` or `error`.
    pub verdict: String,
    pub detail: String,
    /// Who judged it. `agent` unless a person did — the badge must not read as "the tool proved it".
    pub judged_by: String,
    pub at: String,
    /// The ledger fingerprint the record was computed against.
    pub judged_against: String,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub nodes: usize,
    pub edges: usize,
    pub errors: usize,
    pub warnings: usize,
    pub values_without_record: usize,
    pub assumptions_unapproved: usize,
    pub open_questions: usize,
    pub judgements: usize,
    pub judgements_stale: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CanvasDocument {
    pub title: String,
    pub files: Vec<String>,
    /// A digest of the source buffers this document was built from.
    ///
    /// The canvas polls for it and reloads when it moves, which is what makes an edit made in
    /// another editor show up without the person telling the canvas to refresh. It is a digest
    /// and not a timestamp because two writes inside one filesystem tick share an mtime, and a
    /// missed refresh is invisible while a spurious one just redraws.
    pub fingerprint: String,
    pub view: String,
    pub mode: String,
    pub width: f32,
    pub height: f32,
    pub nodes: Vec<CanvasNode>,
    /// Package frames, drawn behind the member boxes. Only the package view has any.
    ///
    /// Carried separately from `nodes` because a frame is not an element: it has a rectangle and
    /// a title, and the canvas draws it before the boxes so a member is never covered.
    pub frames: Vec<CanvasFrame>,
    pub edges: Vec<CanvasEdge>,
    pub issues: Vec<CanvasIssue>,
    pub values: Vec<CanvasValue>,
    pub questions: Vec<CanvasQuestion>,
    /// The ledger's fingerprint of the facts a verdict depends on.
    ///
    /// Deliberately separate from `fingerprint`, which digests the whole text: a comment moves
    /// that one and must not move this one, or every verdict would go stale because someone fixed
    /// a typo. The two are named apart so they cannot be used interchangeably.
    pub ledger_fingerprint: String,
    pub judgements: Vec<CanvasJudgement>,
    pub summary: Summary,
}

/// Everything a request needs: the paths, the buffers, and the arrangement.
pub struct Session {
    pub paths: Vec<PathBuf>,
    pub sources: Sources,
    pub layout_path: PathBuf,
    pub layout: Layout,
    pub view: View,
    pub mode: ViewMode,
    /// The view usage the picture is drawn for, when the model declares one.
    ///
    /// Set by the studio when a model-declared view is selected; its `expose` clauses then decide
    /// what is in the picture. `None` means "the projection over the whole model", which is what
    /// every built-in view button asks for.
    pub model_view: Option<String>,
    /// The model as it was before each edit, newest last.
    ///
    /// Every edit path goes through `commit`, so recording there makes an edit undoable without each
    /// route remembering to offer it. Bounded, because a stack that grows for the life of a session
    /// is a memory leak with a nicer name — and nobody takes back the fortieth edit.
    history: Vec<Sources>,
    /// What each undo took away, newest last, so it can be put back.
    ///
    /// Cleared by a new edit: after an undo, an edit takes the model down a different path, and the
    /// states the old path passed through are no longer something to redo.
    redo: Vec<Sources>,
}

impl Session {
    /// How many edits can be taken back.
    const HISTORY_LIMIT: usize = 32;

    pub fn open(paths: Vec<PathBuf>, view: View, mode: ViewMode) -> Result<Self, String> {
        if paths.is_empty() {
            return Err("no model file given".to_string());
        }
        let sources = Sources::read(&paths)?;
        let layout_path = layout::path_for(&paths[0]);
        let mut layout = Layout::load(&layout_path)?;
        layout.select(view_key(&view), mode_key(&mode));
        Ok(Self {
            paths,
            sources,
            layout_path,
            layout,
            view,
            mode,
            model_view: None,
            history: Vec::new(),
            redo: Vec::new(),
        })
    }

    /// Re-read the files, so an edit made in another editor shows up.
    pub fn reload(&mut self) -> Result<(), String> {
        self.reload_expecting(None)
    }

    /// Re-read the files, and report whether they still look like what the client last saw.
    ///
    /// The server re-reads before every edit, which is what keeps an edit computed against a
    /// fresh buffer. But that alone makes a conflict impossible to detect: by the time the edit
    /// runs, the buffer *is* the file. The stale thing is not the server's buffer, it is the
    /// **client's picture** — the person edited a box that the file no longer describes.
    ///
    /// So the caller passes the fingerprint it was looking at, and this compares it with what the
    /// file says now. A mismatch means the click was aimed at something that has moved, and the
    /// right answer is to refuse and reload rather than to apply an edit whose target the person
    /// did not actually see.
    ///
    /// `None` skips the check, for a caller that has no picture (a script, a test).
    pub fn reload_expecting(&mut self, seen: Option<&str>) -> Result<(), String> {
        self.sources = Sources::read(&self.paths)?;
        if let Some(seen) = seen {
            let now = fingerprint(&self.sources);
            if now != seen {
                return Err(format!(
                    "the model changed on disk since this page loaded it. Nothing was written. \
                     The canvas is reloading; the edit has to be made again on the current text."
                ));
            }
        }
        Ok(())
    }

    /// The fingerprint of the buffers as they stand.
    pub fn fingerprint(&self) -> String {
        fingerprint(&self.sources)
    }

    /// Point the layout at the picture this session is showing.
    ///
    /// Every placement request has to say which drawing it is about, and the answer lives on the
    /// same `Session` as the layout being written. Split into a method so the call site borrows
    /// `view`/`mode` for the duration of the call instead of holding a `&guard.session.view`
    /// across a `&mut guard.session.layout`.
    pub fn select_picture(&mut self) {
        let view = view_key(&self.view);
        let mode = mode_key(&self.mode);
        self.layout.select(view, mode);
    }

    /// Write an edit that `edit` already verified by reparsing.
    pub fn commit(&mut self, outcome: edit::Outcome) -> Result<Vec<String>, String> {
        // `commit` writes every buffer, which is what makes a multi-file edit atomic in the sense
        // that matters here: either both files are on disk in their new form, or the call failed
        // and neither was touched.
        edit::commit(&outcome)?;
        // Recorded after the write, not before: a commit that failed wrote nothing, and a history
        // holding a state the model never had would make one undo do nothing and the next one skip
        // a step.
        self.history.push(self.sources.clone());
        if self.history.len() > Self::HISTORY_LIMIT {
            self.history.remove(0);
        }
        // A new edit is a new branch: the states an earlier undo set aside are not on this path any
        // more, and offering them as a redo would write back a model the person had abandoned.
        self.redo.clear();
        self.sources = outcome.after;
        Ok(outcome.touched)
    }

    /// Write a snapshot over the model and make it the current one.
    ///
    /// The digests are re-read first: every write is checked against the digest each buffer had when
    /// it was read, and the buffer on disk is the one this session wrote — so a restore carrying the
    /// snapshot's old digests would be refused by the very check that protects the model. The
    /// snapshot's *text* is what gets written; the digest is what is there now.
    fn restore(&mut self, previous: Sources) -> Result<Vec<String>, String> {
        let mut restored = Sources::read(&previous.paths)?;
        restored.texts = previous.texts.clone();
        let touched = restored
            .paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        edit::commit(&edit::Outcome {
            before: self.sources.clone(),
            after: restored,
            touched: Vec::new(),
        })?;
        // Re-read after writing: the digests recorded above describe the text this restore
        // *replaced*, and a stale digest makes the next edit look like somebody else's conflict.
        // That is how the first version left a session refusing every edit after one undo.
        self.sources = Sources::read(&previous.paths)?;
        Ok(touched)
    }

    /// Take back the last edit. `None` when there is nothing to take back.
    pub fn undo(&mut self) -> Result<Option<Vec<String>>, String> {
        let Some(previous) = self.history.pop() else {
            return Ok(None);
        };
        // What is being taken away is what a redo would put back.
        self.redo.push(self.sources.clone());
        if self.redo.len() > Self::HISTORY_LIMIT {
            self.redo.remove(0);
        }
        self.restore(previous).map(Some)
    }

    /// Put back the last edit that was taken away. `None` when there is nothing to redo.
    pub fn redo(&mut self) -> Result<Option<Vec<String>>, String> {
        let Some(next) = self.redo.pop() else {
            return Ok(None);
        };
        self.history.push(self.sources.clone());
        if self.history.len() > Self::HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.restore(next).map(Some)
    }

    /// Re-evaluate the requirements and record what came out.
    ///
    /// The same evaluation `sysml judge` runs. It exists as a route because a stale verdict is
    /// actionable exactly where it is shown: asking a person to leave the canvas and run a CLI to
    /// clear a warning teaches them to ignore the warning.
    pub fn judge(&mut self, only: Option<&str>) -> Result<usize, String> {
        let loaded = self.sources.load()?;
        let semantic = sysml_semantics::check(&loaded.roots, &loaded.library);
        if semantic.verdicts.is_empty() {
            return Err("no requirement in this model has a predicate to judge".to_string());
        }
        let current = sysml_ledger::fingerprint(&loaded.roots);
        let path = sysml_ledger::Ledger::path_for(&self.paths[0]);
        let mut ledger = sysml_ledger::Ledger::load(&path)?;
        let mut written = 0;
        for verdict in &semantic.verdicts {
            if let Some(name) = only {
                if verdict.qualified != name {
                    continue;
                }
            }
            let detail = match &verdict.verdict {
                sysml_semantics::Verdict::Pass(_) => String::new(),
                sysml_semantics::Verdict::Fail(value) => format!("evaluates {}", value.render()),
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
                // The checker here is the studio's own; naming it keeps the record from reading
                // as "a person proved this" or "an outside agent said so".
                judged_by: "studio".to_string(),
                at: String::new(),
            });
            written += 1;
        }
        ledger.save(&path)?;
        Ok(written)
    }

    /// The views this model declares.
    pub fn views(&self) -> Result<Vec<ViewEntry>, String> {
        let loaded = self.sources.load()?;
        Ok(views(&loaded.roots))
    }

    /// The model's table of contents, for the navigator.
    pub fn outline(&self) -> Result<Vec<OutlineEntry>, String> {
        let loaded = self.sources.load()?;
        Ok(outline(&loaded.roots))
    }

    /// What the model is waiting for, as the canvas shows it.
    ///
    /// The same ladder `sysml next` prints: the canvas and the command line must not disagree about
    /// which stage a model is in, so this calls the one implementation rather than re-deriving it.
    pub fn next(&self, lang: crate::translate::Lang) -> Result<serde_json::Value, String> {
        let loaded = self.sources.load()?;
        let ledger = sysml_ledger::Ledger::load(&sysml_ledger::Ledger::path_for(&self.paths[0]))?;
        let audit = sysml_ledger::audit(&loaded.roots, &ledger);
        let semantic = sysml_semantics::check(&loaded.roots, &loaded.library);
        let facts = crate::next::gather(&loaded.roots, &self.paths[0], &ledger, &audit, &semantic);
        let plan = crate::next::plan(&facts, lang);
        Ok(crate::next::to_json(&plan, &facts))
    }

    /// The picture over the whole model.
    pub fn document(&self) -> Result<CanvasDocument, String> {
        self.document_with(None)
    }

    /// The picture, narrowed by an annotation filter when one is given.
    pub fn document_with(&self, filter: Option<&str>) -> Result<CanvasDocument, String> {
        let loaded = self.sources.load()?;
        // An annotation filter comes first, because its ids are the model's: `annotated_by` reads the
        // tree the checker reads. Narrowing to what a model-declared view exposes happens after, and
        // that one renumbers what it keeps.
        let narrowed = match filter.map(str::trim).filter(|text| !text.is_empty()) {
            Some(text) => {
                let parsed = sysml_syntax::project::parse_filter(text)?;
                let constraint = match &parsed.constraint {
                    Some(constraint) => Some(sysml_syntax::project::constraint_expr(constraint)?),
                    None => None,
                };
                let keep = sysml_semantics::annotated_by(
                    &loaded.roots,
                    &loaded.library,
                    &parsed.name,
                    constraint.as_ref(),
                );
                sysml_view::keep_only(&loaded.roots, &keep)
            }
            None => loaded.roots.clone(),
        };
        // A model-declared view draws what its `expose` clauses name; every other view draws the
        // whole model. The check below still runs over the whole tree: a diagnostic is about the
        // model, not about the picture on screen.
        let drawn = match &self.model_view {
            Some(key) => prune_to(&narrowed, &exposures_of(&loaded.roots, key)),
            None => narrowed,
        };
        let graph = sysml_view::project(&drawn, self.view, self.mode);

        // The layout is keyed by qualified name, so it has to be pruned against the names that
        // exist now; a placement for a deleted element is a position for nothing.
        let mut layout = self.layout.clone();
        layout.select(view_key(&self.view), mode_key(&self.mode));
        layout::prune(&mut layout, &edit::live_names(&loaded));

        // `drawn` rather than the loaded tree: pruning renumbers, so a node's id is an index into
        // what was drawn, and looking it up in the original tree answers with the wrong name.
        let keys = keys_by_index(&graph, &drawn);
        let geo = geometry(&graph, &self.view);
        let boxes = geo.boxes;
        let polylines = geo.polylines;
        let width = geo.width;
        let height = geo.height;
        // A package's rectangle is its frame. The node still travels so a click on the title can
        // select it and open its declaration, but the canvas draws it from `frames`.
        let frame_rects: BTreeMap<usize, (f32, f32, f32, f32)> = geo
            .frames
            .iter()
            .map(|(i, x, y, w, h)| (*i, (*x, *y, *w, *h)))
            .collect();

        // Which requirements nothing satisfies. Only the traceability view has the notion, so
        // every other view reports an empty set and the flag stays false.
        let unmet_nodes: std::collections::BTreeSet<usize> = match self.view {
            View::Traceability => {
                let traced = sysml_view::trace::traceability(&loaded.roots);
                traced
                    .traces
                    .iter()
                    .filter(|t| t.is_unmet())
                    .map(|t| t.node)
                    .collect()
            }
            _ => Default::default(),
        };

        let mut nodes = Vec::new();
        for (index, node) in graph.nodes.iter().enumerate() {
            let key = keys
                .get(&index)
                .cloned()
                .unwrap_or_else(|| format!("node{index}"));
            let (gx, gy, gw, gh) = boxes
                .get(&index)
                .or_else(|| frame_rects.get(&index))
                .copied()
                .unwrap_or_else(|| {
                    let (w, h) = sysml_view::node_size(node);
                    (0.0, 0.0, w, h)
                });
            let placement = layout.get(&key);
            nodes.push(CanvasNode {
                key,
                index,
                stereotype: node.stereotype.clone(),
                label: node.label().to_string(),
                is_def: node.is_def,
                is_frame: node.is_frame,
                unmet: unmet_nodes.contains(&index),
                x: placement.map(|p| p.x).unwrap_or(gx),
                y: placement.map(|p| p.y).unwrap_or(gy),
                w: placement.and_then(|p| p.w).unwrap_or(gw),
                h: placement.and_then(|p| p.h).unwrap_or(gh),
                placed: placement.is_some(),
                rows: node
                    .attributes
                    .iter()
                    .cloned()
                    .chain(node.constraint.iter().cloned())
                    .collect(),
                ports: node
                    .ports
                    .iter()
                    .map(|port| CanvasPort {
                        name: port.name.clone(),
                        direction: port.direction.map(|d| d.keyword().to_string()),
                        side: match port.side {
                            sysml_view::graph::Side::Left => "left".to_string(),
                            sysml_view::graph::Side::Right => "right".to_string(),
                        },
                    })
                    .collect(),
                parent: node.parent,
            });
        }

        // Frames are emitted in placement order, parents before children, which is the order a
        // painter needs: a nested package then covers part of its parent instead of vanishing
        // under it.
        let frames: Vec<CanvasFrame> = geo
            .frames
            .iter()
            .map(|(index, x, y, w, h)| {
                let node = &graph.nodes[*index];
                CanvasFrame {
                    key: keys
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| format!("node{index}")),
                    stereotype: node.stereotype.clone(),
                    label: node.label().to_string(),
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
                    title: sysml_view::packages::TITLE,
                }
            })
            .collect();

        let mut edges = Vec::new();
        // Hops are computed over every polyline at once, exactly as the SVG export does, so the
        // canvas and `render` put the arcs in the same places. See `sysml_view::jumps`.
        let all_points: Vec<Vec<(f32, f32)>> = graph
            .edges
            .iter()
            .enumerate()
            .filter_map(|(index, _)| polylines.get(&index).cloned())
            .collect();
        let jump_sets = sysml_view::jumps::hops(&all_points);
        let hops_by_index: BTreeMap<usize, Vec<(f32, f32)>> = graph
            .edges
            .iter()
            .enumerate()
            .filter(|(index, _)| polylines.contains_key(index))
            .map(|(index, _)| index)
            .zip(jump_sets)
            .collect();
        for (index, edge) in graph.edges.iter().enumerate() {
            let Some(points) = polylines.get(&index) else {
                continue;
            };
            edges.push(CanvasEdge {
                index,
                from: edge.from,
                to: edge.to,
                arrow: arrow_key(edge.arrow).to_string(),
                label: edge.label.clone(),
                points: points.iter().map(|(x, y)| [*x, *y]).collect(),
                hops: hops_by_index
                    .get(&index)
                    .map(|hops| hops.iter().map(|(x, y)| [*x, *y]).collect())
                    .unwrap_or_default(),
                from_key: keys.get(&edge.from).cloned().unwrap_or_default(),
                to_key: keys.get(&edge.to).cloned().unwrap_or_default(),
            });
        }

        // One checker for the check, the computed values and the units. The canvas rebuilds this
        // document on every load and every edit, and each of the three used to build a checker of
        // its own, which indexes the whole standard library.
        let (report, computed, units) =
            sysml_semantics::check_with_values(&loaded.roots, &loaded.library);
        let ledger = sysml_ledger::Ledger::load(&sysml_ledger::Ledger::path_for(&self.paths[0]))?;
        let audit = sysml_ledger::audit(&loaded.roots, &ledger);

        let mut issues: Vec<CanvasIssue> = report
            .issues
            .iter()
            .map(|issue| CanvasIssue {
                severity: match issue.severity {
                    sysml_semantics::Severity::Error => "error".to_string(),
                    sysml_semantics::Severity::Warning => "warning".to_string(),
                },
                code: issue.code.to_string(),
                message: issue.message.clone(),
                at: issue.at.clone(),
                line: issue.line,
            })
            .collect();
        issues.extend(audit.issues.iter().map(|issue| CanvasIssue {
            severity: "error".to_string(),
            code: "PROV".to_string(),
            message: issue.render(),
            at: String::new(),
            line: 0,
        }));

        let values = sysml_ledger::value_bearing(&loaded.roots)
            .into_iter()
            .map(|(key, text, line)| {
                let decision = ledger.decisions.iter().find(|d| d.element == key);
                let unit = units.get(&key);
                let written = text.split('[').next().unwrap_or(&text).trim();
                let computed = match computed.get(&key) {
                    Some(value) if written.parse::<f64>().is_err() => Some(value.render()),
                    _ => None,
                };
                CanvasValue {
                    key,
                    text,
                    line,
                    origin: decision.map(|d| d.origin.keyword().to_string()),
                    why: decision.map(|d| d.why.clone()),
                    falsifier: decision.map(|d| d.falsifier.clone()),
                    approved_by: decision.and_then(|d| d.approved_by.clone()),
                    cites: decision.map(|d| d.cites.clone()).unwrap_or_default(),
                    decision_id: decision.map(|d| d.id.clone()),
                    unit: unit.as_ref().map(|u| u.symbol.clone()).unwrap_or_default(),
                    dimension: unit
                        .as_ref()
                        .and_then(|u| u.dimension.clone())
                        .unwrap_or_default(),
                    unit_known: unit.as_ref().map(|u| u.known).unwrap_or(false),
                    computed,
                }
            })
            .collect();
        let questions = ledger
            .questions
            .iter()
            .map(|q| CanvasQuestion {
                id: q.id.clone(),
                question: q.text.clone(),
                at: q.at.clone(),
                blocks: q.blocks.clone(),
                state: match q.state {
                    sysml_ledger::QuestionState::Open => "open",
                    sysml_ledger::QuestionState::Answered => "answered",
                    sysml_ledger::QuestionState::Deferred => "deferred",
                }
                .to_string(),
            })
            .collect();

        // One row per requirement: the latest judgement wins, because re-judging a requirement is
        // how a person answers a stale warning. The stale flag comes from the ledger's own rule
        // rather than a local comparison, so the picture and `prov` cannot disagree about which
        // verdicts still describe the model.
        let ledger_fingerprint = sysml_ledger::fingerprint(&loaded.roots);
        let stale_ids: std::collections::BTreeSet<String> =
            sysml_ledger::stale(&ledger, &ledger_fingerprint)
                .into_iter()
                .map(|s| s.id)
                .collect();
        let mut latest: BTreeMap<&str, &sysml_ledger::Judgement> = BTreeMap::new();
        for record in &ledger.judgements {
            latest.insert(record.requirement.as_str(), record);
        }
        let judgements: Vec<CanvasJudgement> = latest
            .values()
            .map(|record| CanvasJudgement {
                key: record.requirement.clone(),
                verdict: record.verdict.clone(),
                detail: record.detail.clone(),
                judged_by: record.judged_by.clone(),
                at: record.at.clone(),
                judged_against: record.fingerprint.clone(),
                stale: stale_ids.contains(&record.id),
            })
            .collect();

        let summary = Summary {
            nodes: graph.nodes.len(),
            edges: graph.edges.len(),
            errors: issues.iter().filter(|i| i.severity == "error").count(),
            warnings: issues.iter().filter(|i| i.severity == "warning").count(),
            values_without_record: audit.unaccounted().len(),
            assumptions_unapproved: ledger
                .decisions
                .iter()
                .filter(|d| d.origin == sysml_ledger::Origin::Assumed && d.approved_by.is_none())
                .count(),
            open_questions: ledger
                .questions
                .iter()
                .filter(|q| q.state == sysml_ledger::QuestionState::Open)
                .count(),
            judgements: judgements.len(),
            judgements_stale: judgements.iter().filter(|j| j.stale).count(),
        };

        Ok(CanvasDocument {
            title: self
                .paths
                .first()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("model")
                .to_string(),
            files: self.paths.iter().map(|p| p.display().to_string()).collect(),
            fingerprint: fingerprint(&self.sources),
            view: view_key(&self.view).to_string(),
            mode: mode_key(&self.mode).to_string(),
            width,
            height,
            nodes,
            frames,
            edges,
            issues,
            values,
            questions,
            ledger_fingerprint,
            judgements,
            summary,
        })
    }
}

/// Box geometry, edge polylines, package frames and canvas size for one view.
struct Geometry {
    boxes: BTreeMap<usize, (f32, f32, f32, f32)>,
    polylines: BTreeMap<usize, Vec<(f32, f32)>>,
    /// `(node, x, y, w, h)` per package. Empty in every view but `pv`.
    frames: Vec<(usize, f32, f32, f32, f32)>,
    width: f32,
    height: f32,
}

fn geometry(graph: &Graph, view: &View) -> Geometry {
    match view {
        View::General => {
            let g = sysml_view::general_geometry(graph, sysml_view::layered::Style::Nested);
            Geometry {
                boxes: g
                    .boxes
                    .into_iter()
                    .map(|(i, x, y, w, h)| (i, (x, y, w, h)))
                    .collect(),
                polylines: g.edges.into_iter().collect(),
                frames: Vec::new(),
                width: g.width,
                height: g.height,
            }
        }
        View::Interconnection => {
            let g = sysml_view::interconnection_geometry(graph);
            Geometry {
                boxes: g
                    .boxes
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i, slot)| slot.map(|(x, y, w, h)| (i, (x, y, w, h))))
                    .collect(),
                polylines: g.connectors.into_iter().collect(),
                frames: Vec::new(),
                width: g.width,
                height: g.height,
            }
        }
        View::Packages => {
            // The frame rectangles are what makes this view a tree; the canvas draws them behind
            // the member boxes. Geometry is computed from the graph the caller already projected,
            // so the two never disagree about which node is inside which frame.
            let frames = sysml_view::packages::frames_of(graph);
            let g = sysml_view::packages::packages_geometry(graph, &frames);
            Geometry {
                boxes: g
                    .boxes
                    .into_iter()
                    .map(|(i, x, y, w, h)| (i, (x, y, w, h)))
                    .collect(),
                polylines: BTreeMap::new(),
                frames: g.frames,
                width: g.width,
                height: g.height,
            }
        }
        View::StateTransition | View::ActionFlow => {
            // The groups are rebuilt from the graph, exactly as the package frames are: the
            // canvas and the export both receive a graph, so one derivation lays out both.
            let groups = sysml_view::states::groups_of(graph);
            let g = sysml_view::states_geometry(graph, &groups);
            Geometry {
                boxes: g
                    .boxes
                    .into_iter()
                    .map(|(i, x, y, w, h)| (i, (x, y, w, h)))
                    .collect(),
                polylines: g.edges.into_iter().collect(),
                frames: g.frames,
                width: g.width,
                height: g.height,
            }
        }
        View::Sequence => {
            let g = sysml_view::sequence_geometry(graph);
            Geometry {
                boxes: g
                    .boxes
                    .into_iter()
                    .map(|(i, x, y, w, h)| (i, (x, y, w, h)))
                    .collect(),
                polylines: g.edges.into_iter().collect(),
                frames: Vec::new(),
                width: g.width,
                height: g.height,
            }
        }
        View::Traceability => {
            let g = sysml_view::traceability_geometry(graph);
            Geometry {
                boxes: g
                    .boxes
                    .into_iter()
                    .map(|(i, x, y, w, h)| (i, (x, y, w, h)))
                    .collect(),
                polylines: g.connectors.into_iter().collect(),
                frames: Vec::new(),
                width: g.width,
                height: g.height,
            }
        }
    }
}

/// One line of the model's table of contents, for the navigator panel.
#[derive(Debug, Clone, Serialize)]
pub struct OutlineEntry {
    /// Qualified name, the same key the document and the edit routes use.
    pub key: String,
    pub label: String,
    /// The declaration keyword (`package`, `part def`, `attribute`, …).
    pub kind: String,
    pub is_def: bool,
    /// The line the declaration starts on, so the panel can jump to it.
    pub line: u32,
    pub children: Vec<OutlineEntry>,
}

/// The model as a tree, in source order.
///
/// This is the source tree, not a projection. A navigator that showed only what the current view
/// draws would lose the packages in `gv` and every feature in `pv`, and the point of the panel is
/// that it lists the same model whatever picture is on screen.
///
/// What counts as a member is the same question the package view answers, so it gets the same
/// answer: relationship statements (`import`, `satisfy`, `connection`) are claims about elements,
/// not elements, and a tree that listed them would show `satisfy Requirements::Endurance by
/// vehicle` as a second entry named `Requirements::Endurance`. `Unsupported` is a front-end marker
/// rather than a declaration.
pub fn outline(roots: &[Element]) -> Vec<OutlineEntry> {
    fn walk(el: &Element, path: &mut Vec<String>, out: &mut Vec<OutlineEntry>) {
        if matches!(
            el.kind,
            Kind::Import | Kind::Satisfy | Kind::Connection | Kind::Unsupported
        ) {
            return;
        }
        let anonymous = el.name == "<anonymous>";
        // The label is what the panel shows, and a nameless element needs something visible
        // there. The key is what `locate` receives, so it has to be the name the document and
        // the edit routes use — which never includes an anonymous segment.
        let label = if el.name.is_empty() {
            el.kind.keyword().to_string()
        } else {
            el.name.clone()
        };
        if !anonymous {
            path.push(el.name.clone());
        }
        let key = path.join("::");
        let mut children = Vec::new();
        for child in &el.children {
            walk(child, path, &mut children);
        }
        if !anonymous {
            path.pop();
        }
        out.push(OutlineEntry {
            key,
            label,
            kind: el.kind.keyword_as(el.is_definition).to_string(),
            // Both must hold: the kind says the construct has definitions, and the declaration is
            // one. A `concern c : T` is a `ConcernDef` kind written as a usage.
            is_def: el.kind.is_definition() && el.is_definition,
            line: el.line,
            children,
        });
    }
    let mut out = Vec::new();
    for root in roots {
        walk(root, &mut Vec::new(), &mut out);
    }
    out
}

/// Qualified name for each projected node.
///
/// The projection drops packages and adds no placeholders, so a node's depth in the drawing says
/// nothing about its depth in the model. The name has to come from the element the node carries.
fn keys_by_index(graph: &Graph, roots: &[Element]) -> BTreeMap<usize, String> {
    fn walk(el: &Element, path: &mut Vec<String>, out: &mut BTreeMap<usize, String>) {
        // The same rule the resolver uses: an anonymous element is not a name, so it does not
        // become a segment of one. Joining every segment gave `P::A::<anonymous>`, which no
        // other crate knows, so a box the canvas offered to jump to was one `sysml impact` said
        // did not exist.
        let anonymous = el.name == "<anonymous>";
        if !anonymous {
            path.push(el.name.clone());
        }
        out.insert(el.id, path.join("::"));
        for child in &el.children {
            walk(child, path, out);
        }
        if !anonymous {
            path.pop();
        }
    }
    let mut by_id = BTreeMap::new();
    for root in roots {
        let mut path = Vec::new();
        walk(root, &mut path, &mut by_id);
    }
    let mut out = BTreeMap::new();
    for (index, node) in graph.nodes.iter().enumerate() {
        if let Some(id) = node.element {
            if let Some(name) = by_id.get(&id) {
                out.insert(index, name.clone());
            }
        }
    }
    out
}

/// One digest over every source buffer, so a poll can tell "nothing changed" from "reload".
///
/// Order matters: a rename that swaps two names between files has to produce a different digest
/// than the state before it, and hashing a set would call those equal.
fn fingerprint(sources: &edit::Sources) -> String {
    let mut acc = String::new();
    for (path, text) in sources.paths.iter().zip(&sources.texts) {
        acc.push_str(&path.display().to_string());
        acc.push('\u{1}');
        acc.push_str(text);
        acc.push('\u{2}');
    }
    format!("{:016x}", conflict::Digest::of(&acc).0)
}

fn arrow_key(arrow: sysml_view::Arrow) -> &'static str {
    match arrow {
        sysml_view::Arrow::Composite => "composite",
        sysml_view::Arrow::Typing => "typing",
        sysml_view::Arrow::Connector => "connector",
        sysml_view::Arrow::Satisfy => "satisfy",
        sysml_view::Arrow::Transition => "transition",
        sysml_view::Arrow::Annotate => "annotate",
    }
}

pub fn view_key(view: &View) -> &'static str {
    match view {
        View::General => "gv",
        View::Interconnection => "iv",
        View::Traceability => "tv",
        View::Packages => "pv",
        View::StateTransition => "stv",
        View::ActionFlow => "afv",
        View::Sequence => "sv",
    }
}

pub fn mode_key(mode: &ViewMode) -> &'static str {
    mode.keyword()
}

/// Which member kinds can be added inside an element of this kind.
///
/// Asked before the canvas offers the action, so a refusal explains itself rather than arriving
/// after a person has typed a name into a dialog.
pub fn addable_under(kind: &Kind) -> Vec<&'static str> {
    match kind {
        Kind::Package => vec!["part", "attribute", "requirement", "port"],
        Kind::PartDef | Kind::PartUsage | Kind::IndividualDef => vec!["part", "attribute", "port"],
        Kind::RequirementDef | Kind::RequirementUsage => vec!["attribute"],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canvas_keys_an_element_the_way_the_resolver_names_it() {
        // The canvas keys a box by its qualified name, and impact, diff and the ledger all name
        // elements the same way. Two spellings would mean a box the canvas offers to jump to is
        // one `sysml impact` says does not exist.
        //
        // The case that separates them is an anonymous element: the resolver folds it into its
        // owner (`P::A`), while joining every path segment gives `P::A::<anonymous>`.
        let loaded = sysml_syntax::load(
            "package P {\n\tmetadata def Safety;\n\tpart def A {\n\t\t@Safety;\n\t}\n}\n",
        )
        .expect("parses");
        let graph = sysml_view::project(
            &loaded.roots,
            sysml_view::View::General,
            sysml_view::Mode::Extended,
        );
        let keys = keys_by_index(&graph, &loaded.roots);
        assert!(
            !keys.values().any(|k| k.contains("<anonymous>")),
            "no key may carry the anonymous marker: {keys:?}"
        );
        let index = sysml_resolve::Index::build(&loaded.roots);
        for key in keys.values() {
            assert!(
                index.qualified.iter().any(|q| q == key),
                "`{key}` is a canvas key the resolver does not know"
            );
        }
    }

    #[test]
    fn every_view_has_a_stable_key() {
        // The key is what a layout file and a URL query both name the view by, so a change here
        // silently reassigns every saved arrangement.
        assert_eq!(view_key(&View::General), "gv");
        assert_eq!(view_key(&View::Interconnection), "iv");
        assert_eq!(view_key(&View::Traceability), "tv");
        assert_eq!(view_key(&View::Packages), "pv");
    }

    #[test]
    fn a_package_can_hold_parts_and_requirements() {
        let kinds = addable_under(&Kind::Package);
        assert!(kinds.contains(&"part"));
        assert!(kinds.contains(&"requirement"));
    }

    /// Re-judging clear the stale warning, which is the only reason to offer the button.
    ///
    /// A warning that cannot be cleared by doing the thing it asks for is one people learn to
    /// ignore, so this checks the whole loop: record, move the model, see stale, re-judge, see
    /// fresh again.
    #[test]
    fn re_judging_a_requirement_clears_the_stale_flag() {
        let dir = std::env::temp_dir().join(format!("sysml-rejudge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let model = dir.join("m.sysml");
        std::fs::write(
            &model,
            "package P {\n  part def V { attribute mass : Real = 1200 [kg]; }\n  part v : V;\n  \
             requirement def Light { subject s : V; require constraint { v.mass <= 1000 } }\n}\n",
        )
        .expect("write model");

        let mut session =
            Session::open(vec![model.clone()], View::General, ViewMode::Extended).expect("session");
        let written = session.judge(None).expect("judge");
        assert_eq!(written, 1, "one requirement with a predicate");
        let first = session.document().expect("document").judgements;
        assert_eq!(first.len(), 1);
        assert!(!first[0].stale);
        assert_eq!(first[0].judged_by, "studio");

        std::fs::write(
            &model,
            "package P {\n  part def V { attribute mass : Real = 900 [kg]; }\n  part v : V;\n  \
             requirement def Light { subject s : V; require constraint { v.mass <= 1000 } }\n}\n",
        )
        .expect("write model");
        session.reload().expect("reload");
        assert!(
            session.document().expect("document").judgements[0].stale,
            "the value the verdict read has moved"
        );

        assert_eq!(session.judge(None).expect("judge"), 1);
        let document = session.document().expect("document");
        let after = &document.judgements[0];
        // Which verdict comes out is the evaluator's business and its depth is tested elsewhere.
        // What this loop must guarantee is that the record is about the model as it is now.
        assert!(!after.stale, "re-judging answers the warning");
        assert_eq!(
            after.judged_against, document.ledger_fingerprint,
            "the new record is written against the current model, not the old one"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A verdict travels with the requirement and says which model state it is about.
    ///
    /// The failure this guards: a green tick drawn over a number that changed after it was
    /// judged. A verdict is only evidence while the facts it read still hold, so the record and
    /// the staleness flag are checked together.
    #[test]
    fn a_judgement_reaches_the_document_and_goes_stale_when_the_model_moves() {
        let dir = std::env::temp_dir().join(format!("sysml-verdict-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let model = dir.join("m.sysml");
        std::fs::write(
            &model,
            "package P {\n  part def V { attribute mass : Real = 1200 [kg]; }\n  \
             requirement def Light { subject v : V; require constraint { v.mass <= 1000 } }\n}\n",
        )
        .expect("write model");

        let mut session =
            Session::open(vec![model.clone()], View::Traceability, ViewMode::Extended)
                .expect("session");
        // Judge against the model as it stands, then move one number.
        let verdict = sysml_ledger::fingerprint(&session.sources.load().expect("load").roots);
        let mut ledger =
            sysml_ledger::Ledger::load(&sysml_ledger::Ledger::path_for(&model)).expect("ledger");
        ledger.judgements.push(sysml_ledger::Judgement {
            id: "J001".to_string(),
            requirement: "P::Light".to_string(),
            verdict: "fail".to_string(),
            detail: "1200 > 1000".to_string(),
            fingerprint: verdict,
            judged_by: "agent".to_string(),
            at: "2026-10-30".to_string(),
        });
        ledger
            .save(&sysml_ledger::Ledger::path_for(&model))
            .expect("save");

        let before = session.document().expect("document").judgements;
        assert_eq!(before.len(), 1, "one requirement, one verdict");
        assert_eq!(before[0].verdict, "fail");
        assert!(!before[0].stale, "judged against the model as it stands");

        std::fs::write(
            &model,
            "package P {\n  part def V { attribute mass : Real = 900 [kg]; }\n  \
             requirement def Light { subject v : V; require constraint { v.mass <= 1000 } }\n}\n",
        )
        .expect("write model");
        session.reload().expect("reload");
        let after = session.document().expect("document").judgements;
        assert_eq!(after.len(), 1);
        assert!(
            after[0].stale,
            "the number the verdict read has changed, so the verdict no longer describes the model"
        );
        assert_eq!(after[0].detail, "1200 > 1000", "the record still says why");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A value's unit reaches the canvas, with the three states kept apart.
    ///
    /// The unit lives only in the written text, so a panel that shows just the number says nothing
    /// about what kind of quantity it is. Three states travel: a unit the table knows (`mass`), no
    /// unit at all, and a unit outside the table — the last one is what keeps a reader from
    /// believing a comparison was checked when nothing looked at it.
    #[test]
    fn a_value_travels_with_its_unit() {
        // A counter, not just the process id: two tests in this file can share a name and
        // delete each other's model, which showed up as "no value mass" in a run where the
        // other test had already removed the directory.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("sysml-units-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let model = dir.join("m.sysml");
        std::fs::write(
            &model,
            "package P {\n  private import ScalarValues::*;\n  part def S {\n    \
             attribute mass : Real = 12 [kg];\n    attribute count : Real = 5;\n    \
             attribute odd : Real = 3 [furlong];\n  }\n}\n",
        )
        .expect("write model");

        let session =
            Session::open(vec![model.clone()], View::General, ViewMode::Extended).expect("session");
        let doc = session.document().expect("document");
        let by = |name: &str| {
            doc.values
                .iter()
                .find(|v| v.key.ends_with(name))
                .unwrap_or_else(|| panic!("no value {name}"))
        };
        assert_eq!(by("mass").unit, "kg");
        assert_eq!(by("mass").dimension, "mass");
        assert!(by("mass").unit_known);

        assert_eq!(by("count").unit, "", "no unit written");
        assert_eq!(by("count").dimension, "");
        assert!(!by("count").unit_known);

        assert_eq!(by("odd").unit, "furlong", "the authored spelling is kept");
        assert_eq!(by("odd").dimension, "");
        assert!(
            !by("odd").unit_known,
            "a unit outside the table is not dimensionless"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
    fn an_attribute_def_cannot_hold_a_part() {
        assert!(addable_under(&Kind::AttributeDef).is_empty());
    }

    /// The navigator lists the file's declarations, not the current view's boxes.
    ///
    /// A tree built from the projected graph would lose every package in `gv` and every feature in
    /// `pv`, so the same panel would show two different models depending on which picture is open.
    #[test]
    fn the_outline_keeps_packages_and_features() {
        let loaded = sysml_syntax::load(
            "package Top {
                 import ScalarValues::*;
                 package Sub { part def Engine { attribute p : Real = 1; } }
                 part def Car;
             }",
        )
        .expect("parses");
        let tree = outline(&loaded.roots);
        assert_eq!(tree.len(), 1, "one root package");
        let top = &tree[0];
        assert_eq!(top.key, "Top");
        assert_eq!(top.kind, "package");
        assert_eq!(top.line, 1, "the line the declaration starts on");
        let keys: Vec<&str> = top.children.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["Top::Sub", "Top::Car"],
            "the import is a claim about a name, not a member of the package"
        );
        let engine = top
            .children
            .iter()
            .find(|c| c.label == "Sub")
            .expect("Sub")
            .children
            .iter()
            .find(|c| c.label == "Engine")
            .expect("Engine");
        assert!(engine.is_def);
        assert_eq!(engine.key, "Top::Sub::Engine");
        assert_eq!(
            engine.children.first().map(|c| c.key.as_str()),
            Some("Top::Sub::Engine::p"),
            "a feature is in the tree even though no view but the general one draws it"
        );
    }
}

/// One view the model declares: a `view` usage and the projection its type maps to.
///
/// A model states which views it has (`view states : StateTransitionView;`). Until this existed the
/// studio offered a fixed list of projections and nothing in the model could add to it, so "which
/// views are there" was a property of the release rather than of the model.
#[derive(Debug, Clone, Serialize)]
pub struct ViewEntry {
    /// Qualified name of the view usage; stable enough to select by.
    pub key: String,
    /// The name a person reads.
    pub name: String,
    /// The type it names (`StateTransitionView`), or empty when it names none.
    pub target: String,
    /// The projection that type asks for (`stv`), or empty when nothing draws it.
    ///
    /// Empty is a real answer: a model may name a view kind this studio has no projection for, and
    /// saying so is better than guessing the nearest one.
    pub projection: String,
}

/// Which projection a standard view definition asks for.
///
/// The names are the ones the SysML v2 standard library defines (`StandardViewDefinitions.sysml`),
/// matched by the last segment of the type so a qualified name works too. A kind that is not here
/// has no projection: geometry is out of scope, and a grid or a browser is not a canvas drawing.
/// The projection a view's type asks for.
///
/// Only the five standard view types name one. A model's own view type does not: `view def
/// StructureView` says what the view is for, not how to draw it, so the answer is `None` and the
/// canvas keeps whatever projection it is already showing. A view type the specification lists
/// but this renderer does not draw (`GeometryView`) is also `None` — mapping it to the general
/// projection would claim a picture the notation does not have.
fn projection_for(view_name: &str) -> Option<&'static str> {
    match view_name
        .rsplit(['.', ':'])
        .next()
        .unwrap_or(view_name)
        .trim()
    {
        "GeneralView" => Some("gv"),
        "InterconnectionView" => Some("iv"),
        "StateTransitionView" => Some("stv"),
        "ActionFlowView" => Some("afv"),
        "SequenceView" => Some("sv"),
        _ => None,
    }
}

/// The views the model declares, in source order.
pub fn views(roots: &[Element]) -> Vec<ViewEntry> {
    fn walk(el: &Element, path: &mut Vec<String>, out: &mut Vec<ViewEntry>) {
        if el.kind == Kind::ViewUsage {
            path.push(if el.name.is_empty() {
                "view".to_string()
            } else {
                el.name.clone()
            });
            let target = el.type_ref.clone().unwrap_or_default();
            out.push(ViewEntry {
                key: path.join("::"),
                name: el.name.clone(),
                projection: projection_for(&target).unwrap_or("").to_string(),
                target,
            });
            path.pop();
            return;
        }
        if matches!(el.kind, Kind::Import | Kind::Unsupported) {
            return;
        }
        // The same rule the resolver uses: an anonymous element is not a name.
        let pushed = el.name != "<anonymous>";
        if pushed {
            path.push(el.name.clone());
        }
        for child in &el.children {
            walk(child, path, out);
        }
        if pushed {
            path.pop();
        }
    }
    let mut out = Vec::new();
    for root in roots {
        walk(root, &mut Vec::new(), &mut out);
    }
    out
}

/// A view usage is its own kind, and only a usage names a view a consumer can draw.
///
/// `view def V` states what a kind of view is for; `view v : V` is the one in the model. They
/// lowered to the same kind until a consumer needed to tell them apart, which is what this
/// checks from both ends: the outline says `view` for one and `view def` for the other, and
/// only the usage appears in the list the studio offers.
#[test]
fn a_view_usage_is_not_a_view_definition() {
    let loaded = sysml_syntax::load(
        "package P {
                 view def StructureView;
                 view structure : StructureView;
             }",
    )
    .expect("parses");
    let view = sysml_view::outline(&loaded.roots);
    assert!(
        view.contains("view def StructureView"),
        "the definition keeps its keyword:\n{view}"
    );
    assert!(
        view.contains("view structure : StructureView"),
        "so does the usage:\n{view}"
    );

    let declared = views(&loaded.roots);
    assert_eq!(declared.len(), 1, "only the usage is a view in the model");
    assert_eq!(declared[0].name, "structure");
    assert_eq!(declared[0].target, "StructureView");
    assert_eq!(declared[0].projection, "", "nothing draws a StructureView");
}

/// A standard view's type picks the projection; a name the studio cannot draw says so.
#[test]
fn a_standard_view_type_names_its_projection() {
    let loaded = sysml_syntax::load(
        "package P {
                 view states : StateTransitionView;
                 view flow : ActionFlowView;
                 view shape : GeometryView;
             }",
    )
    .expect("parses");
    let by_name: std::collections::BTreeMap<String, String> = views(&loaded.roots)
        .into_iter()
        .map(|v| (v.name, v.projection))
        .collect();
    assert_eq!(by_name["states"], "stv");
    assert_eq!(by_name["flow"], "afv");
    assert_eq!(
        by_name["shape"], "",
        "geometry is out of scope, not mapped to something else"
    );
}

/// The elements a view exposes, as written.
///
/// `expose Q;` names one element; `expose Q::*;` names everything inside Q. The `*` is kept as
/// written rather than expanded here, because expansion needs the tree and happens in `prune`.
pub fn exposures(el: &Element) -> Vec<String> {
    el.children
        .iter()
        .filter(|c| c.kind == Kind::Expose)
        .filter_map(|c| c.to.clone().or_else(|| c.type_ref.clone()))
        .collect()
}

/// The element tree cut down to what a view exposes.
///
/// A view's exposes are the model's own statement of what belongs in it. An element is kept when
/// it, or something inside it, is exposed — ancestors are kept because a member without its owner
/// is a tree with a hole in it — and everything else is dropped, so the projection draws the view
/// the model declared rather than the whole model with the view's name on the tab.
///
/// An empty expose list means the view states no restriction, and the whole tree is returned. That
/// is the "no view body" case, which is every view in the standard library: they carry a doc
/// comment and no exposes.
pub fn prune_to(roots: &[Element], exposed: &[String]) -> Vec<Element> {
    if exposed.is_empty() {
        return roots.to_vec();
    }
    let wants = |path: &str| {
        exposed.iter().any(|target| {
            let target = target.trim();
            if let Some(prefix) = target.strip_suffix("::*") {
                path.starts_with(prefix) && path.len() > prefix.len()
            } else if target == "*" {
                true
            } else {
                path == target
            }
        })
    };

    fn walk(el: &Element, path: &mut Vec<String>, wants: &dyn Fn(&str) -> bool) -> Option<Element> {
        let anonymous = el.name == "<anonymous>";
        if !anonymous {
            path.push(el.name.clone());
        }
        let qualified = path.join("::");
        let exposed = wants(&qualified);
        // An exposed element is kept whole: exposing `Power` in a state view asks for the machine,
        // and a machine whose states were filtered out is an empty box. Only elements the exposes
        // do not cover are dropped.
        let mut kept = Element {
            children: if exposed {
                el.children.clone()
            } else {
                Vec::new()
            },
            ..el.clone()
        };
        if !exposed {
            for child in &el.children {
                if let Some(child) = walk(child, path, wants) {
                    kept.children.push(child);
                }
            }
        }
        if !anonymous {
            path.pop();
        }
        // Keep this element when it is exposed, or when something inside it survived.
        if exposed || !kept.children.is_empty() {
            Some(kept)
        } else {
            None
        }
    }

    let mut out = Vec::new();
    for root in roots {
        if let Some(root) = walk(root, &mut Vec::new(), &wants) {
            out.push(root);
        }
    }
    // The same renumbering every pruning path needs: the views index by id, and a tree that dropped
    // elements carries ids that no longer line up with its own shape.
    sysml_view::renumber_ids(&mut out);
    out
}

/// The exposes written on the view usage with this qualified name.
///
/// An unknown key yields an empty list, which means "no restriction" — the same reading as a view
/// that states no exposes. That is deliberate: a selector naming a view that no longer exists
/// should fall back to the whole model, not to a blank picture that looks like an empty model.
pub fn exposures_of(roots: &[Element], key: &str) -> Vec<String> {
    fn walk(el: &Element, path: &mut Vec<String>, key: &str) -> Option<Vec<String>> {
        // The same rule the resolver uses: an anonymous element is not a name, so it is not a
        // segment of one. Counting it would put `P::<anonymous>::v` where the resolver says
        // `P::v`, and the key being looked up is the resolver's.
        let anonymous = el.name == "<anonymous>";
        if !anonymous {
            path.push(el.name.clone());
        }
        if el.kind == Kind::ViewUsage && path.join("::") == key {
            // `expose Power;` written inside `package P` means `P::Power`: a name resolves in the
            // scope the view is written in, not from the root. Without this the target never
            // matches a qualified path, and the view draws an empty picture.
            let scope = path[..path.len().saturating_sub(1)].join("::");
            let resolved = exposures(el)
                .into_iter()
                .map(|target| {
                    if let Some(absolute) = target.strip_prefix("::") {
                        absolute.to_string()
                    } else if scope.is_empty() {
                        target
                    } else if target.contains("::") {
                        // `expose Demo::power` is already qualified. Prefixing it with the scope
                        // gave `Demo::Demo::power`, which matched nothing and pruned the whole
                        // picture away. A `::` in the target means the author named it from a
                        // scope of their own choosing, and that is the one to use.
                        target
                    } else {
                        format!("{scope}::{target}")
                    }
                })
                .collect();
            return Some(resolved);
        }
        for child in &el.children {
            if let Some(found) = walk(child, path, key) {
                return Some(found);
            }
        }
        if !anonymous {
            path.pop();
        }
        None
    }
    for root in roots {
        if let Some(found) = walk(root, &mut Vec::new(), key) {
            return found;
        }
    }
    Vec::new()
}

/// A view draws what its exposes name, and nothing else.
///
/// `expose Power;` names one machine; `expose Q::*;` names everything inside Q. The pruning
/// keeps ancestors so a member is never drawn without its owner, and the projection then sees
/// a tree rather than a hole.
#[test]
fn a_view_draws_only_what_it_exposes() {
    let dir = std::env::temp_dir().join(format!("sysml-expose-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let model = dir.join("m.sysml");
    std::fs::write(
        &model,
        "package P {\n  view onlyPower : StateTransitionView { expose Power; }\n  \
             state def Power { state off; state on; transition off then on; }\n  \
             state def Engine { state idle; state running; transition idle then running; }\n}\n",
    )
    .expect("write model");

    let mut session = Session::open(
        vec![model.clone()],
        View::StateTransition,
        ViewMode::Extended,
    )
    .expect("session");

    let all = session.document().expect("document");
    assert_eq!(
        all.nodes.iter().filter(|n| n.is_frame).count(),
        2,
        "without a view selection both machines are drawn"
    );

    assert_eq!(
        exposures_of(&session.sources.load().expect("load").roots, "P::onlyPower"),
        vec!["P::Power".to_string()],
        "the exposed name is resolved in the scope the view is written in"
    );
    session.model_view = Some("P::onlyPower".to_string());
    let only = session.document().expect("document");
    let frames: Vec<&str> = only
        .nodes
        .iter()
        .filter(|n| n.is_frame)
        .map(|n| n.label.as_str())
        .collect();
    assert_eq!(
        frames,
        vec!["Power"],
        "Engine is not exposed and is not drawn"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The filter the CLI takes, applied to the picture the canvas draws.
#[test]
fn a_filter_narrows_the_picture_to_what_an_annotation_says() {
    let dir = std::env::temp_dir().join(format!("sysml-filter-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let model = dir.join("m.sysml");
    std::fs::write(
        &model,
        "package P {\n  metadata def Safety { attribute level : Integer; }\n  \
             @Safety { level = 3; }\n  part def Brake;\n  \
             @Safety { level = 1; }\n  part def Cup;\n  part def Bracket;\n}\n",
    )
    .expect("write model");

    let session =
        Session::open(vec![model.clone()], View::General, ViewMode::Extended).expect("session");

    let labels = |doc: &CanvasDocument| -> Vec<String> {
        doc.nodes.iter().map(|n| n.label.clone()).collect()
    };
    let all = labels(&session.document().expect("document"));
    assert!(all.contains(&"Bracket".to_string()), "{all:?}");

    let narrowed = labels(
        &session
            .document_with(Some("@Safety{level >= 2}"))
            .expect("filtered document"),
    );
    assert!(narrowed.contains(&"Brake".to_string()), "{narrowed:?}");
    assert!(
        !narrowed.contains(&"Cup".to_string()),
        "level 1 is filtered out: {narrowed:?}"
    );
    assert!(
        !narrowed.contains(&"Bracket".to_string()),
        "an element with no annotation is filtered out: {narrowed:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// `Q::*` exposes Q's members; the wildcard is not a literal element name.
#[test]
fn a_wildcard_expose_keeps_the_members() {
    let loaded = sysml_syntax::load(
        "package P {
                 package Q { part def A; part def B; }
                 part def C;
             }",
    )
    .expect("parses");
    let pruned = prune_to(&loaded.roots, &["P::Q::*".to_string()]);
    let outline = sysml_view::outline(&pruned);
    assert!(outline.contains("part def A"), "{outline}");
    assert!(outline.contains("part def B"), "{outline}");
    assert!(
        !outline.contains("part def C"),
        "C is outside what was exposed:\n{outline}"
    );
    assert!(
        outline.contains("package Q"),
        "the container is kept:\n{outline}"
    );
}

#[cfg(test)]
mod expose_qualified {
    use super::*;

    /// `expose Demo::power;` is already qualified, and prefixing it with the view's own scope
    /// gave `Demo::Demo::power`: the picture came out empty instead of showing `power`.
    #[test]
    fn a_qualified_expose_target_is_not_prefixed_with_the_scope_again() {
        let dir = std::env::temp_dir().join(format!("sysml-expose-q-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let model = dir.join("m.sysml");
        std::fs::write(
            &model,
            "package Demo {\n  part def Power;\n  part def Thermal;\n  part power : Power;\n  \
             part thermal : Thermal;\n  view def PowerView;\n  \
             view powerView : PowerView { expose Demo::power; }\n}\n",
        )
        .expect("write model");

        let session =
            Session::open(vec![model.clone()], View::General, ViewMode::Extended).expect("session");

        let roots = session.sources.load().expect("load").roots;
        assert_eq!(
            exposures_of(&roots, "Demo::powerView"),
            vec!["Demo::power".to_string()],
            "a qualified target is left as written"
        );

        let mut scoped = session;
        scoped.model_view = Some("Demo::powerView".to_string());
        let doc = scoped.document().expect("document");
        let labels: Vec<&str> = doc.nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| *l == "power"),
            "the exposed part is drawn: {labels:?}"
        );
        assert!(
            !labels.iter().any(|l| *l == "thermal"),
            "the part it does not expose is not: {labels:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
