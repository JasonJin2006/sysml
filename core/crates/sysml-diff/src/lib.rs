//! What changed between two versions of a model, and what a change reaches.
//!
//! This is the one thing a text model can answer that a drawing cannot: a `.sysml` file is a
//! diffable artifact, and once two versions are compared by name rather than by line, "what did
//! this change affect" becomes a query instead of a review meeting.
//!
//! Two operations, deliberately separate:
//!
//! - [`delta`] compares two models element by element, keyed by qualified name. A renamed element
//!   is a removal and an addition, which is honest: the name *is* the identity, and a caller that
//!   wants to follow a rename needs a relation the model does not carry.
//! - [`impact`] walks backwards from one element through every written reference that lands on
//!   it. The walk follows resolved targets, not text, so a requirement naming `d.endurance`
//!   reports on the attribute even when the spelling differs.
//!
//! Neither answers "is this change safe". That is a judgement, and it is left where judgements
//! belong — in the ledger, recorded by a person.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use sysml_model::{Element, Kind};
use sysml_resolve::{Index, Resolution};

/// One element as the comparison sees it: the facts that decide whether it changed.
///
/// Type text rather than resolved ids, because the two models are separate id spaces and a
/// resolved id from one means nothing in the other. Comparing the written clause is what makes
/// the answer stable across an edit earlier in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub qualified: String,
    pub kind: String,
    pub type_ref: Option<String>,
    pub supertypes: Vec<String>,
    pub redefines: Vec<String>,
    /// The value written after `=`, verbatim.
    ///
    /// Without this, `= 80 [mm]` and `= 85 [mm]` compared equal and `diff` answered "no change"
    /// for the one edit a change-propagation study cares about most. Every other field here
    /// describes an element's shape; this is the only one that carries what it says.
    pub value: Option<String>,
    /// The `require constraint { … }` a requirement states, rendered.
    ///
    /// A requirement's meaning is its predicate, and without it `<= 20` to `<= 25` compared
    /// equal: the requirement's own shape did not move, only the claim it makes. Rendered rather
    /// than compared as a tree because the two models are separate allocations; the text is what
    /// a reader would compare too.
    pub predicate: Option<String>,
    pub children: usize,
}

impl Snapshot {
    fn of(el: &Element, index: &Index) -> Self {
        Self {
            qualified: index
                .qualified
                .get(el.id)
                .cloned()
                .unwrap_or_else(|| el.name.clone()),
            kind: el.kind.keyword().to_string(),
            type_ref: el.type_ref.clone(),
            supertypes: el.supertypes.clone(),
            redefines: el.redefines.clone(),
            value: el.value_text.clone(),
            predicate: el.predicate.as_ref().map(|p| p.render()),
            children: el.children.len(),
        }
    }

