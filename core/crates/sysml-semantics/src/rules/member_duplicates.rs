//! `member-duplicates` — two members a reader cannot tell apart by name.
//!
//! The pilot implementation states two warnings, and both are about a namespace holding two members
//! with one name:
//!
//!   * "Duplicate of other owned member name" — two members the same namespace declares.
//!   * "Duplicate of inherited member name 'X' from C" — a member whose name a supertype already
//!     provides, so a reference to it is ambiguous.
//!
//! ```text
//! class C { class D {} }
//! class D2 :> C {
//!     class D {}        // warns: `D` is inherited from C
//! }
//! ```
//!
//! # Why these are warnings
//!
//! The specification does not forbid either: `validateNamespaceDistinguishibility` is about
//! *memberships* being distinguishable, and a redeclaration that redefines the inherited member is
//! legal. The pilot reports them as warnings for the same reason: they are legal but almost always a
//! mistake, and a reader wants to know.
//!
//! # What is not counted
//!
//! An authored name only: `bind a = b` and a bare `a then b` are stored under the keyword that
//! introduced them, and two of those in one body are not two members called `bind`. Inherited copies
//! (`inherit_ports`) are skipped for the same reason they are skipped everywhere: they are the same
//! authored element.
//!
//! # When a repeated name is a redefinition rather than a duplicate
//!
//! Three exemptions, each read off a model the pilot states as clean or as a warning:
//!
//!   * The name arrives through a specialization clause. `:>` is a `Subsetting`, which is what a
//!     redefinition is made of, so `class B :> A { feature p; }` redefines `A`'s `p`
//!     (`Redefinition_Diamond0_valid.kerml.xt`).
//!   * The member states a direction. A directed member is a parameter, and repeating the type's
//!     parameter by name is how the notation re-specifies one: `action a : A { in p; }` against
//!     `action def A { in p : P; }` is `valid_ActionUsage.sysml.xt:60`.
//!   * The member is an `end`. An end is not a member a reference can name, and a connector
//!     repeats its association's end names by design (`Redefinition_End_Invalid.kerml.xt`).
//!
//! The case they are exemptions *from* is the other half of the same question: `part a : A { part b
//! : B; }` repeats a name that arrives through a `:` type clause, and a `FeatureTyping` creates no
//! redefinition, so the name resolves to two things (`Membership_distingushability_invalid.sysml.xt:43`).
//! A member that is not a feature is a duplicate either way — a `Subsetting` relates `Feature`s, so
//! a nested classifier repeating a name redefines nothing.
//!
//! Two providers is the ambiguous inheritance the pilot warns about whatever the clause is, and
//! that case is reported without consulting any of this.
//!
//! The report code this group reports under is `SEM091`.

use std::collections::HashMap;

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;

const CODE: &str = "SEM091";

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Every supertype that provides `name`, nearest layer first, deduplicated.
///
/// The walk is breadth-first over the supertype closure rather than a single lookup, because a
/// member can inherit one name from two unrelated supertypes and that is exactly the case the
/// pilot warns about ("from Part, Port").
fn inherited_providers(index: &Index, ns: usize, name: &str) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut frontier = index.supertypes_of_public(ns);
    let mut seen: std::collections::HashSet<usize> = frontier.iter().copied().collect();
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for &sup in &frontier {
            for id in index.member_of(sup, name) {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
            for anc in index.supertypes_of_public(sup) {
                if seen.insert(anc) {
                    next.push(anc);
                }
            }
        }
        frontier = next;
    }
    out
}