    /// The fields that differ, for a caller that wants to say what changed rather than that it did.
    fn changes_from(&self, before: &Snapshot) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.kind != before.kind {
            out.push("kind");
        }
        if self.type_ref != before.type_ref {
            out.push("type");
        }
        if self.supertypes != before.supertypes {
            out.push("supertypes");
        }
        if self.redefines != before.redefines {
            out.push("redefines");
        }
        if self.value != before.value {
            out.push("value");
        }
        if self.predicate != before.predicate {
            out.push("predicate");
        }
        if self.children != before.children {
            out.push("children");
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct Changed {
    pub qualified: String,
    pub kind: String,
    /// Which facts moved: `type`, `supertypes`, `children`, and so on.
    pub fields: Vec<&'static str>,
    pub before: Snapshot,
    pub after: Snapshot,
}

#[derive(Debug, Clone, Default)]
pub struct Delta {
    pub added: Vec<Snapshot>,
    pub removed: Vec<Snapshot>,
    pub changed: Vec<Changed>,
}

impl Delta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Every qualified name the change touched, in one set.
    pub fn touched(&self) -> Vec<String> {
        let mut out: BTreeSet<&str> = BTreeSet::new();
        for s in &self.added {
            out.insert(&s.qualified);
        }
        for s in &self.removed {
            out.insert(&s.qualified);
        }
        for c in &self.changed {
            out.insert(&c.qualified);
        }
        out.into_iter().map(str::to_string).collect()
    }
}

/// Every element of a model, keyed by qualified name, project elements only.
fn snapshot(roots: &[Element], index: &Index) -> BTreeMap<String, Snapshot> {
    let mut out = BTreeMap::new();
    let library_start = index.len() - index.library_len();
    for root in roots {
        fn walk(el: &Element, index: &Index, end: usize, out: &mut BTreeMap<String, Snapshot>) {
            if el.id >= end {
                return;
            }
            // Imports are not part of what the model declares; they describe how it sees
            // something else. Their targets are compared through the elements themselves.
            if el.kind != Kind::Import {
                let snap = Snapshot::of(el, index);
                out.insert(snap.qualified.clone(), snap);
            }
            for child in &el.children {
                walk(child, index, end, out);
            }
        }
        walk(root, index, library_start, &mut out);
    }
    out
}

/// Compare two models by qualified name.
pub fn delta(before: &[Element], after: &[Element]) -> Delta {
    let before_index = Index::build(before);
    let after_index = Index::build(after);
    delta_with(before, &before_index, after, &after_index)
}

/// The same comparison, when the caller already built the indexes.
///
/// Split out because a caller comparing a model against itself after an edit does not want to
/// pay for two builds it already did.
pub fn delta_with(
    before: &[Element],
    before_index: &Index,
    after: &[Element],
    after_index: &Index,
) -> Delta {
    let old = snapshot(before, before_index);
    let new = snapshot(after, after_index);
    let mut out = Delta::default();
    for (name, snapshot) in &new {
        match old.get(name) {
            None => out.added.push(snapshot.clone()),
            Some(previous) if previous != snapshot => out.changed.push(Changed {
                qualified: name.clone(),
                kind: snapshot.kind.clone(),
                fields: snapshot.changes_from(previous),
                before: previous.clone(),
                after: snapshot.clone(),
            }),
            Some(_) => {}
        }
    }
    for (name, snapshot) in &old {
        if !new.contains_key(name) {
            out.removed.push(snapshot.clone());
        }
    }
    out
}

/// One hop of a backward walk: who names the element, and how.
#[derive(Debug, Clone)]
pub struct Dependant {
    pub qualified: String,
    pub kind: String,
    /// The written clause that names it, so a reader can go straight to the line.
    pub because: String,
    /// How many hops from the starting element. One means it names it directly.
    pub distance: usize,
}

/// Everything that depends on `qualified`, nearest first.
///
/// The walk goes backwards through resolved references: an element depends on what its type
/// clause, its specializations and its requirement predicate names. It is breadth-first, so a
/// caller that only cares about the direct dependencies can stop at `distance == 1`, and each
/// element appears once however many ways it is reached.
///
/// `qualified` itself is never in the result, even when the model is circular: an element is
/// not its own dependency.
///
/// # Errors
///
/// Returns a message naming what does exist when no element has this qualified name.
pub fn impact(roots: &[Element], qualified: &str) -> Result<Vec<Dependant>, String> {
    let index = Index::build(roots);
    impact_with(roots, &index, qualified)
}

/// Everything that depends on `qualified`, including dependencies written in library files.
///
/// A project is often several files, and `load_project_with_extra` lowers the ones a model
/// imports as library rather than as roots. `impact` cannot see those: it indexes only the roots,
/// so a reference in a library file to a name the model declares answered "nothing depends on
/// this". This entry indexes both slices, which is what a project-wide answer needs.
///
/// # Errors
///
/// Returns a message naming what does exist when no element in either slice has this qualified
/// name.
pub fn impact_in(
    roots: &[Element],
    library: &[Element],
    qualified: &str,
) -> Result<Vec<Dependant>, String> {
    let mut all = Vec::with_capacity(roots.len() + library.len());
    all.extend_from_slice(roots);
    all.extend_from_slice(library);
    let index = Index::build(&all);
    impact_with(&all, &index, qualified)
}

/// The name a subject is bound to, from `subject u : Ue = deployment;`.
///
/// The binding lives on the subject element's value. It is returned as the authored text; the
/// caller resolves it, because a binding may itself be a path.
fn bound_subject(roots: &[Element], id: usize) -> Option<String> {
    fn walk(el: &Element, want: usize, out: &mut Option<String>) {
        if el.id == want {
            if let Some(text) = &el.value_text {
                *out = Some(text.trim().to_string());
            }
            return;
        }
        for child in &el.children {
            walk(child, want, out);
        }
    }
    let mut found = None;
    for root in roots {
        walk(root, id, &mut found);
        if found.is_some() {
            break;
        }
    }
    found
}

pub fn impact_with(
    roots: &[Element],
    index: &Index,
    qualified: &str,
) -> Result<Vec<Dependant>, String> {
    let start = index
        .qualified
        .iter()
        .position(|q| q == qualified)
        .ok_or_else(|| {
            let leaf = qualified.split("::").last().unwrap_or(qualified);
            // A misspelt leaf (`Componet` for `Component`) is usually a transposition, which no
            // prefix test sees, so the whole element name is compared more loosely: if the
            // shorter of the two appears inside the longer in order, it is worth offering.
            let close: Vec<&str> = index
                .qualified
                .iter()
                .take(index.len() - index.library_len())
                .filter(|q| {
                    let candidate = q.split("::").last().unwrap_or(q);
                    candidate == leaf
                        || candidate.starts_with(leaf)
                        || leaf.starts_with(candidate)
                        || is_subsequence(leaf, candidate)
                        || is_subsequence(candidate, leaf)
                })
                .map(String::as_str)
                .take(3)
                .collect();
            match close.is_empty() {
                true => format!("no element `{qualified}` in this model"),
                false => format!(
                    "no element `{qualified}` in this model; did you mean {}",
                    close
                        .iter()
                        .map(|c| format!("`{c}`"))
                        .collect::<Vec<_>>()
                        .join(" or ")
                ),
            }
        })?;

    // Who names what, built once: target id -> the references that land on it.
    let mut dependants: BTreeMap<usize, Vec<(usize, String, bool)>> = BTreeMap::new();
    for reference in index.references(roots) {
        if let Resolution::Found(target) = reference.target {
            dependants.entry(target).or_default().push((
                reference.from,
                reference.text.clone(),
                reference.in_predicate,
            ));
        }
    }

    // `subject u : Ue = deployment;` with `u.cycle` in the constraint: the name resolves through
    // the *type*, so the dependency lands on `Ue::cycle` and changing `deployment.cycle` reaches
    // nothing. The evaluator already reads the bound instance; the dependency graph has to agree
    // with it, or `diff` reports a requirement as affected while `impact` reports none.
    for reference in index.references(roots) {
        if !reference.in_predicate {
            continue;
        }
        let Some((head, rest)) = reference.text.split_once('.') else {
            continue;
        };
        let Resolution::Found(head_id) = index.resolve(reference.from, head) else {
            continue;
        };
        if index.kind.get(head_id) != Some(&Kind::Subject) {
            continue;
        }
        let Some(bound) = bound_subject(roots, head_id) else {
            continue;
        };
        let rewritten = format!("{bound}.{rest}");
        let name = sysml_model::NameRef::new(&rewritten, 0);
        if let Resolution::Found(target) = index.resolve_path(reference.from, &name) {
            dependants
                .entry(target)
                .or_default()
                .push((reference.from, rewritten, true));
        }
    }

    let mut seen: BTreeSet<usize> = BTreeSet::new();
    seen.insert(start);
    let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
    queue.push_back((start, 0));
    let mut out = Vec::new();
    while let Some((current, distance)) = queue.pop_front() {
        let Some(sources) = dependants.get(&current) else {
            continue;
        };
        for (from, text, in_predicate) in sources {
            if !seen.insert(*from) {
                continue;
            }
            let what = if *in_predicate {
                format!("predicate names `{text}`")
            } else {
                format!("clause `{text}`")
            };
            out.push(Dependant {
                qualified: index.qualified[*from].clone(),
                kind: index.kind[*from].keyword().to_string(),
                because: what,
                distance: distance + 1,
            });
            queue.push_back((*from, distance + 1));
        }
    }
    Ok(out)
}

/// The requirement `id` sits inside, counting `id` itself.
///
/// A requirement's own attributes and constraints are members of it, so an edit anywhere in its
/// body is an edit to the requirement. Stopping at proper ancestors missed exactly that case.
fn containing_requirement(
    index: &Index,
    id: usize,
    is_requirement: impl Fn(usize) -> bool,
) -> Option<usize> {
    let mut at = Some(id);
    while let Some(node) = at {
        if is_requirement(node) {
            return Some(node);
        }
        at = index.parent(node);
    }
    None
}

/// Whether `short` appears inside `long` in order, allowing characters between.
fn is_subsequence(short: &str, long: &str) -> bool {
    if short.is_empty() || long.is_empty() {
        return false;
    }
    let mut rest = long.chars();
    short.chars().all(|c| rest.any(|x| x == c))
}

/// The requirements a change reaches, which is the question a reviewer actually asks.
///
/// A `requirement` is where a model states what a number must satisfy. If none of them reaches
/// the changed element, the change is internal and needs no review; if one does, its text is
/// what has to be re-read.
pub fn affected_requirements(roots: &[Element], touched: &[String]) -> Vec<String> {
    let index = Index::build(roots);
    let is_requirement = |id: usize| {
        matches!(
            index.kind[id],
            Kind::RequirementDef | Kind::RequirementUsage
        )
    };
    let mut out: BTreeSet<String> = BTreeSet::new();
    for name in touched {
        // An element inside a requirement is itself part of that requirement: changing
        // `massRequired = 2000` to `2200` edits the requirement's own body, and reporting "no
        // requirement reached" for it inverts the answer. The containment check has to start at
        // the changed element, not at everything that names it.
        if let Some(id) = index.qualified.iter().position(|q| q == name) {
            if let Some(owner) = containing_requirement(&index, id, is_requirement) {
                out.insert(index.qualified[owner].clone());
            }
        }
        let Ok(reached) = impact_with(roots, &index, name) else {
            continue;
        };
        for dependant in reached {
            let Some(id) = index
                .qualified
                .iter()
                .position(|q| q == &dependant.qualified)
            else {
                continue;
            };
            // Two ways an element reaches a requirement. Naming it directly — a `satisfy` clause
            // or a predicate that mentions it — is the obvious one. Being *contained* in one is
            // the other, and it is why this cannot stop at the direct dependants: a requirement's
            // subject is a reference from the requirement's own body, so `Drone` is reached by
            // `Endurance::d`, and `Endurance` is reached by nothing at all. Without the
            // containment step a change to `Drone` reported no affected requirement, which is
            // exactly the answer that makes this command useless.
            if is_requirement(id) {
                out.insert(dependant.qualified);
                continue;
            }
            if let Some(owner) = containing_requirement(&index, id, is_requirement) {
                out.insert(index.qualified[owner].clone());
            }
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(src: &str) -> Vec<Element> {
        sysml_syntax::load(src).expect("parses").roots
    }

    #[test]
    fn a_changed_value_is_reported_as_a_change_not_as_two_elements() {
        let before = load("package P {\n\tpart def A { attribute x : Integer; }\n}\n");
        let after = load("package P {\n\tpart def A { attribute x : Real; }\n}\n");
        let d = delta(&before, &after);
        assert!(d.added.is_empty(), "{:?}", d.added);
        assert!(d.removed.is_empty(), "{:?}", d.removed);
        assert_eq!(d.changed.len(), 1, "{:?}", d.changed);
        assert_eq!(d.changed[0].qualified, "P::A::x");
        assert_eq!(d.changed[0].fields, ["type"]);
    }

    #[test]
    fn a_package_whose_body_changed_is_not_reported_as_changed_itself() {
        // The package's child count moves, but a reader asking what changed wants the new
        // member, not its container. Both are reported, and the distinction is the `children`
        // field: the complaint about noise is answered by reading which one moved.
        let before = load("package P {\n\tpart def A;\n}\n");
        let after = load("package P {\n\tpart def A;\n\tpart def B;\n}\n");
        let d = delta(&before, &after);
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.added[0].qualified, "P::B");
        assert!(
            d.changed
                .iter()
                .any(|c| c.qualified == "P" && c.fields == ["children"]),
            "the container's child count moved: {:?}",
            d.changed
        );
    }

    #[test]
    fn a_changed_number_is_a_change() {
        // The whole reason to compare two model versions is to find out which numbers moved.
        // `Snapshot` carried kind, type, supertypes and child count but not the value, so
        // `= 80 [mm]` and `= 85 [mm]` compared equal and `diff` answered "no change" — for the
        // one edit a change-propagation study is about.
        let before = load(
            "package P {
	part def A { attribute d : Real = 80; }
}
",
        );
        let after = load(
            "package P {
	part def A { attribute d : Real = 85; }
}
",
        );
        let d = delta(&before, &after);
        assert_eq!(d.changed.len(), 1, "{:?}", d.changed);
        assert_eq!(d.changed[0].qualified, "P::A::d");
        assert_eq!(d.changed[0].fields, ["value"], "{:?}", d.changed[0].fields);
        assert_eq!(d.changed[0].before.value.as_deref(), Some("80"));
        assert_eq!(d.changed[0].after.value.as_deref(), Some("85"));
    }

    #[test]
    fn an_identical_model_has_an_empty_delta() {
        let src = "package P {\n\tpart def A { attribute x : Integer; }\n}\n";
        assert!(delta(&load(src), &load(src)).is_empty());
    }

    #[test]
    fn impact_finds_the_attribute_a_predicate_names() {
        let roots = load(
            "package P {\n\tprivate import ScalarValues::*;\n\
             \tpart def D { attribute endurance : Real; }\n\
             \trequirement R { subject d : D; require constraint { d.endurance >= 1.0 } }\n}\n",
        );
        let reached = impact(&roots, "P::D::endurance").expect("the attribute exists");
        assert!(
            reached.iter().any(|d| d.qualified == "P::R"),
            "the requirement names it: {reached:?}"
        );
    }

    #[test]
    fn impact_does_not_list_the_element_it_started_from() {
        let roots = load("package P {\n\tpart def A :> B;\n\tpart def B;\n}\n");
        let reached = impact(&roots, "P::B").expect("B exists");
        assert!(
            !reached.iter().any(|d| d.qualified == "P::B"),
            "an element is not its own dependency: {reached:?}"
        );
    }

    #[test]
    fn a_change_to_a_subject_reaches_the_requirement_that_states_it() {
        // The requirement reaches the part through its subject, which is a reference from
        // inside the requirement's own body. Nothing names the requirement, so a walk that
        // only follows direct dependants reported "no requirements reached" for a change to
        // the very part the requirement constrains.
        let before = load(
            "package P {
	private import ScalarValues::*;
             	part def D { attribute e : Real; }
             	requirement R { subject d : D; require constraint { d.e >= 1.0 } }
}
",
        );
        let after = load(
            "package P {
	private import ScalarValues::*;
             	part def D { attribute e : Real; attribute m : Real; }
             	requirement R { subject d : D; require constraint { d.e >= 1.0 } }
}
",
        );
        let d = delta(&before, &after);
        let reached = affected_requirements(&after, &d.touched());
        assert_eq!(reached, ["P::R"], "got {reached:?}");
    }

    #[test]
    fn a_change_inside_a_requirement_reaches_that_requirement() {
        // The commonest edit of all: a bound moves. `massRequired = 2000` becoming `2200` edits
        // the requirement's own body, and it is the requirement that has to be re-read. Looking
        // only at what names the changed element answered "no requirement reached", which
        // inverts the answer for the edit this whole command exists to serve.
        let before = load(
            "package P {
	private import ScalarValues::*;
             	requirement R {
		attribute limit : Real = 2000;
             		require constraint { limit >= 1.0 }
	}
}
",
        );
        let after = load(
            "package P {
	private import ScalarValues::*;
             	requirement R {
		attribute limit : Real = 2200;
             		require constraint { limit >= 1.0 }
	}
}
",
        );
        let d = delta(&before, &after);
        let reached = affected_requirements(&after, &d.touched());
        assert_eq!(reached, ["P::R"], "got {reached:?}");
    }

    #[test]
    fn a_change_no_requirement_reaches_is_reported_as_such() {
        let before = load(
            "package P {
	part def A;
}
",
        );
        let after = load(
            "package P {
	part def A;
	part def B;
}
",
        );
        let d = delta(&before, &after);
        assert!(
            affected_requirements(&after, &d.touched()).is_empty(),
            "nothing states a constraint over these"
        );
    }

    #[test]
    fn an_unknown_name_says_what_exists_instead() {
        let roots = load("package P {\n\tpart def Component;\n}\n");
        let err = impact(&roots, "P::Componet").unwrap_err();
        assert!(err.contains("P::Component"), "{err}");
    }

    #[test]
    fn a_rename_is_a_removal_and_an_addition_and_impact_follows_the_new_name() {
        // The module comment states the contract: the name is the identity, so a rename cannot be
        // followed by `delta`. This test pins both halves of that: the delta reports the pair, and
        // `impact` on the new name still finds the requirement that writes the reference, because
        // the walk follows resolution rather than text.
        let before = load(
            "package P {\n\tprivate import ScalarValues::*;\n\
             \tpart def Drone { attribute endurance : Real; }\n\
             \trequirement R { subject d : Drone; require constraint { d.endurance >= 1.0 } }\n}\n",
        );
        let after = load(
            "package P {\n\tprivate import ScalarValues::*;\n\
             \tpart def Drone { attribute range : Real; }\n\
             \trequirement R { subject d : Drone; require constraint { d.range >= 1.0 } }\n}\n",
        );

        let d = delta(&before, &after);
        assert!(
            d.removed
                .iter()
                .any(|s| s.qualified == "P::Drone::endurance"),
            "the old name is gone: {:?}",
            d.removed.iter().map(|s| &s.qualified).collect::<Vec<_>>()
        );
        assert!(
            d.added.iter().any(|s| s.qualified == "P::Drone::range"),
            "the new name is new: {:?}",
            d.added.iter().map(|s| &s.qualified).collect::<Vec<_>>()
        );
        assert!(
            d.changed.iter().any(|c| c.qualified == "P::R"),
            "the requirement's written constraint changed too: {:?}",
            d.changed.iter().map(|c| &c.qualified).collect::<Vec<_>>()
        );

        let reached =
            impact(&after, "P::Drone::range").expect("the attribute exists after the rename");
        assert!(
            reached.iter().any(|x| x.qualified == "P::R"),
            "impact follows the resolved reference, not the old spelling: {reached:?}"
        );
    }

    #[test]
    fn impact_crosses_a_file_boundary() {
        // The corpus is one project split across directories, so a reference in one file to a
        // type in another is the normal case. `impact` must see it, or the tool answers "nothing
        // depends on this" for the files a project is actually made of.
        let library = "package Lib {\n\tpart def Motor { attribute power : Real; }\n}\n";
        let roots = "package P {\n\tprivate import Lib::*;\n\
                     \tpart def Drone { part motor : Motor; }\n\
                     \trequirement R { subject d : Drone; require constraint { d.motor.power >= 1.0 } }\n}\n";
        let loaded = sysml_syntax::load_project_with_extra(&[roots], &[library]).expect("loads");
        let reached = impact_in(&loaded.roots, &loaded.library, "Lib::Motor::power")
            .expect("the attribute exists");
        assert!(
            reached.iter().any(|x| x.qualified == "P::R"),
            "the requirement in another file reaches it: {reached:?}"
        );
    }

    #[test]
    fn a_value_change_reaches_the_requirement_through_the_type_chain() {
        // The chain the tool is for: edit a leaf value, ask which requirement states something
        // about it. The walk goes backwards through types and subjects, so a requirement about
        // the vehicle hears about a change to one part's mass.
        let before = load(
            "package P {\n\tprivate import ScalarValues::*;\n\
             \tpart def Vehicle { attribute mass : Real; }\n\
             \trequirement R { subject v : Vehicle; require constraint { v.mass <= 20.0 } }\n}\n",
        );
        let after = load(
            "package P {\n\tprivate import ScalarValues::*;\n\
             \tpart def Vehicle { attribute mass : Real; }\n\
             \trequirement R { subject v : Vehicle; require constraint { v.mass <= 25.0 } }\n}\n",
        );
        let d = delta(&before, &after);
        let affected = affected_requirements(&after, &d.touched());
        assert!(
            affected.iter().any(|r| r == "P::R"),
            "the requirement that states the bound is affected: {affected:?}"
        );
    }

    #[test]
    fn the_indexed_forms_answer_what_the_plain_forms_do() {
        // `delta_with` and `impact_with` exist so a caller that already built the indexes does not
        // pay for them twice. That is only worth having if they give the same answer; a second
        // implementation that drifted would make the canvas and the command line disagree about what
        // changed.
        let before = load(
            "package P {\n\tprivate import ScalarValues::*;\n\
         \tpart def D { attribute endurance : Real; }\n\
         \trequirement R { subject d : D; require constraint { d.endurance >= 1.0 } }\n}\n",
        );
        let after = load(
            "package P {\n\tprivate import ScalarValues::*;\n\
         \tpart def D { attribute endurance : Real; }\n\
         \trequirement R { subject d : D; require constraint { d.endurance >= 2.0 } }\n}\n",
        );

        let plain = delta(&before, &after);
        let before_index = Index::build(&before);
        let after_index = Index::build(&after);
        let indexed = delta_with(&before, &before_index, &after, &after_index);
        assert_eq!(plain.touched(), indexed.touched());
        assert_eq!(plain.added.len(), indexed.added.len());
        assert_eq!(plain.removed.len(), indexed.removed.len());
        assert_eq!(plain.changed.len(), indexed.changed.len());

        let plain_impact = impact(&after, "P::D::endurance").expect("exists");
        let after_only = Index::build(&after);
        let indexed_impact = impact_with(&after, &after_only, "P::D::endurance").expect("exists");
        assert_eq!(
            plain_impact
                .iter()
                .map(|d| (&d.qualified, d.distance))
                .collect::<Vec<_>>(),
            indexed_impact
                .iter()
                .map(|d| (&d.qualified, d.distance))
                .collect::<Vec<_>>()
        );
    }
}