/// Kinds that are not members of the namespace they sit in.
/// Which namespaces declare each name reachable from `sup`, `sup` included.
///
/// A provider is the namespace that declares the member, not the member itself: a library class may
/// declare one name in several `specialization` blocks (`Occurrences::Occurrence` writes `self` four
/// times) and that is one provider, not four.
///
/// Memoised per namespace, and a namespace already known is merged rather than walked again: the
/// chain a part shares with every other part is computed once, and the next part only contributes
/// its own members.
fn providers_from(
    index: &Index,
    sup: usize,
    cache: &mut HashMap<usize, std::rc::Rc<std::collections::BTreeMap<String, Vec<usize>>>>,
) -> std::rc::Rc<std::collections::BTreeMap<String, Vec<usize>>> {
    if let Some(known) = cache.get(&sup) {
        return std::rc::Rc::clone(known);
    }
    let mut provided: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    let mut frontier = vec![sup];
    let mut seen: std::collections::HashSet<usize> = frontier.iter().copied().collect();
    while let Some(current) = frontier.pop() {
        // Everything below a known namespace is already in the answer; merging it is the whole
        // reason this is faster than walking.
        if current != sup {
            if let Some(known) = cache.get(&current) {
                for (name, owners) in known.iter() {
                    let entry = provided.entry(name.clone()).or_default();
                    for &owner in owners {
                        if !entry.contains(&owner) {
                            entry.push(owner);
                        }
                    }
                }
                continue;
            }
        }
        for &member in &index.children[current] {
            let name = index.name[member].as_str();
            // Only a *named* member, and not an import binding: what a namespace re-exports is not a
            // member of it, and an anonymous member has no name to collide on.
            if name.is_empty() || name.starts_with('<') || !is_member(&index.kind[member]) {
                continue;
            }
            let entry = provided.entry(name.to_string()).or_default();
            if !entry.contains(&current) {
                entry.push(current);
            }
        }
        for next in index.supertypes_of_public(current) {
            if seen.insert(next) {
                frontier.push(next);
            }
        }
    }
    let shared = std::rc::Rc::new(provided);
    cache.insert(sup, std::rc::Rc::clone(&shared));
    shared
}

/// The names `direct` and their closures provide from two unrelated places, with those places.
///
/// Every distinct *provider* of each name along the closure is collected. A provider is the
/// namespace that declares the member, not the member itself: a library class may declare one name
/// in several `specialization` blocks (`Occurrences::Occurrence` writes `self` four times) and that
/// is one provider, not four.
///
/// One provider is not a duplicate even when several branches reach it: `Parts::Part` and
/// `Ports::Port` both specialize `Objects::Object`, so `bind` arrives from one place twice. So only
/// the *most specific* providers are kept — the ones no other provider specializes. `self` on a port
/// arrives from `Ports::Port`, `Objects::Object`, `Occurrences::Occurrence`, `Base::Anything`,
/// `Parts::Part` and `Items::Item`; the pilot names two, "from Part, Port", and those are exactly the
/// providers no other provider specializes. One such provider is not a duplicate: `part cyl :
/// Cylinder` gets `done` only from `Parts::Part`, with `Items::Item` above it in the same chain.
fn ambiguous_inherited(
    index: &Index,
    direct: &[usize],
    reach: &mut HashMap<usize, std::rc::Rc<std::collections::BTreeMap<String, Vec<usize>>>>,
) -> Vec<(String, Vec<usize>)> {
    let mut provided: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for &sup in direct {
        for (name, owners) in providers_from(index, sup, reach).iter() {
            let entry = provided.entry(name.clone()).or_default();
            for &owner in owners {
                if !entry.contains(&owner) {
                    entry.push(owner);
                }
            }
        }
    }
    provided
        .into_iter()
        .filter_map(|(name, owners)| {
            let maximal: Vec<usize> = owners
                .iter()
                .copied()
                .filter(|&owner| {
                    !owners
                        .iter()
                        .any(|&other| other != owner && index.is_subtype(other, owner))
                })
                .collect();
            (maximal.len() >= 2).then_some((name, maximal))
        })
        .collect()
}

/// Whether `ns` is an anonymous `perform`, `exhibit` or reference-form `event` that names a feature.
///
/// `perform b.a;` lowers to the statement and a host that redefines `b.a` to carry the body; the host
/// is the same usage's body, not a second namespace, so it is not counted.
fn performs_a_feature(ns: &Element, index: &Index) -> bool {
    matches!(
        ns.kind,
        Kind::Perform | Kind::ExhibitState | Kind::EventOccurrenceUsage
    ) && !index.has_authored_name(ns.id)
        && ns.redefines.is_empty()
        && index.performs_text(ns.id).is_some()
}

/// The inherited-name ambiguities of a performance and, when it names a chain, of the chain.
fn performed_duplicates(
    ns: &Element,
    index: &Index,
    reach: &mut HashMap<usize, std::rc::Rc<std::collections::BTreeMap<String, Vec<usize>>>>,
    out: &mut Vec<Violation>,
) {
    let Some(performed) = index.performed_feature_of(ns.id) else {
        return;
    };
    let text = index.performs_text(ns.id).unwrap_or_default().to_string();
    let chain = text.contains('.');
    for (name, maximal) in ambiguous_inherited(index, &[performed], reach) {
        let names: Vec<String> = maximal
            .iter()
            .map(|&id| index.qualified[id].clone())
            .collect();
        out.push(Violation::warning(
            CODE,
            ns,
            qualified(ns, index),
            format!(
                "the {} of `{text}` inherits `{name}` from {}; a reference to it inside the \
                 usage is ambiguous (duplicate of inherited member name)",
                ns.kind.keyword(),
                names.join(", ")
            ),
        ));
        if chain {
            out.push(Violation::warning(
                CODE,
                ns,
                qualified(ns, index),
                format!(
                    "the feature chain `{text}` inherits `{name}` from {}; a reference to it \
                     through the chain is ambiguous (duplicate of inherited member name)",
                    names.join(", ")
                ),
            ));
        }
    }
}

/// The inherited-name ambiguities of every feature chain `el` writes as a specialization target.
fn chain_duplicates(
    el: &Element,
    index: &Index,
    reach: &mut HashMap<usize, std::rc::Rc<std::collections::BTreeMap<String, Vec<usize>>>>,
    out: &mut Vec<Violation>,
) {
    // `perform b.a { … }` lowers a host that redefines `b.a` to carry the body; that chain is the
    // performance's own, already reported by `performed_duplicates`.
    if matches!(
        el.kind,
        Kind::Perform | Kind::ExhibitState | Kind::EventOccurrenceUsage
    ) {
        return;
    }
    let Some(scope) = index.parent.get(el.id).copied().flatten() else {
        return;
    };
    // `:>` on a usage is a subsetting, which the tree keeps under `specializes`.
    let chains = el
        .subsets
        .iter()
        .chain(&el.specializes)
        .chain(&el.redefines)
        .chain(&el.references)
        .chain(&el.crosses)
        .map(|text| text.split('[').next().unwrap_or(text).trim())
        .filter(|text| text.contains('.') && !text.contains("::>"));
    let mut seen: Vec<&str> = Vec::new();
    for chain in chains {
        if seen.contains(&chain) {
            continue;
        }
        seen.push(chain);
        let Some(target) = index
            .resolve_path(scope, &sysml_model::NameRef::new(chain, el.line))
            .found()
        else {
            continue;
        };
        for (name, maximal) in ambiguous_inherited(index, &[target], reach) {
            let names: Vec<String> = maximal
                .iter()
                .map(|&id| index.qualified[id].clone())
                .collect();
            out.push(Violation::warning(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "the feature chain `{chain}` inherits `{name}` from {}; a reference to it \
                     through the chain is ambiguous (duplicate of inherited member name)",
                    names.join(", ")
                ),
            ));
        }
    }
}

fn is_member(kind: &Kind) -> bool {
    !matches!(
        kind,
        Kind::Import
            | Kind::Expose
            | Kind::Filter
            | Kind::Dependency
            | Kind::Alias
            | Kind::Unsupported
            | Kind::Other(_)
            | Kind::PseudoState
            // A state subaction is a `StateSubactionMembership`'s owned action, not a member of
            // the state's namespace: `entry action boot;` does not introduce `entry` as a name.
            // Counting it made a state whose supertype also has an `entry` report a duplicate of
            // a name neither of them declares.
            | Kind::StateAction
    ) && !kind.is_relationship()
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    // What each namespace provides, computed once and merged on later asks. Without this every
    // element typed by a library definition walked that definition's whole supertype chain again:
    // nine hundred parts cost nine hundred walks of the same fifty library namespaces, and the rule
    // measured as a third of the whole check.
    let mut reach: HashMap<usize, std::rc::Rc<std::collections::BTreeMap<String, Vec<usize>>>> =
        HashMap::new();
    super::walk(roots, |ns| {
        // Owned duplicates: members of one namespace sharing a name.
        //
        // Every member of a collision group is reported, not only the later ones: the pilot model
        // that pins this (`MembershipTests_Distinguishability`) declares four members called `A` in
        // one package and expects four warnings, one per declaration — each of them is a name that
        // resolves to several things.
        //
        // An `end` is not one of them. `end a; end b;` inside an association is the ordinary way to
        // declare two ends, and every association in the corpus writes two: the pilot warns on
        // neither, because an end is not a member a reference can name.
        // Every identifier a member writes, not only its name. `<one> two` and `<two> three`
        // share the identifier `two` — the first as the name, the second as the short name — and
        // the pilot reports both declarations
        // (`ShortNameTests_Distinguishibility2.kerml.xt:22`). Grouping by name alone missed
        // every short name in the suite.
        let mut groups: HashMap<String, Vec<&Element>> = HashMap::new();
        for child in &ns.children {
            if child.inherited
                || child.is_end
                || !is_member(&child.kind)
                || !index.has_authored_name(child.id)
            {
                continue;
            }
            groups.entry(child.name.clone()).or_default().push(child);
            if let Some(short) = index.short_name_of(child.id) {
                groups.entry(short.to_string()).or_default().push(child);
            }
        }
        let mut names: Vec<&String> = groups.keys().collect();
        names.sort();
        for name in names {
            let members = &groups[name];
            if members.len() < 2 {
                continue;
            }
            let others: Vec<String> = members
                .iter()
                .map(|member| qualified(member, index))
                .collect();
            for member in members {
                out.push(Violation::warning(
                    CODE,
                    member,
                    qualified(member, index),
                    format!(
                        "`{name}` is declared {} times in one namespace ({}); a reference to it is \
                         ambiguous (duplicate of other owned member name)",
                        members.len(),
                        others.join(", ")
                    ),
                ));
            }
        }

        // Two *immediate* supertypes providing one name.
        //
        // `port p1 : B` has the supertypes `B` and `Ports::Port`; `B` reaches `self` through
        // `Parts::Part` and `Ports::Port` declares one itself, so `self` inside `p1` names two
        // elements. The pilot states this per declaration and per name — "Duplicate of inherited
        // member name 'self' from Part, Port" at `port p1 : B;` — and it does *not* fire on
        // `part def B`, which has a single immediate supertype even though that supertype's own
        // chain provides `self` several times over. Counting the whole closure instead reports
        // every part definition in the corpus, which is what the first attempt did.
        // An anonymous declaration has no name for a reference to reach, so a name it inherits
        // twice cannot be ambiguous around it — except when the declaration is a flow: the pilot
        // states three duplicate warnings for `message : A from b to c;`
        // (`FlowUsage_Invalid.sysml.xt:72`, `:79`), which is anonymous because `message` is the
        // keyword and not a name. Measured: this kernel reported none of the six.
        //
        // A `perform b.a;`, `exhibit s.sa;` or `event a.areal;` is anonymous too, and still a
        // namespace of its own: it reference-subsets the feature it names and so inherits what
        // that feature inherits. The reference implementation checks every `Type` it builds
        // (`KerMLValidator.checkNamespace`, `KerMLValidator.xtend:421-448`), and the feature chain
        // `b.a` is one more — an owned feature typed through its last link. So the pilot states
        // each ambiguous name twice on that line, once at the usage and once at the chain
        // (`ActionUsage_invalid.sysml.xt:62-69`, `StateUsage_invalid` `:95`,
        // `OccurrenceUsage_invalid` `:63`). This kernel used to report neither.
        if performs_a_feature(ns, index) {
            performed_duplicates(ns, index, &mut reach, &mut out);
            return;
        }
        // A feature chain written as a specialization target (`:> b.a`, `:>> b.a`, `::> b.a`,
        // `=> b.a`) is an owned feature of its own, typed through its last link — the same kind of
        // namespace the `perform b.a` chain is, so the same check applies to it.
        chain_duplicates(ns, index, &mut reach, &mut out);
        if !index.has_authored_name(ns.id) && ns.kind != Kind::FlowUsage && !ns.is_end {
            return;
        }
        // An interface that names two types of its own is not ambiguous: `interface original :
        // FuelInterface, FuelInterface2` merges both, and the pilot reports that model clean.
        // The warning is about a name arriving from two places the model did not intend to join
        // — `port p1 : B` reaching `self` from its type and from `Ports::Port`.
        //
        // Only an interface gets the exemption. `part p7 : PartDef, AttDef` names two types too,
        // and the pilot does warn there: a `part`'s several types come from one thing being both
        // a part and a data value, which a reader has to be told about. An interface names one
        // behaviour joining two ports, and the joined names are the point.
        let direct = index.supertypes_of_public(ns.id);
        if direct.len() > 1 && !matches!(ns.kind, Kind::InterfaceDef | Kind::InterfaceUsage) {
            for (name, maximal) in ambiguous_inherited(index, &direct, &mut reach) {
                let names: Vec<String> = maximal
                    .iter()
                    .map(|&id| index.qualified[id].clone())
                    .collect();
                out.push(Violation::warning(
                    CODE,
                    ns,
                    qualified(ns, index),
                    format!(
                        "`{}` inherits `{name}` from {}; a reference to it inside the declaration \
                         is ambiguous (duplicate of inherited member name)",
                        qualified(ns, index),
                        names.join(", ")
                    ),
                ));
            }
        }

        // Inherited duplicates.
        //
        // Two shapes, and both are the pilot's: a member whose name a supertype already provides
        // (`classifier D :> C { classifier D {} }`), and a member that inherits one name from two
        // unrelated supertypes — `port q` reaching `self` through both `Ports::Port` and
        // `Parts::Part`, which is what "Duplicate of inherited member name 'self' from Part, Port"
        // names. The second shape is why the whole closure is walked: the nearest provider alone
        // hides the other, and a resolver that stops at the first hit sees one name and no defect.
        if index.supertypes_of_public(ns.id).is_empty() {
            return;
        }
        // Same exemption as the branch above, for the same model: the two branches report at
        // different elements, so each needs it.
        if matches!(ns.kind, Kind::InterfaceDef | Kind::InterfaceUsage) {
            return;
        }
        for child in &ns.children {
            if child.inherited || !is_member(&child.kind) || !index.has_authored_name(child.id) {
                continue;
            }
            // End names are not checked here: positional inherited-end conflicts are checked at
            // the end itself by the namespace pass above.
            if child.is_end {
                continue;
            }
            // A member that redefines what it collides with is the legal spelling, not a mistake:
            // `part :>> wheel` says "this is the inherited one".
            if !child.redefines.is_empty() {
                continue;
            }
            let providers = inherited_providers(index, ns.id, &child.name);
            if providers.is_empty() {
                continue;
            }
            // One provider and a feature reached through a specialization clause is the legal
            // spelling, not a duplicate: `feature B :> A1, A2 { feature p; }` repeats the `p` that
            // `A` provides and redefines it, which is what `Redefinition_Diamond0_valid.kerml.xt`
            // states as clean.
            //
            // The clause that carries the name decides. `:>` is a `Subsetting`, and a `Subsetting`
            // is what a redefinition is made of (KerML 8.3.3.3.2); `:` is a `FeatureTyping`, which
            // creates no redefinition, so the same name under a type clause is a duplicate —
            // `part a : A { part b: B; }` is
            // `Membership_distingushability_invalid.sysml.xt:43`. A member that is not a feature is
            // a duplicate either way: a `Subsetting` relates `Feature`s, so a nested classifier
            // repeating a name redefines nothing (`MembershipTests_Distinguishability.kerml.xt:36`).
            //
            // Two providers is the ambiguous inheritance the pilot warns about whatever the clause
            // is, and that case is reported below.
            let through_a_type = ns.type_ref.is_some();
            // A directed member is a parameter, and repeating the type's parameter by name is how
            // the notation re-specifies one: `action a: A { in p; out q; }` against
            // `action def A { in p : P; out q : Q; }` is `valid_ActionUsage.sysml.xt:60` and the
            // pilot reports that model clean.
            let is_a_parameter = child.direction.is_some();
            if providers.len() < 2 && child.kind.is_feature() && (!through_a_type || is_a_parameter)
            {
                continue;
            }
            // The warning names every provider, because the ambiguity is the point: a reader has to
            // know which two supertypes disagree.
            let names: Vec<String> = providers
                .iter()
                .map(|&id| index.qualified[id].clone())
                .collect();
            out.push(Violation::warning(
                CODE,
                child,
                qualified(child, index),
                format!(
                    "`{}` repeats the name `{}` that `{}` inherits from {}; without a `:>>` the \
                     two are ambiguous (duplicate of inherited member name)",
                    qualified(child, index),
                    child.name,
                    qualified(ns, index),
                    names.join(", ")
                ),
            ));
        }
    });
    out
}
