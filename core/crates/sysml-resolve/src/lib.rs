//! Name resolution: scopes, qualified names, imports, and what a reference actually points at.
//!
//! This replaces the leaf-name match the view used to do. Matching the last segment of a
//! qualified reference against the first element with that name is wrong as soon as two packages
//! declare the same name: `Cart::wheel : Wheel` then binds to whichever `Wheel` happens to come
//! first in the file. Resolution has to walk outward from the scope the reference is written in.
//!
//! What is modelled here:
//!
//!   * a pre-order index of the tree, keyed by `Element::id`;
//!   * qualified names built from the owner chain, packages included;
//!   * a scope walk — a bare name resolves against the enclosing scopes, innermost first;
//!   * relationship statements are not nameable members: a `satisfy R by x` element is named
//!     after its target for display only, and that name neither shadows nor collides with the
//!     real `R` during the walk;
//!   * qualified references, resolved segment by segment from the current scope outward;
//!   * imports, which bring names from another namespace into the scope that owns them;
//!   * ambiguity, reported rather than silently resolved to the first match.
//!
//! # Import visibility
//!
//! The textual notation requires a visibility indicator on every import (OMG issue
//! SYSML2_-207, resolved in SysML 2.0b4; the abstract-syntax default is private). What that
//! visibility controls is *re-export*, not local reach: per KerML 8.2.3.3 a private import is
//! visible inside the namespace that declares it **and inside everything nested in it** — only
//! namespaces that import the declaring namespace are shut out. So the scope walk below
//! consults every binding it passes, and `public` is the only visibility that propagates
//! further, into namespaces that import the importing one.
//!
//! The reference implementation reads this differently: `SymbolTable.lookup` hides a private
//! import from child scopes (semantic.py:433-435), so in sysmlpy a package-level
//! `private import ISQ::*` does not reach a `part def` nested in that package. That reading
//! contradicts KerML 8.2.3.3, and it would leave every unit-bearing typing in the examples
//! unresolved, so it is not copied here.
//!
//! # The bundled standard library
//!
//! Library elements are attached with [`Index::with_library`]. They are indexed so that
//! `import ISQ::*` has something to bind, but they are reachable *only* through an import: an
//! ordinary reference never falls back to a library name, because a project that does not
//! import a namespace has not said it depends on it. Import *targets* do fall back, project
//! roots first and library roots second, which is what lets `ISQ` resolve at all.

use std::collections::HashMap;

use sysml_model::{Element, ImportShape, Kind, Modifiers, NameRef, Visibility};

thread_local! {
    /// How deep the "what does this inherit" question currently is.
    static INHERITANCE_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Questions about an element that are being answered right now, on this thread.
    ///
    /// A depth counter cannot stop the cycles this file actually hits. Both "what do I
    /// specialize" and "what do I perform" are answered by running the full resolver over a
    /// clause the element was written with, and either clause can name the element itself:
    /// `perform p.a.b` lowers to a host element whose clause is the path it already sits on.
    /// Each re-entry starts at a fresh depth, so one element of a three-line model was entered
    /// 9,711 times while the nesting never passed the limit. An element already being resolved
    /// cannot contribute to its own answer, so a second entry returns nothing.
    static IN_PROGRESS: std::cell::RefCell<std::collections::HashSet<(Question, usize)>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Which of the recursive questions an element is currently being asked.
///
/// The two are tracked apart. They are different relations over the same element, and an
/// element whose supertypes are being resolved can still legitimately be asked what it
/// performs — only re-entering the *same* question means the walk has closed a loop.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Question {
    /// What does this element specialize — [`Index::supertypes_of`].
    Supertypes,
    /// What does this performance perform — [`Index::performed_feature`].
    Performs,
    /// What does this type conjugate — [`Index::conjugate_edges`].
    Conjugates,
    ImplicitRedefinitions,
    EffectiveEnds,
}

/// Decrements the inheritance depth on drop, including on an early return.
///
/// A plain increment/decrement pair looked correct and was not: any early `return` between them
/// left the counter raised for the rest of the thread, so a later query saw a depth it had not
/// reached and answered "unresolved" where the same query had succeeded a moment earlier. The
/// symptom was a model that passed one command and failed the next with no change to the model.
struct DepthGuard {
    at_limit: bool,
}

impl DepthGuard {
    fn enter() -> Self {
        let at_limit = INHERITANCE_DEPTH.with(|d| {
            if d.get() >= 32 {
                true
            } else {
                d.set(d.get() + 1);
                false
            }
        });
        Self { at_limit }
    }

    fn exhausted(&self) -> bool {
        self.at_limit
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        if !self.at_limit {
            INHERITANCE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        }
    }
}

/// Marks one question about one element as being answered, and clears the mark on drop.
///
/// `false` from `enter` means the same question about the same element is already being
/// answered further up the stack, so the caller must answer with nothing rather than ask
/// again. This is a colour mark on a graph walk, not a budget: an element on the current path
/// cannot contribute to its own answer.
struct InProgressGuard {
    key: (Question, usize),
    entered: bool,
}

impl InProgressGuard {
    fn enter(question: Question, id: usize) -> Self {
        let key = (question, id);
        let entered = IN_PROGRESS.with(|set| set.borrow_mut().insert(key));
        Self { key, entered }
    }

    fn reentered(&self) -> bool {
        !self.entered
    }
}

impl Drop for InProgressGuard {
    fn drop(&mut self) {
        if self.entered {
            IN_PROGRESS.with(|set| set.borrow_mut().remove(&self.key));
        }
    }
}

/// What a written reference turned out to mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Exactly one element matched.
    Found(usize),
    /// Several elements matched at the same scope depth. Legitimate only when the language
    /// allows overloading, which SysML v2 does not for these names, so it is a modelling error.
    Ambiguous(Vec<usize>),
    /// Nothing matched. Usually a library name reached through an `import` that is not loaded.
    Unresolved,
    /// A `::` path that reaches its target by stepping through a feature.
    ///
    /// `f::a` names `a` inside what `f` is, and the notation reaches a feature's members with `.`;
    /// `::` is for namespaces. The pilot states this as "Must be an accessible feature (use dot
    /// notation for nesting)" and reports it at the path (`FeaturePath_Invalid.sysml.xt:45`, `:61`).
    /// Imports are exempt: `public import y::y1` crosses an end and is written that way in
    /// `AssociationTest_CrossFeatures_invalid.kerml.xt`, which asserts nothing about the import.
    NotAccessible,
}

impl Resolution {
    pub fn found(&self) -> Option<usize> {
        match self {
            Resolution::Found(id) => Some(*id),
            _ => None,
        }
    }

    pub fn is_resolved(&self) -> bool {
        matches!(self, Resolution::Found(_))
    }
}

/// A reference in the model: where it was written, what it said, and what it resolved to.
#[derive(Debug, Clone)]
pub struct Reference {
    pub from: usize,
    pub text: String,
    pub target: Resolution,
    /// True when the name was written inside a requirement predicate rather than as a typing.
    /// The two are reported together but read differently: a broken typing means the model
    /// cannot be drawn, a broken predicate name means the requirement says nothing checkable.
    pub in_predicate: bool,
}

/// One name introduced by an import, with the visibility the import was written with.
/// Visibility decides re-export only; locally every binding is reachable (see the module
/// documentation).
#[derive(Debug, Clone)]
struct Binding {
    name: String,
    target: usize,
    visibility: Option<Visibility>,
}

/// One import statement and what it did, for the CLI's report.
#[derive(Debug, Clone)]
pub struct ImportEntry {
    /// Scope that owns the import; `None` for a root-level import.
    pub owner: Option<usize>,
    /// The import element itself.
    pub element: usize,
    pub shape: ImportShape,
    pub visibility: Option<Visibility>,
    /// What the target path resolved to. `Unresolved` means the import binds nothing.
    pub target: Resolution,
    /// Names this import introduced into its owning scope, in bind order.
    pub bound: Vec<(String, usize)>,
}

pub struct Index {
    pub kind: Vec<Kind>,
    /// Whether the declaration was written with `def`, per element id. `Kind` cannot answer it —
    /// `interface def I` and `interface i` share a kind — and the reference judges them
    /// differently, so a rule that reads a type has to be able to ask.
    pub is_definition: Vec<bool>,
    /// Whether the declaration was written `variant`, per element id.
    ///
    /// `variant part car` keeps `Kind::PartUsage`, so a rule cannot tell it from an ordinary part
    /// by kind. This is the flag it reads instead.
    pub is_variant: Vec<bool>,
    /// The visibility a member states, per element id; `None` means public.
    ///
    /// `private` and `protected` both keep a member out of reach of a `.` access from outside the
    /// type that declares it (`AssignmentActionUsage_invalid.sysml.xt:41`).
    pub visibility: Vec<Option<Visibility>>,
    pub name: Vec<String>,
    /// `Element::from` / `Element::to` per element id: a relationship's ends as authored text.
    ///
    /// A succession's ends are names, not references to ids, so a rule that counts how many
    /// transitions meet at one node has to read them here rather than resolve them.
    ends: Vec<(Option<String>, Option<String>)>,
    /// Whether the element at this id states a conjugator (`conjugates T` / `~T`).
    conjugated: Vec<bool>,
    /// The `conjugates T` targets as authored, per element id.
    conjugate_text: Vec<Vec<String>>,
    /// The `chains a.b` targets as authored, per element id — `Feature::ownedFeatureChaining`.
    chains: Vec<Vec<String>>,
    /// The `crosses c` targets as authored, per element id — the cross feature a declaration names.
    crosses: Vec<Vec<String>>,
    /// The direction written on each element, if any.
    /// The clause keyword that introduced each element (`require`, `assume`, `return`).
    clause: Vec<Option<String>>,
    direction: Vec<Option<sysml_model::Direction>>,
    pub qualified: Vec<String>,
    pub parent: Vec<Option<usize>>,
    /// Every `TypeFeaturing` element, by id, so a feature's explicit featuring types are found
    /// without a walk over the whole tree.
    type_featurings: Vec<usize>,
    pub children: Vec<Vec<usize>>,
    pub line: Vec<u32>,
    /// Import form and visibility per element id; `None` everywhere except on imports.
    form: Vec<Option<(ImportShape, Option<Visibility>)>>,
    /// Bindings per owning scope, indexed by scope id.
    scope_bindings: Vec<Vec<Binding>>,
    /// Bindings from root-level imports, which have no owning scope.
    root_bindings: Vec<Binding>,
    /// Names bound by each import element, for reporting.
    bound_by: HashMap<usize, Vec<(String, usize)>>,
    /// Authored `type_ref` per element id, kept so a `.` access can walk into a usage's type.
    type_text: Vec<Option<String>>,
    /// Every type a usage names, in authored order. `type_text` keeps the first for the callers
    /// that want one; the member walk needs all of them, because `part p7 : PartDef, AttDef`
    /// inherits from both (`PartUsage_invalid.sysml.xt:63`).
    type_list: Vec<Vec<String>>,
    /// The `:>>` / `redefines` targets per element, resolved differently from a supertype.
    redefines: Vec<Vec<String>>,
    /// The `subsets` targets per element.
    ///
    /// Kept apart from `supertypes` even though that union already carries them, because
    /// `deriveFeatureType` follows subsetting and *only* subsetting: `typingFeatures()` is
    /// `subsetting->reject(oclIsKindOf(CrossSubsetting)).subsettedFeatures`. Reading the union
    /// instead would also inherit through `performs`, which is a different relation, and the three
    /// rules that ask for `type` would then answer about a feature the declaration never named.
    /// `crosses` and `references` are their own fields in the syntax layer and are not in here.
    subsets: Vec<Vec<String>>,
    /// The `references` targets per element.
    ///
    /// `ReferenceSubsetting` is a `Subsetting` (`KerML 8.3.3.3.2`), so it is one of the edges
    /// `typingFeatures()` follows even though this tree records it in its own field.
    references: Vec<Vec<String>>,
    /// The full specialization clause per element id, in authored order. Empty for an element
    /// that writes no `:>` clause, and for every usage (whose `type_ref` is a type, not a
    /// supertype). Kept separate from `type_text` so a comma list is never reconstructed by
    /// splitting a string that legitimately contains punctuation.
    supertypes: Vec<Vec<String>>,
    /// The `<B>` short name of each element, when it authored one. A short name is an
    /// alternative spelling that references may use, not a separate element.
    short_name: Vec<Option<String>>,
    /// The feature each `perform` / `exhibit` performs, as authored. Resolved lazily in the
    /// member walk, which is where the body's names are looked up.
    performs: Vec<Option<String>>,
    /// `Element::value_text` per element id — the authored right-hand side of `= v`.
    ///
    /// Kept because §7.27.3 derives an implicit specialization from the *value* of
    /// `SemanticMetadata::baseType`: `feature :>> baseType = f meta KerML::Type;` means the annotated
    /// element implicitly specializes what `f` denotes. The rule runs from an element id, so the
    /// text has to be reachable without walking back to the tree.
    value_text: Vec<Option<String>>,
    /// `Element::to` per element id: the `about` targets of a metadata annotation. An annotation
    /// that names its targets is not attached to the element that holds it, so the derivation in
    /// [`Index::implicit_metadata_supertypes`] must not treat it as annotating its parent.
    annotations_about: Vec<Option<String>>,
    /// `Element::is_end` per element id. Kept in the index because a rule comparing a feature
    /// with the one it redefines has the target's id, not the element.
    is_end: Vec<bool>,
    inherited: Vec<bool>,
    /// `Element::is_variable` per element id — `mayTimeVary`, which redefines KerML's
    /// `is_variable`. `validateAssignmentActionUsage` asks it of the feature an assignment writes,
    /// and that feature is reached by id rather than walked to as an element.
    may_vary: Vec<bool>,
    /// `Element::is_composite`, which is `composite` written or `ref` omitted.
    is_composite: Vec<bool>,
    /// The specialization closure per element id, computed once.
    ///
    /// `supertypes_of` resolves the written clause through `resolve_path`, which walks scopes and
    /// consults the inheritance graph. On a model with a deep chain of `D_n :> D_{n-1}` that walk is
    /// O(depth), and the rule pass calls `is_subtype` many times per element, so the total is O(n²):
    /// measured at 400 chained definitions, `sysml check` took 80.5s against 2.9s for the same number
    /// of *independent* definitions, and the doubling curve was ×3.86.
    ///
    /// The graph does not change after `Index::build`, so the answer per id can be computed once. A
    /// `OnceCell` per id would allocate 8+ bytes per element for a lazy answer; a plain map filled on
    /// demand is enough because the rule pass asks about every element anyway.
    supertype_cache: std::cell::RefCell<std::collections::HashMap<usize, Vec<usize>>>,
    /// The direct supertypes per element, filled on demand once
    /// [`Index::enable_supertype_cache`] has run.
    ///
    /// Distinct from [`Self::supertype_cache`], which holds the transitive closure: this holds the
    /// *edges*. Cached separately because the two answer different questions — a caller that wants the
    /// direct supertypes must not receive the whole ancestry.
    supertype_edges_cache: std::cell::RefCell<std::collections::HashMap<usize, Vec<usize>>>,
    /// Whether [`Self::supertype_edges_cache`] is live, meaning the index is complete and an answer
    /// recorded now will stay true.
    ///
    /// A flag rather than a "is the map empty" test, because the map is filled lazily: absent means
    /// "not asked yet" and an empty vector means "asked, and there are none", and the two must not be
    /// confused. Recording answers before this flag is set is a real failure mode, not a theoretical
    /// one — it produced 48 spurious diagnostics.
    supertype_edges_ready: std::cell::Cell<bool>,
    /// Which library element each implicit base names, resolved once per name.
    ///
    /// The base is found by qualified name (`Parts::Part`), and that lookup used to be a linear
    /// scan of `qualified` — which carries the whole standard library — on every call. `None` is a
    /// legitimate value ("this checkout's library has no such element"), so the map stores the
    /// `Option` rather than only the hits.
    implicit_base_cache: std::cell::RefCell<std::collections::HashMap<&'static str, Option<usize>>>,
    /// `Element::is_render` per element id: `render r` rather than `rendering r`. A rule that
    /// bounds a view to one rendering has the element, but a walk over children only has ids.
    is_render: Vec<bool>,
    /// `Element::membership` per element id: how the member was introduced.
    membership: Vec<sysml_model::Membership>,
    /// The authored bound per element id (`[4]`, `[0..*]`), kept because a rule comparing two
    /// features' bounds has the other one's id, not its element.
    multiplicity: Vec<Option<String>>,
    /// `Element::is_initial` per element id: the value was written with `:=`.
    initial: Vec<bool>,
    /// `Element::is_default` per element id: the value was written with `default`.
    ///
    /// Distinct from `initial` in both directions. The grammar is
    /// `'=' | isInitial ?= ':=' | isDefault ?= 'default' ('=' | ':=')?` (`KerML.xtext:744-750`), so a
    /// plain `=` sets neither flag, and `validateFeatureValueOverriding` — whose OCL asks for
    /// `valuation->forAll(isDefault)` — rejects a redefinition standing on a plain value exactly as it
    /// rejects one standing on `:=`.
    defaulted: Vec<bool>,
    /// Whether the element states a value at all (`= v`, `:= v` or `default = v`).
    valued: Vec<bool>,
    /// `Element::modifiers` per element id, for the same reason: `Subsetting` conformance reads
    /// the flags of the feature at the other end of the relation.
    modifiers: Vec<Modifiers>,
    /// Whether each element owns the name it is stored under. A relationship statement does
    /// not: `satisfy R by x` is stored as `R` for display, and the anonymous `connect a to b;`
    /// is stored as `connect`. A *named* connection usage does own its name, and marking it
    /// unbindable hid `connection trailerHitch : TrailerHitch` from every dotted read.
    named: Vec<bool>,
    /// Whether the author wrote a name, for any kind — including the ones `named` waves through.
    authored_name: Vec<bool>,
    /// First library element id; equal to `len()` when no library is attached.
    library_start: usize,
}

/// Whether a binding replaces an existing one of the same name or only fills a gap.
///
/// The reference implementation gets the same effect from dict assignment: direct members of
/// an imported namespace overwrite (`semantic.py:_resolve_namespace_import`), while re-exports
/// and recursive imports fill only if absent. A later import in the same scope still wins,
/// because lookup takes the last match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fill {
    Overwrite,
    IfAbsent,
}

/// A name with its surrounding apostrophes removed, and trimmed.
///
/// The notation quotes a name that contains spaces or punctuation — `'3a-Behavior'`,
/// `'provide power'` — and the quotes are not part of it. `Element::name` holds the unquoted
/// form, so a reference that keeps its quotes matches nothing. Single names were unquoted when
/// the parser read them; the segments of a qualified path were not, so `'A'::'b'` resolved to
/// nothing while `A::b` resolved.
fn unquote(segment: &str) -> &str {
    let text = segment.trim();
    match text.strip_prefix('\'') {
        Some(inner) => inner.strip_suffix('\'').unwrap_or(inner),
        None => text,
    }
}

impl Index {
    pub fn build(roots: &[Element]) -> Self {
        let mut index = Index {
            kind: Vec::new(),
            type_featurings: Vec::new(),
            is_definition: Vec::new(),
            is_variant: Vec::new(),
            visibility: Vec::new(),
            name: Vec::new(),
            ends: Vec::new(),
            conjugate_text: Vec::new(),
            chains: Vec::new(),
            crosses: Vec::new(),
            clause: Vec::new(),
            conjugated: Vec::new(),
            direction: Vec::new(),

            qualified: Vec::new(),
            parent: Vec::new(),
            children: Vec::new(),
            line: Vec::new(),
            form: Vec::new(),
            scope_bindings: Vec::new(),
            root_bindings: Vec::new(),
            bound_by: HashMap::new(),
            type_text: Vec::new(),
            type_list: Vec::new(),
            redefines: Vec::new(),
            subsets: Vec::new(),
            references: Vec::new(),
            supertypes: Vec::new(),
            short_name: Vec::new(),
            performs: Vec::new(),
            value_text: Vec::new(),
            annotations_about: Vec::new(),
            is_end: Vec::new(),
            inherited: Vec::new(),
            may_vary: Vec::new(),
            is_composite: Vec::new(),
            supertype_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            supertype_edges_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            supertype_edges_ready: std::cell::Cell::new(false),
            implicit_base_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            is_render: Vec::new(),
            membership: Vec::new(),
            multiplicity: Vec::new(),
            initial: Vec::new(),
            defaulted: Vec::new(),
            valued: Vec::new(),
            modifiers: Vec::new(),
            named: Vec::new(),
            authored_name: Vec::new(),
            library_start: 0,
        };
        for root in roots {
            index.insert(root, None);
        }
        index.library_start = index.kind.len();
        index.type_featurings = (0..index.kind.len())
            .filter(|&id| index.kind[id] == Kind::TypeFeaturing)
            .collect();
        index.bind_imports();
        index
    }

    /// Attach library elements. Their ids continue after the project's, so one index holds
    /// both trees and a reference can resolve into the library.
    pub fn with_library(mut self, library: &[Element]) -> Self {
        let start = self.kind.len();
        for element in library {
            self.insert(element, None);
        }
        self.library_start = start;
        self.bind_imports();
        self
    }

    // `bind_standard_library` lived here and was removed after measurement. The idea was KerML's:
    // a `standard library package` has its members visible from the root namespace of every model,
    // which would explain why `11a-View-Viewpoint.sysml:49` writes `render asElementTable` without
    // importing `Views`. Binding every such member into every root made the pilot lose six
    // assertions (350/387 → 344/387, files exact 74 → 69): names the suite states are undefined
    // resolved to library elements of the same leaf name. The reference implementation therefore
    // does not treat those members as globally visible under their simple names. Whatever makes
    // `asElementTable` resolve there is narrower than this, and is still an open question.

    fn insert(&mut self, el: &Element, parent: Option<usize>) {
        let id = el.id;
        debug_assert_eq!(id, self.kind.len(), "ids must be assigned in pre-order");
        // An anonymous element is not a name, so it does not become a segment of one: a report about
        // `@Bogus` belongs to the element it annotates (`R::X`), not to `R::X::<anonymous>`.
        let anonymous = el.name == "<anonymous>";
        let qualified = match parent {
            Some(p) if anonymous => self.qualified[p].clone(),
            Some(p) => format!("{}::{}", self.qualified[p], el.name),
            None => el.name.clone(),
        };
        self.kind.push(el.kind.clone());
        self.is_definition.push(el.is_definition);
        self.is_variant.push(el.is_variant);
        self.visibility.push(el.visibility);
        self.name.push(el.name.clone());
        self.ends.push((el.from.clone(), el.to.clone()));
        self.conjugated.push(!el.conjugates.is_empty());
        self.conjugate_text.push(el.conjugates.clone());
        self.chains.push(el.chains.clone());
        self.crosses.push(el.crosses.clone());
        self.clause.push(el.clause.clone());
        self.direction.push(el.direction);
        self.qualified.push(qualified);
        self.parent.push(parent);
        self.children.push(Vec::new());
        // One binding bucket per element, kept in step with the id space so `with_library`
        // can grow the index without re-sizing anything by hand.
        self.scope_bindings.push(Vec::new());
        self.line.push(el.line);
        self.type_text.push(el.type_ref.clone());
        self.type_list.push(el.type_refs.clone());
        self.redefines.push(el.redefines.clone());
        self.subsets.push(el.subsets.clone());
        self.references.push(el.references.clone());
        // `subsets` is a `Specialization` — "Subsetting is Specialization in which the specific and
        // general Types are Features" (KerML 8.3.3.3.2) — so a subsetting feature is a subtype of
        // what it subsets. Folding it in here keeps every `specializes` question answered from one
        // list; leaving it out made `feature c subsets b` state no relation at all, and
        // `validateFeatureCrossFeatureSpecialization` then reported a conforming model.
        let mut supertypes = el.supertypes.clone();
        supertypes.extend(el.subsets.iter().cloned());
        self.supertypes.push(supertypes);
        // A relationship element only names a member when the source gave it a name of its own.
        // `satisfy R by x` never does; `connect a to b;` never does; `connection c connect ...`
        // does, and its `name_span` is the evidence.
        self.named
            .push(!el.kind.is_relationship() || el.name_span.is_some());
        self.authored_name.push(el.name_span.is_some());
        self.short_name.push(el.short_name.clone());
        self.performs.push(el.performs.clone());
        self.value_text.push(el.value_text.clone());
        self.annotations_about.push(el.to.clone());
        self.is_end.push(el.is_end);
        self.inherited.push(el.inherited);
        self.may_vary.push(el.modifiers.is_variable);
        self.is_composite.push(el.is_composite);
        self.is_render.push(el.is_render);
        self.membership.push(el.membership);
        self.multiplicity.push(el.multiplicity.clone());
        self.initial.push(el.is_initial);
        self.defaulted.push(el.is_default);
        self.valued.push(el.value_expr.is_some() || el.is_initial);
        self.modifiers.push(el.modifiers.clone());
        self.form.push(match el.kind {
            Kind::Import => Some((
                el.import_shape.unwrap_or(ImportShape::Filter),
                el.visibility,
            )),
            _ => None,
        });
        if let Some(p) = parent {
            self.children[p].push(id);
        }
        for child in &el.children {
            self.insert(child, Some(id));
        }
    }

    pub fn len(&self) -> usize {
        self.kind.len()
    }

    /// Number of elements belonging to the attached library.
    ///
    /// Reporting code uses this to separate "what the project contains" from "what the standard
    /// library defines": a marker in the library is not a coverage gap in the model under review.
    pub fn library_len(&self) -> usize {
        self.len() - self.library_start
    }

    /// Whether this id belongs to the attached standard library rather than to the model.
    ///
    /// The library is one tree of elements whose ids continue after the model's, so the boundary
    /// is the id. Several rules count what a declaration *inherits*, and a count that included the
    /// library would count what every model has in common rather than what this model states.
    pub fn is_library(&self, id: usize) -> bool {
        id >= self.library_start
    }

    /// Diagnostic: the feature this element performs, as an id.
    pub fn debug_performed(&self, id: usize) -> Option<usize> {
        self.performed_feature(id)
    }

    /// The feature a `perform`, `exhibit` or reference-form `event` denotes, as an id.
    ///
    /// Such a usage reference-subsets that feature (`PerformActionUsageDeclaration`,
    /// `SysML.xtext:1411-1425`), so it inherits what the feature inherits.
    pub fn performed_feature_of(&self, id: usize) -> Option<usize> {
        self.performed_feature(id)
    }

    /// The feature reference a `perform`, `exhibit` or `event` was written with, as authored.
    pub fn performs_text(&self, id: usize) -> Option<&str> {
        self.performs.get(id)?.as_deref()
    }

    /// Diagnostic: the authored `performs` text.
    pub fn debug_performs_text(&self, id: usize) -> Option<String> {
        self.performs[id].clone()
    }

    /// Diagnostic: what does this element claim to specialize, as resolved ids?
    pub fn debug_supertypes(&self, id: usize) -> Vec<usize> {
        self.supertypes_of(id)
    }

    /// Whether the element at `id` states a conjugator.
    pub fn is_conjugated(&self, id: usize) -> bool {
        self.conjugated.get(id).copied().unwrap_or(false)
    }

    /// The short name written for the element at `id`, if any.
    ///
    /// `<one> two` declares the short name `one` and the name `two`, and both are identifiers a
    /// reference may use: two members sharing either one are indistinguishable.
    pub fn short_name_of(&self, id: usize) -> Option<&str> {
        self.short_name.get(id).and_then(|n| n.as_deref())
    }

    /// The type reference written for the element at `id`, if any.
    pub fn type_of(&self, id: usize) -> Option<&str> {
        self.type_text.get(id).and_then(|t| t.as_deref())
    }

    /// The child ids of the element at `id`, in authored order.
    fn children_of(&self, id: usize) -> Vec<usize> {
        self.children.get(id).cloned().unwrap_or_default()
    }

    /// The `about` target list written on the annotation at `id`.
    ///
    /// An annotation with `about` names what it annotates, so §7.27.3 applies to those named
    /// elements rather than to the element holding the annotation.
    fn to_text(&self, id: usize) -> Option<String> {
        self.annotations_about.get(id).cloned().flatten()
    }

    /// Every type the element at `id` declares, in authored order.
    ///
    /// `part p : A, B` names two, and a caller that asks for "the type" gets the first one. A rule
    /// that must not judge half a declaration asks this instead.
    pub fn types_of(&self, id: usize) -> &[String] {
        self.type_list.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Whether the element at `id` is an instance of `metaclass`.
    ///
    /// This is `oclIsKindOf` for a rule written against the metamodel. The answer follows the
    /// metaclass hierarchy, not the projection: `part p : Wheel` is a `PartUsage`, which is a
    /// `Usage`, a `Feature`, an `ItemUsage`, an `OccurrenceUsage`, a `Type`, a `Namespace` and an
    /// `Element`, and is not a `Definition` or a `Behavior`.
    ///
    /// A kind that stands for no metaclass — [`Kind::Variant`](sysml_model::Kind::Variant),
    /// `Unsupported`, `Other` — answers `false` for everything, including `"Element"`. A rule that
    /// asks about one of those must treat the answer as "not modelled" rather than as "does not
    /// apply": the tree cannot tell whether the element is one.
    pub fn is_kind_of(&self, id: usize, metaclass: &str) -> bool {
        self.kind
            .get(id)
            .is_some_and(|kind| kind.is_metaclass(metaclass))
    }

    /// One authored end of the element at `id`: `true` for its source, `false` for its target.
    pub fn end_text(&self, id: usize, source: bool) -> Option<&str> {
        let (from, to) = self.ends.get(id)?;
        let end = if source {
            from.as_deref()
        } else {
            to.as_deref()
        }?;
        Some(end)
    }

    /// Whether the element at `id` was declared with the `end` prefix.
    pub fn is_end_of(&self, id: usize) -> bool {
        self.is_end.get(id).copied().unwrap_or(false)
    }

    /// Whether the feature at `id` may vary in time — `mayTimeVary`, the SysML redefinition of
    /// KerML's `is_variable`. An assignment must write such a feature, so a rule about one needs
    /// to ask it of the referent, which arrives as an id.
    ///
    /// `deriveUsageMayTimeVary` is a conjunction of four conditions, and reading only the first
    /// gave the wrong answer for an action nested in an item:
    ///
    /// ```text
    /// mayTimeVary =
    ///     owningType <> null and
    ///     owningType.specializesFromLibrary('Occurrences::Occurrence') and
    ///     not (
    ///         isPortion or
    ///         specializesFromLibrary('Links::SelfLink') or
    ///         specializesFromLibrary('Occurrences::HappensLink') or
    ///         isComposite and specializesFromLibrary('Actions::Action')
    ///     )
    /// ```
    ///
    /// The last exclusion is the one that matters most in practice: a `composite` feature that is an
    /// `Action` does not vary, because an action nested in an occurrence is part of that occurrence's
    /// structure rather than a value that changes over its lifetime. `assign i.d := null` where
    /// `d` is `action d;` inside an `item` is therefore rejected, which is what the reference does
    /// (`AssignmentActionUsage_invalid.sysml.xt:45`).
    ///
    /// A written `var` still decides on its own: it states the intent directly, and no derivation
    /// overrides an explicit modifier. `constant` likewise.
    pub fn may_vary_of(&self, id: usize) -> bool {
        // `constant` is a written denial and outranks the derivation in both directions.
        if self
            .modifiers
            .get(id)
            .map(|m| m.is_constant)
            .unwrap_or(false)
        {
            return false;
        }
        // A written `var` does **not** short-circuit the derivation, which is what an earlier
        // version of this function did. `isVariable` is a KerML property about the feature's value;
        // `mayTimeVary` is the SysML redefinition that *also* requires an owner with a lifetime. A
        // `var attribute` inside an `attribute def` writes a feature that changes, but nothing
        // changes **over time** — the containing type is a datatype, not an occurrence — so an
        // assignment to it has no meaning and the reference rejects it. `var` is reported on its own
        // by `validateFeatureIsVariable` (SEM077), which is a different question.
        self.derived_may_time_vary(id)
    }

    /// `deriveUsageMayTimeVary`, applied to the tree.
    ///
    /// Split out from [`Index::may_vary_of`] because the derivation reads supertypes and the
    /// written modifier does not; keeping them in one function made the written flag look like the
    /// whole answer.
    fn derived_may_time_vary(&self, id: usize) -> bool {
        let Some(owner) = self.parent.get(id).copied().flatten() else {
            return false;
        };
        if !self.specializes_from_library(owner, "Occurrences::Occurrence") {
            return false;
        }
        if self.is_portion(id)
            || self.specializes_from_library(id, "Links::SelfLink")
            || self.specializes_from_library(id, "Occurrences::HappensLink")
        {
            return false;
        }
        let composite = self
            .modifiers
            .get(id)
            .map(|m| m.portion.is_some() || self.is_composite_flag(id))
            .unwrap_or(false);
        if composite && self.specializes_from_library(id, "Actions::Action") {
            return false;
        }
        true
    }

    /// `isPortion` — the feature was written with `portion`, `snapshot` or `timeslice`.
    ///
    /// The three are the same question at different grains: a snapshot is a portion at an instant and
    /// a timeslice is one over an interval, and all three make the feature a part of its owner rather
    /// than a value of its own.
    fn is_portion(&self, id: usize) -> bool {
        self.modifiers
            .get(id)
            .and_then(|m| m.portion.as_ref())
            .is_some()
    }

    /// Whether the effective value of `composite` is true: written `composite`, or not written `ref`.
    ///
    /// `Element::is_composite` already carries the resolved answer; this reads it through the
    /// per-element flag so the derivation does not have to consult the original tree.
    fn is_composite_flag(&self, id: usize) -> bool {
        self.is_composite.get(id).copied().unwrap_or(false)
    }
    /// Whether the element at `id` specializes the library element at `qualified_path` —
    /// `specializesFromLibrary(path)` in the specification's OCL.
    ///
    /// The path is written the way the library writes it (`Occurrences::Occurrence`,
    /// `Actions::Action`), and is resolved by walking `member_of` down from the roots of the loaded
    /// library. A path that does not resolve answers `false`: a library that is not loaded cannot
    /// supply the supertype, and silently answering `true` would make every rule that asks a
    /// positive question pass by default.
    pub fn specializes_from_library(&self, id: usize, qualified_path: &str) -> bool {
        let Some(target) = self.library_element(qualified_path) else {
            return false;
        };
        self.is_subtype(id, target)
    }

    /// A top-level library package by name — the first segment of a `specializesFromLibrary` path.
    ///
    /// Only ids at or after `library_start` are considered, so a project package that happens to
    /// share a name with a library one cannot satisfy the lookup. The roots are the elements the
    /// library was inserted with, which is the same set `top_level_fallback` searches for imports.
    fn library_root_named(&self, name: &str) -> Option<usize> {
        let mut found = (self.library_start..self.len())
            .filter(|id| self.parent[*id].is_none() && self.name[*id] == name)
            .filter(|_| true);
        match (found.next(), found.next()) {
            (Some(id), None) => Some(id),
            _ => None,
        }
    }

    /// The library element a `Package::Sub::Name` path names, or `None` when the library is not
    /// loaded or the path does not resolve.
    ///
    /// Each segment is looked up among the members of the previous one, starting from the library
    /// roots. Ambiguity answers `None` rather than picking one: two library elements of the same
    /// name under the same package would make the question ill-posed, and answering it arbitrarily
    /// would decide rules by iteration order.
    pub fn library_element(&self, qualified_path: &str) -> Option<usize> {
        let mut segments = qualified_path.split("::");
        let first = segments.next()?;
        let mut current = match self.library_root_named(first) {
            Some(id) => id,
            None => return None,
        };
        for segment in segments {
            let found = self.member_of(current, segment);
            current = match found.len() {
                1 => found[0],
                _ => return None,
            };
        }
        Some(current)
    }

    /// Whether the value of the element at `id` was written with `:=` — `FeatureValue::isInitial`.
    pub fn is_initial_of(&self, id: usize) -> bool {
        self.initial.get(id).copied().unwrap_or(false)
    }

    /// Whether the element at `id` wrote its value as a `default` one.
    ///
    /// `validateFeatureValueOverriding` is the caller: its OCL tests `forAll(isDefault)` over the
    /// redefinition closure, so it needs the distinction between `= 42` (not a default) and
    /// `default = 42` (a default). Reading [`Self::is_initial_of`] for that question is wrong in both
    /// directions, because `:=` is a third spelling rather than the negation of `default`.
    pub fn is_default_of(&self, id: usize) -> bool {
        self.defaulted.get(id).copied().unwrap_or(false)
    }

    /// The element that owns the one at `id`, or `None` at a root.
    ///
    /// Read by callers that walk upward without otherwise needing the parallel vectors, such
    /// as the change-impact walk deciding which requirement a touched feature sits inside.
    pub fn parent(&self, id: usize) -> Option<usize> {
        self.parent.get(id).copied().flatten()
    }

    /// Whether the element at `id` states a value at all.
    pub fn has_value_of(&self, id: usize) -> bool {
        self.valued.get(id).copied().unwrap_or(false)
    }

    /// The `:>>` / `redefines` targets of the element at `id`, as authored.
    pub fn redefines_of(&self, id: usize) -> &[String] {
        self.redefines.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The `subsets` targets of the element at `id`, as authored.
    ///
    /// A subsetting is the edge `deriveFeatureType` follows: a feature that subsets another
    /// inherits its types. Distinct from [`supertypes_of_public`](Self::supertypes_of_public),
    /// which unions this with specialization, redefinition and `perform`.
    pub fn subsets_of(&self, id: usize) -> &[String] {
        self.subsets.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The `references` targets of the element at `id`, as authored.
    ///
    /// `ReferenceSubsetting` is a `Subsetting`, so `deriveFeatureType` follows it: `end x
    /// references y` is typed like `y`.
    pub fn references_of(&self, id: usize) -> &[String] {
        self.references.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Which membership introduced the element at `id`.
    pub fn membership_of(&self, id: usize) -> sysml_model::Membership {
        self.membership.get(id).copied().unwrap_or_default()
    }

    /// The authored bound of the element at `id`, when it wrote one.
    ///
    /// `None` means the model wrote no bound, which is not the same as "cannot be read": an absent
    /// bound is the default `[1..1]`, and a rule comparing bounds has to make that distinction.
    pub fn multiplicity_of(&self, id: usize) -> Option<&str> {
        self.multiplicity.get(id).and_then(|m| m.as_deref())
    }

    /// Authored ends in their declaration order.
    pub fn owned_ends_of(&self, id: usize) -> Vec<usize> {
        self.children
            .get(id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&child| {
                self.is_end_of(child) && !self.inherited.get(child).copied().unwrap_or(false)
            })
            .collect()
    }

    /// Effective ends, with redefined inherited identities removed.
    pub fn effective_ends_of(&self, id: usize) -> Vec<usize> {
        let guard = InProgressGuard::enter(Question::EffectiveEnds, id);
        if guard.reentered() {
            return Vec::new();
        }
        let mut out = self.owned_ends_of(id);
        for general in self.supertypes_of_public(id) {
            for end in self.effective_ends_of(general) {
                if !out.contains(&end) {
                    out.push(end);
                }
            }
        }
        // A sibling branch can still expose an ancestor replaced in another branch. Remove
        // identities through the redefinition closure, not a prefix of each branch's positions.
        let mut replaced = std::collections::HashSet::new();
        let mut pending = out.clone();
        while let Some(end) = pending.pop() {
            for target in self.end_redefinition_targets(end) {
                if replaced.insert(target) {
                    pending.push(target);
                }
            }
        }
        out.retain(|end| !replaced.contains(end));
        out
    }

    fn end_redefinition_targets(&self, id: usize) -> Vec<usize> {
        let mut out = self.implicit_redefinition_targets(id);
        for target in self.explicit_end_redefinition_targets(id) {
            if !out.contains(&target) {
                out.push(target);
            }
        }
        out
    }

    fn explicit_end_redefinition_targets(&self, id: usize) -> Vec<usize> {
        if self.redefines_of(id).is_empty() {
            return Vec::new();
        }
        let scope = self.clause_scope_of(id);
        let inherited: Vec<usize> = self
            .supertypes_of_public(scope)
            .into_iter()
            .flat_map(|general| self.effective_ends_of(general))
            .collect();
        let mut out = Vec::new();
        for text in self.redefines_of(id) {
            if let Some(target) = self
                .inherited_member_of(scope, text)
                .or_else(|| self.resolve_excluding(scope, text, id).found())
                .filter(|&target| {
                    target != id && self.is_end_of(target) && inherited.contains(&target)
                })
            {
                if !out.contains(&target) {
                    out.push(target);
                }
            }
        }
        out
    }

    /// Whether `id` is a parameter: a directed feature of a behavior or a step
    /// (`FeatureUtil.isParameter` in the reference implementation).
    pub fn is_parameter(&self, id: usize) -> bool {
        let Some(owner) = self.parent(id) else {
            return false;
        };
        self.direction_of(id).is_some()
            && (self.kind[owner].is_metaclass("Behavior") || self.kind[owner].is_metaclass("Step"))
    }

    /// A type's own parameters, in authored order, its result left out.
    ///
    /// The reference ignores result parameters when it pairs by position (`isIgnoredParameter`,
    /// `FeatureAdapter.java:741-743`): a result redefines the general's result whatever its place,
    /// so counting it would shift every other parameter onto the wrong partner — `function f { in
    /// x; }` would pair `x` with `Evaluation::result`.
    fn owned_parameters_of(&self, owner: usize) -> Vec<usize> {
        self.children[owner]
            .iter()
            .copied()
            .filter(|&child| self.is_parameter(child) && self.clause_of(child) != Some("return"))
            .collect()
    }

    /// The result parameter a type owns or inherits, nearest first.
    fn result_parameter_of(&self, owner: usize, depth: usize) -> Option<usize> {
        let own = self.children[owner]
            .iter()
            .copied()
            .find(|&child| self.is_parameter(child) && self.clause_of(child) == Some("return"));
        if own.is_some() || depth > 16 {
            return own;
        }
        self.supertypes_of_public(owner)
            .into_iter()
            .filter(|&general| general != owner)
            .find_map(|general| self.result_parameter_of(general, depth + 1))
    }

    /// Every parameter a type has, owned first and then what it inherits and does not redefine
    /// (`TypeUtil.getAllParametersOf`, which is `directedFeature`: owned features come before
    /// inherited ones). An owned parameter takes the place of the inherited one at its position
    /// or with its name, which is how positional redefinition leaves the inherited list.
    fn all_parameters_of(&self, owner: usize, depth: usize) -> Vec<usize> {
        let owned = self.owned_parameters_of(owner);
        if depth > 16 {
            return owned;
        }
        let mut out = owned.clone();
        for general in self.supertypes_of_public(owner) {
            if general == owner {
                continue;
            }
            for (position, inherited) in self
                .all_parameters_of(general, depth + 1)
                .into_iter()
                .enumerate()
            {
                let replaced = position < owned.len()
                    || owned.iter().any(|&own| {
                        self.has_authored_name(own) && self.name[own] == self.name[inherited]
                    });
                if !replaced && !out.contains(&inherited) {
                    out.push(inherited);
                }
            }
        }
        out
    }

    /// The parameters of general types that a parameter redefines by position.
    ///
    /// The reference pairs each parameter with the parameter at the same index of every general
    /// type of its owner (`FeatureAdapter.addRedefinitions`, `:634-651`; the list is
    /// `getRelevantParameters`, `:728-733`), and a result parameter with the general's result
    /// (`getParameterRelevantFeatures`, `:712-724`). `behavior B specializes A { out x1; }` thereby
    /// redefines `A::x` — which is what makes its direction conformance a question at all
    /// (`Redefinition_DirectionConformance_invalid.kerml.xt:35`). A parameter that redefines
    /// something explicitly, or repeats an inherited name, is not paired by position.
    pub fn positional_parameter_targets(&self, id: usize) -> Vec<usize> {
        if !self.is_parameter(id) || self.redefines.get(id).is_some_and(|r| !r.is_empty()) {
            return Vec::new();
        }
        let Some(owner) = self.parent(id) else {
            return Vec::new();
        };
        if self.has_authored_name(id) && self.inherited_member_of(owner, &self.name[id]).is_some() {
            return Vec::new();
        }
        let is_result = self.clause_of(id) == Some("return");
        let owned = self.owned_parameters_of(owner);
        let position = owned.iter().position(|&p| p == id);
        if !is_result && position.is_none() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for general in self.supertypes_of_public(owner) {
            if general == owner {
                continue;
            }
            let target = if is_result {
                self.result_parameter_of(general, 0)
            } else {
                position.and_then(|i| self.all_parameters_of(general, 0).get(i).copied())
            };
            if let Some(target) = target {
                if target != id && !out.contains(&target) {
                    out.push(target);
                }
            }
        }
        out
    }

    /// Implicit named or positional redefinition targets for an owned end.
    pub fn implicit_redefinition_targets(&self, id: usize) -> Vec<usize> {
        let guard = InProgressGuard::enter(Question::ImplicitRedefinitions, id);
        if guard.reentered() {
            return Vec::new();
        }
        // A parameter redefines its general's parameter at the same position, which makes it a
        // kind of that parameter and gives it what that parameter has.
        if !self.is_end_of(id) {
            return if id < self.kind.len() && self.direction_of(id).is_some() {
                self.positional_parameter_targets(id)
            } else {
                Vec::new()
            };
        }
        let Some(owner) = self.parent(id) else {
            return Vec::new();
        };
        // An explicit redefinition of another inherited end takes precedence over position. A
        // redefinition of an unrelated feature does not erase the end's positional replacement.
        if !self.explicit_end_redefinition_targets(id).is_empty() {
            return Vec::new();
        }
        let ends = self.owned_ends_of(owner);
        let Some(position) = ends.iter().position(|&end| end == id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for general in self.supertypes_of_public(owner) {
            let inherited = self.effective_ends_of(general);
            let named = inherited
                .iter()
                .copied()
                .find(|&end| self.has_authored_name(id) && self.name[end] == self.name[id]);
            if let Some(target) = named.or_else(|| inherited.get(position).copied()) {
                if target != id && !out.contains(&target) {
                    out.push(target);
                }
            }
        }
        out
    }

    /// Implicit subset targets added by semantic rules, separate from authored clauses.
    pub fn implicit_subsetting_targets(&self, id: usize) -> Vec<usize> {
        // A bare feature that redefines another one takes the base of what it redefines, not the
        // attribute default. `part A { part p[*]; }` with `p1 :>> p;` lowers `p1` as an
        // `AttributeUsage`, and `Base::dataValues` then supplied a second `self` beside the
        // `Parts::Part` the redefinition already brings — 35 "Duplicate of inherited member name
        // 'self'" warnings across `RedefinitionDiamond`, `UsageDefinition_invalid` and the port
        // examples, none of which the pilot states.
        let redefines_something = self
            .redefines
            .get(id)
            .is_some_and(|targets| !targets.is_empty());
        // A `ref x : T` wears the attribute kind but states its own base through `T`: `ref i2 : i`
        // already inherits `Items::Item` through `i`, so `Base::dataValues` added a second `self`
        // (7 warnings across `UsageDefinition_invalid`, `StateUsage_invalid` and
        // `SemanticMetadata_valid`, none of which the pilot states).
        let is_reference = self.modifiers_of(id).is_reference;
        let target = match self.kind.get(id) {
            Some(Kind::AttributeUsage) if !redefines_something && !is_reference => {
                self.attribute_usage_default(id)
            }
            Some(Kind::InterfaceUsage) => {
                let wanted = if self.owned_ends_of(id).len() == 2 {
                    "Interfaces::binaryInterfaces"
                } else {
                    "Interfaces::interfaces"
                };
                self.implicit_library_element(wanted)
            }
            _ => None,
        };
        target.filter(|&target| target != id).into_iter().collect()
    }

    /// Resolve a single implicit library element by its qualified name.
    fn implicit_library_element(&self, wanted: &'static str) -> Option<usize> {
        if let Some(hit) = self.implicit_base_cache.borrow().get(wanted).copied() {
            return hit;
        }
        let hit = self
            .qualified
            .iter()
            .position(|qualified| qualified == wanted);
        self.implicit_base_cache.borrow_mut().insert(wanted, hit);
        hit
    }

    /// The supertypes the element at `id` claims, resolved to ids.
    ///
    /// `supertypes_of` is private because it is the recursive step of the member walk; this is the
    /// same list for a caller that wants to know whether an element inherits anything at all.
    pub fn supertypes_of_public(&self, id: usize) -> Vec<usize> {
        self.supertypes_of(id)
    }

    /// Whether the element at `id` was given a name by the author.
    ///
    /// A lowering path may synthesise a name for something anonymous — an unnamed action is named
    /// for its own text. `name_span` is set only when the model wrote one, so this separates a
    /// declared element from a synthesised one.
    pub fn is_named(&self, id: usize) -> bool {
        self.named.get(id).copied().unwrap_or(false)
    }

    /// Whether the author wrote a name for the element at `id`, whatever its kind.
    pub fn has_authored_name(&self, id: usize) -> bool {
        self.authored_name.get(id).copied().unwrap_or(false)
    }

    /// The feature modifiers of the element at `id`.
    ///
    /// A rule comparing a feature with the one it subsets or redefines has the target's id, not
    /// the element, so the flags it needs have to be reachable from the index.
    pub fn modifiers_of(&self, id: usize) -> Modifiers {
        self.modifiers.get(id).cloned().unwrap_or_default()
    }

    /// First id that belongs to the library; equal to `len()` when none is attached.
    pub fn project_end(&self) -> usize {
        self.library_start
    }

    pub fn is_empty(&self) -> bool {
        self.kind.is_empty()
    }

    /// Members of a scope, by name. Only direct children are considered: SysML v2 does not
    /// flatten a nested namespace into its parent implicitly. Imports and relationship-like
    /// elements are skipped — neither owns the name it is stored under. An import is a
    /// relationship, not a membership, and its stored name is the target path it refers to;
    /// a `satisfy R by x` element carries `R`'s reference text as a display name, which would
    /// otherwise collide with the very requirement it points at and make every same-scope
    /// `satisfy R ...` ambiguous.
    /// The clause keyword that introduced the element at `id`, if one did.
    pub fn clause_of(&self, id: usize) -> Option<&str> {
        self.clause.get(id).and_then(|c| c.as_deref())
    }

    /// The direction written on the element at `id`, if any.
    pub fn direction_of(&self, id: usize) -> Option<sysml_model::Direction> {
        self.direction.get(id).copied().flatten()
    }

    /// The owners the element at `id` conjugates, by id.
    pub fn conjugates_of(&self, id: usize) -> Vec<usize> {
        let Some(texts) = self.conjugate_text.get(id) else {
            return Vec::new();
        };
        texts
            .iter()
            .filter_map(|t| {
                let head = t.split(',').next().unwrap_or(t).trim();
                match self.resolve(id, head) {
                    Resolution::Found(found) => Some(found),
                    _ => None,
                }
            })
            .collect()
    }

    /// The `chains a.b` targets of the element at `id`, as authored.
    pub fn chains_of(&self, id: usize) -> &[String] {
        self.chains.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The `crosses c` targets of the element at `id`, as authored.
    pub fn crosses_of(&self, id: usize) -> &[String] {
        self.crosses.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The scope a clause written on `id` resolves from: its owner, or the element itself when
    /// nothing owns it.
    pub fn clause_scope_of(&self, id: usize) -> usize {
        self.parent.get(id).copied().flatten().unwrap_or(id)
    }

    /// The member a redefinition names, looked up in what its owner inherits.
    ///
    /// `part :>> x` names the `x` its owner inherits, not a member of the redefinition itself and
    /// not an `x` its owner declares — resolving it from either of those scopes finds the
    /// redefinition or a sibling. This walks the supertype closure only, which is the scope the
    /// notation means.
    pub fn inherited_member_of(&self, owner: usize, name: &str) -> Option<usize> {
        match self.inherited_member(owner, name) {
            Some(Resolution::Found(id)) => Some(id),
            _ => None,
        }
    }

    /// The members a scope declares, by name — direct children only.
    ///
    /// Three questions wear this shape, and answering one with another's function has produced the
    /// same defect three times: a name that resolves to something real and wrong, or a legal name
    /// that does not resolve at all. Neither shows up as "unresolved".
    ///
    /// * What does this scope *declare*? — this function. A rule that walks supertypes or imports
    ///   itself wants this one.
    /// * What does this value *provide*? — [`Index::member_of_access`]. A `.` access asks this: the
    ///   container, then its type, then that type's specialization closure, then the imports bound
    ///   in each scope. Reading only direct children reported `d->b` as missing when `b` was
    ///   inherited, and made `new D(b = 1)` treat an inherited `b` as unknown.
    /// * What does this name *mean here*? — [`Index::resolve`] and [`Index::resolve_path`]. A plain
    ///   or qualified name is looked up through the scopes around it, imports included. That is a
    ///   different walk: using it for a `.` step binds an unrelated feature from an enclosing scope
    ///   (`chains a.b` with `b` declared beside the chain), and a one-segment `resolve_path` is
    ///   exactly that trap — a *scope* lookup from an element, not a member lookup of it.
    pub fn member_of(&self, scope: usize, name: &str) -> Vec<usize> {
        self.children[scope]
            .iter()
            .copied()
            .filter(|&c| {
                self.kind[c] != Kind::Import
                    && self.named[c]
                    && (self.name[c] == name || self.short_name[c].as_deref() == Some(name))
            })
            .collect()
    }

    /// Resolve `reference` as written inside `from`.
    ///
    /// A qualified reference (`A::B::C`) is resolved head first: `A` is looked up outward from
    /// `from`, then `B` inside it, and so on. A bare name walks the enclosing scopes. Library
    /// names are reachable only through an import, so the outermost fallback searches project
    /// roots only.
    pub fn resolve(&self, from: usize, reference: &str) -> Resolution {
        self.resolve_inner(from, reference, false)
    }

    /// Resolve an import's target path. Same walk, but a name no scope provides falls back to
    /// top-level elements — project roots first, then library roots.
    fn resolve_target(&self, from: usize, reference: &str) -> Resolution {
        self.resolve_inner(from, reference, true)
    }

    fn resolve_inner(&self, from: usize, reference: &str, library: bool) -> Resolution {
        let reference = reference.trim();
        if reference.is_empty() {
            return Resolution::Unresolved;
        }
        let segments: Vec<&str> = reference.split("::").map(unquote).collect();

        // A qualified name narrows its first segment by the segments after it. When that segment
        // names several things, the rest of the path decides which of them was meant:
        //
        //     classifier A {}
        //     package A { alias A_alias for A; }
        //     package A {}
        //     feature A: A::A_alias;      // one of the three `A`s has `A_alias`
        //
        // The four `A`s are not distinguishable, and `validateNamespaceDistinguishibility` is the
        // rule that says so — it is reported as a warning here, which is what the suite asserts.
        // Resolution is a separate question, and the specification answers it by taking the first
        // match, not by failing: `resolveLocal` and `resolveVisible` both end `memberships->first()`
        // (`generated.rs:9711`, `:9717`), and neither has a branch that turns several candidates
        // into an error. Treating the ambiguity as a resolution failure rejected a model the suite
        // states as clean.
        //
        // So an ambiguous first segment is carried forward as a set and filtered by the next
        // segment, which is the information the reference implementation's ordering gives it for
        // free. Only when the filter cannot decide — no candidate, or several — does the ambiguity
        // survive, and then it is reported, because at that point nothing in the name says which
        // one was meant.
        let mut current = match self.resolve_bare(from, segments[0], library) {
            Resolution::Found(id) => vec![id],
            Resolution::Ambiguous(ids) => ids,
            other => return other,
        };
        for segment in &segments[1..] {
            // A qualified name reaches what the namespace *exports*, not only what it declares.
            // `ISQ::TorqueValue` names a member ISQ imported publicly from ISQMechanics; using
            // `member_of` alone (direct children only) made every such name unresolved, which is
            // most of the standard library's useful surface.
            let narrowed: Vec<usize> = current
                .iter()
                .filter_map(|&namespace| {
                    let matches = self.exports_of(namespace, segment);
                    match matches.len() {
                        0 => None,
                        // One candidate, or the first of several inside one namespace, which is the
                        // specification's `first()`.
                        1 => Some(matches[0]),
                        _ => matches.first().copied(),
                    }
                })
                .collect();
            match narrowed.len() {
                0 => return Resolution::Unresolved,
                1 => current = narrowed,
                _ => return Resolution::Ambiguous(narrowed),
            }
        }
        match current.len() {
            1 => Resolution::Found(current[0]),
            _ => Resolution::Ambiguous(current),
        }
    }

    /// Walk outward from `from`: the element itself, then each enclosing scope. At every
    /// scope, direct members are consulted before imported names, so a local definition
    /// shadows an import and an inner import shadows an outer one.
    ///
    /// `library` decides only the outermost fallback: whether a name no scope provides may
    /// come from the bundled standard library's roots.
    fn resolve_bare(&self, from: usize, name: &str, _library: bool) -> Resolution {
        let mut scope = Some(from);
        while let Some(current) = scope {
            let matches = self.member_of(current, name);
            match matches.len() {
                1 => return Resolution::Found(matches[0]),
                n if n > 1 => return Resolution::Ambiguous(matches),
                _ => {}
            }
            // A scope inherits the members of what it specializes or redefines. `part frontWheel_1
            // subsets frontWheel` is written inside a part that redefines another part, and
            // `frontWheel` is that other part's member.
            if let Some(found) = self.inherited_member(current, name) {
                return found;
            }
            // A performance is written against the feature it performs, so that feature's
            // members are in scope: `perform vehicleMassTest { in part :>> testVehicle }`
            // refines a `testVehicle` belonging to `vehicleMassTest`, not to the part that
            // contains the performance.
            if let Some(performed) = self.performed_feature(current) {
                let matches = self.member_of_for_access(performed, name);
                match matches.len() {
                    1 => return Resolution::Found(matches[0]),
                    n if n > 1 => return Resolution::Ambiguous(matches),
                    _ => {}
                }
            }
            if let Some(binding) = self.binding_in(current, name) {
                return Resolution::Found(binding);
            }
            scope = self.parent[current];
        }
        // Nothing in scope. Root-level imports come next: an import written outside every
        // package is a member of the root namespace, and the root namespace encloses
        // everything.
        if let Some(binding) = self
            .root_bindings
            .iter()
            .rev()
            .find(|b| b.name == name)
            .map(|b| b.target)
        {
            return Resolution::Found(binding);
        }
        // A library package is a top-level namespace too, and KerML 8.2.3.5.2 makes top-level
        // names globally visible. `ScalarValues::Real` written without an import is the same kind
        // of reference as `MyPkg::Thing`: the first segment names a *package*, which is visible
        // everywhere, and only the segments after it need an import. Refusing the head because
        // the package happens to live in the library made every fully-qualified standard-library
        // name unresolved.
        //
        // This exposes package names only. `library` controls the deeper segments, and nothing
        // below falls back to "any element anywhere with this leaf name" — that fallback would
        // resolve a name the model never made visible, and would defeat the visibility an import
        // is written to state: a private import stops being private if a name can be found by
        // simply existing.
        self.top_level_fallback(name, true)
    }

    /// Resolve a name from `scope`, never returning `exclude` itself.
    ///
    /// Used for a supertype clause. `part :>> subsystemA = ...` renames nothing: it redefines a
    /// feature called `subsystemA`. Asking the owner for `subsystemA` finds this declaration
    /// first, so the declaration would come out as its own supertype and inherit nothing.
    ///
    /// A metadata usage has the same shape: `@A { … }` is typed by `A`, and the usage is itself a
    /// member called `A`, so asking its owner for the type answers the usage.
    /// Resolve a type the declaration at `from` writes, as the specification resolves it.
    ///
    /// A feature hides the member it redefines from its own type name. In `calc c2 : C { return r1
    /// : T; }`, `r1` redefines the result `T` of `C`, so the `T` written as `r1`'s type is the
    /// `attribute def T` outside, not the parameter it replaces
    /// (`CalculationUsage_Invalid2.sysml.xt:38-53`). Every other name resolves as [`Index::resolve`]
    /// does.
    pub fn resolve_typing(&self, from: usize, text: &str) -> Resolution {
        let first = self.resolve(from, text);
        let Resolution::Found(target) = first else {
            return first;
        };
        if !self.implicit_redefinition_targets(from).contains(&target) {
            return first;
        }
        self.resolve_excluding(from, text, target)
    }

    pub fn resolve_excluding(&self, from: usize, reference: &str, exclude: usize) -> Resolution {
        let segments: Vec<&str> = reference.split("::").map(unquote).collect();
        let mut current = match self.resolve_bare_excluding(from, segments[0], exclude) {
            Resolution::Found(id) => id,
            other => return other,
        };
        for segment in &segments[1..] {
            let matches = self.exports_of(current, segment);
            current = match matches.len() {
                0 => return Resolution::Unresolved,
                1 => matches[0],
                _ => return Resolution::Ambiguous(matches),
            };
        }
        Resolution::Found(current)
    }

    /// The scope walk, skipping one element.
    fn resolve_bare_excluding(&self, from: usize, name: &str, exclude: usize) -> Resolution {
        let mut scope = Some(from);
        while let Some(current) = scope {
            let matches: Vec<usize> = self
                .member_of(current, name)
                .into_iter()
                .filter(|&id| id != exclude)
                .collect();
            match matches.len() {
                1 => return Resolution::Found(matches[0]),
                n if n > 1 => return Resolution::Ambiguous(matches),
                _ => {}
            }
            if let Some(inherited) = self
                .inherited_member(current, name)
                .filter(|r| r.found() != Some(exclude))
            {
                return inherited;
            }
            // A performance is written against the feature it performs, so that feature's members
            // are in scope — `resolve_bare` says so and this walk did not. Measured consequence:
            // `perform vehicleMassTest { in part :>> testVehicle = vehicleUnderTest; }` could not
            // see the `testVehicle` that `vehicleMassTest` declares, while a plain reference of the
            // same name in the same body resolved (`9-Verification-simplified.sysml:80`).
            if let Some(performed) = self.performed_feature(current) {
                let matches: Vec<usize> = self
                    .member_of_for_access(performed, name)
                    .into_iter()
                    .filter(|&id| id != exclude)
                    .collect();
                match matches.len() {
                    1 => return Resolution::Found(matches[0]),
                    n if n > 1 => return Resolution::Ambiguous(matches),
                    _ => {}
                }
            }
            if let Some(binding) = self.binding_in(current, name) {
                return Resolution::Found(binding);
            }
            scope = self.parent[current];
        }
        if let Some(binding) = self
            .root_bindings
            .iter()
            .rev()
            .find(|b| b.name == name)
            .map(|b| b.target)
        {
            return Resolution::Found(binding);
        }
        self.top_level_fallback(name, true)
    }

    /// The scope walk, skipping several elements.
    ///
    /// Needed for a nested performance whose name repeats an outer one: the walk must pass
    /// through both the container and its same-named ancestors to reach the feature performed.
    fn resolve_bare_skipping(&self, from: usize, name: &str, skip: &[usize]) -> Resolution {
        let mut scope = Some(from);
        while let Some(current) = scope {
            let matches: Vec<usize> = self
                .member_of(current, name)
                .into_iter()
                .filter(|id| !skip.contains(id))
                .collect();
            match matches.len() {
                1 => return Resolution::Found(matches[0]),
                n if n > 1 => return Resolution::Ambiguous(matches),
                _ => {}
            }
            if let Some(inherited) = self
                .inherited_member(current, name)
                .filter(|r| !skip.contains(&r.found().unwrap_or(usize::MAX)))
            {
                return inherited;
            }
            if let Some(binding) = self.binding_in(current, name) {
                return Resolution::Found(binding);
            }
            scope = self.parent[current];
        }
        if let Some(binding) = self
            .root_bindings
            .iter()
            .rev()
            .find(|b| b.name == name)
            .map(|b| b.target)
        {
            return Resolution::Found(binding);
        }
        self.top_level_fallback(name, true)
    }
    ///
    /// Walks the supertype closure breadth first, nearest declaration winning. A name absent from
    /// every ancestor is `None` so the caller can continue outward through the lexical scopes.
    fn inherited_member(&self, scope: usize, name: &str) -> Option<Resolution> {
        let mut frontier: Vec<usize> = self.supertypes_of(scope);
        // A conjugated type provides what it conjugates. `class C1 conjugates C` has C's features
        // with every direction turned around — the turn itself belongs to the direction rule — so
        // `C1` is not the empty type the specialization walk alone makes it. Without this edge a
        // redefinition inherited through a conjugation was skipped rather than judged
        // (`Redefinition_DirectionConformance_invalid.kerml.xt:52`).
        for conjugated in self.conjugate_edges(scope) {
            if !frontier.contains(&conjugated) {
                frontier.push(conjugated);
            }
        }
        if frontier.is_empty() {
            return None;
        }
        let mut seen: std::collections::HashSet<usize> = frontier.iter().copied().collect();
        while !frontier.is_empty() {
            let mut found: Vec<usize> = Vec::new();
            for &sup in &frontier {
                for id in self.member_of(sup, name) {
                    if !found.contains(&id) {
                        found.push(id);
                    }
                }
            }
            match found.len() {
                0 => {}
                1 => return Some(Resolution::Found(found[0])),
                _ => return Some(Resolution::Ambiguous(found)),
            }
            let mut next = Vec::new();
            for &sup in &frontier {
                for anc in self.supertypes_of(sup) {
                    if seen.insert(anc) {
                        next.push(anc);
                    }
                }
                for conjugated in self.conjugate_edges(sup) {
                    if seen.insert(conjugated) {
                        next.push(conjugated);
                    }
                }
            }
            frontier = next;
        }
        None
    }

    /// The types the element at `id` conjugates, as edges of the inheritance walk.
    ///
    /// Public because a rule that walks the inheritance chain has to know which edges turn the
    /// directions around: `class C1 conjugates C` gives `C1` the members of `C` with every
    /// direction flipped, and a redefinition two steps down from `C1` must match the flipped one.
    ///
    /// `conjugates_of` resolves the written name, and resolution asks [`Index::inherited_member`]
    /// in turn, which is where this edge is added — so the walk is re-entrancy guarded. On a second
    /// entry the edge is left out instead of asking again, which terminates the cycle at the cost of
    /// one answer computed without conjugates. That only matters for a conjugation whose target is
    /// itself reachable only through inheritance, which no model in the corpus writes.
    pub fn conjugate_edges(&self, id: usize) -> Vec<usize> {
        let guard = InProgressGuard::enter(Question::Conjugates, id);
        if guard.reentered() {
            return Vec::new();
        }
        self.conjugates_of(id)
    }

    /// What `scope` exports under `name`: its own members first, then what it imported publicly.
    ///
    /// Imports are namespace members in KerML, so `ISQ::LengthValue` resolves to the element
    /// `ISQMechanics` declares and `ISQ` re-exports. Private imports are not exported — that is
    /// the whole point of the visibility prefix — so they are skipped here even though the
    /// declaring scope can still see them.
    fn exports_of(&self, scope: usize, name: &str) -> Vec<usize> {
        let direct = self.member_of(scope, name);
        if !direct.is_empty() {
            return direct;
        }
        // A qualified name reaches what the namespace inherits as well as what it declares:
        // `vehicleConfigA::subsystemA::subsystem1` names the `subsystem1` of the `subsystemA` that
        // `vehicleConfigA` inherits from its supertype, not one it writes itself.
        if let Some(inherited) = self.inherited_member(scope, name) {
            let ids: Vec<usize> = match inherited {
                Resolution::Found(id) => vec![id],
                Resolution::Ambiguous(ids) => ids,
                // `inherited_member` answers only Found or Ambiguous; the other two are not
                // reachable here, and an inherited name is never a notation error.
                Resolution::Unresolved | Resolution::NotAccessible => Vec::new(),
            };
            if !ids.is_empty() {
                return ids;
            }
        }
        self.scope_bindings[scope]
            .iter()
            .filter(|b| b.name == name && b.visibility == Some(Visibility::Public))
            .map(|b| b.target)
            .fold(Vec::new(), |mut acc, id| {
                if !acc.contains(&id) {
                    acc.push(id);
                }
                acc
            })
    }

    fn binding_in(&self, scope: usize, name: &str) -> Option<usize> {
        self.scope_bindings[scope]
            .iter()
            .rev()
            .find(|b| b.name == name)
            .map(|b| b.target)
    }

    /// Top-level elements are globally visible (KerML 8.2.3.5.2), which is what lets
    /// `A::Thing` cross packages without an import. `library` extends the search to the
    /// bundled standard library's roots and is used for import targets only. Relationship-like
    /// elements are skipped for the same reason as in [`Index::member_of`]: their stored name
    /// is a reference text, not a declared name.
    fn top_level_fallback(&self, name: &str, library: bool) -> Resolution {
        let roots = |range: std::ops::Range<usize>| {
            range
                .filter(|&i| {
                    self.parent[i].is_none()
                        // An import element is stored under the text of its target, so a root-level
                        // `import Views::*` is a top-level element *named* `Views`. Without this
                        // the name resolves to the import statement itself: measured,
                        // `11b-Safety and Security Feature Views.sysml` wrote
                        // `private import Views::*;` before its package, `Views` answered
                        // `Found(the import)`, nothing was bound, and `asTreeDiagram` and
                        // `asElementTable` came back unresolved. `member_of` already skips imports
                        // for the same reason.
                        && self.kind[i] != Kind::Import
                        && self.named[i]
                        && (self.name[i] == name || self.short_name[i].as_deref() == Some(name))
                })
                .collect::<Vec<_>>()
        };
        let matches = roots(0..self.library_start);
        if !matches.is_empty() || !library {
            return match matches.len() {
                1 => Resolution::Found(matches[0]),
                0 => Resolution::Unresolved,
                _ => Resolution::Ambiguous(matches),
            };
        }
        let matches = roots(self.library_start..self.len());
        match matches.len() {
            1 => Resolution::Found(matches[0]),
            0 => Resolution::Unresolved,
            _ => Resolution::Ambiguous(matches),
        }
    }

    /// Resolve a name as written inside an expression, where the separator may be `.` as well
    /// as `::`.
    ///
    /// `element.member.path` is the shape a predicate actually writes, and the parser keeps it
    /// as one reference, so the separator has to be told apart here: `::` walks enclosing
    /// scopes, `.` walks into a resolved element's members. Both end at the last segment, and
    /// any segment failing makes the whole path unresolved — a partially resolvable path is
    /// still a broken path, and reporting it as resolved would hide exactly the typo this
    /// check exists to find.
    pub fn resolve_path(&self, from: usize, name: &NameRef) -> Resolution {
        let text = name.text.trim();
        if text.is_empty() || text == "*" {
            return Resolution::Unresolved;
        }
        // Fast path: no dots, so it is the ordinary qualified-name walk.
        if !text.contains('.') {
            return self.resolve(from, text);
        }

        let segments: Vec<&str> = text
            .split('.')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if segments.is_empty() {
            return Resolution::Unresolved;
        }

        let mut current = match self.resolve(from, segments[0]) {
            Resolution::Found(id) => id,
            other => return other,
        };
        for segment in &segments[1..] {
            // A `private` or `protected` member is not a member the rest of the model can name:
            // `assign i.b := null;` with `protected attribute b;` inside `item i` is "Couldn't
            // resolve reference to Element 'b'" (`AssignmentActionUsage_invalid.sysml.xt:41`).
            // The filter is applied to the dotted walk alone — a qualified name (`i::b`) is a
            // different question, and the library reaches its own members by `::`.
            let matches: Vec<usize> = self
                .member_of_for_access(current, segment)
                .into_iter()
                .filter(|&member| self.is_visible_from(from, member, current))
                .collect();
            current = match matches.len() {
                0 => return Resolution::Unresolved,
                1 => matches[0],
                _ => return Resolution::Ambiguous(matches),
            };
        }
        Resolution::Found(current)
    }

    /// Whether `member`, declared by `owner`, is nameable from `from`.
    ///
    /// `private` and `protected` both mean "not outside the type that declares it"; the
    /// difference between them is inheritance, which this projection does not need to decide the
    /// question the reference asks. A member with no stated visibility is public.
    fn is_visible_from(&self, from: usize, member: usize, owner: usize) -> bool {
        if !matches!(
            self.visibility.get(member).copied().flatten(),
            Some(Visibility::Private) | Some(Visibility::Protected)
        ) {
            return true;
        }
        // Inside the owner, everything it declares is nameable.
        let mut at = Some(from);
        for _ in 0..64 {
            let Some(id) = at else { break };
            if id == owner {
                return true;
            }
            at = self.parent.get(id).copied().flatten();
        }
        false
    }

    /// Whether an end written with `::` names something an end cannot denote.
    ///
    /// An end names a feature of an instance: `y.z` walks instances, `y::z` names declarations. So
    /// a `::` step that passes through a definition has left the instance, and one that lands on a
    /// part has landed on one too. The three shapes the pilot reports are `connect A::x` with
    /// `part def A { part x; }`, `connect y::z` with `part def B { part y { part z; } }`, and
    /// `flow … from B::myOut …` with `part def B { out ref myOut; }`
    /// (`Connector_Invalid.sysml.xt:45`, `Relationship_invalid_relatedElement1.sysml.xt:54`).
    ///
    /// An instance-level path is not this question: `Vehicle_2::wheelAssy` in a value names a part
    /// of a package-level part, and the corpus writes it (`15_05-Unification …`).
    pub fn end_is_unreachable(&self, from: usize, text: &str) -> bool {
        let segments: Vec<&str> = text
            .split("::")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if segments.len() < 2 {
            return false;
        }
        let mut current = match self.resolve_bare(from, segments[0], false) {
            Resolution::Found(id) => id,
            _ => return false,
        };
        for (step, segment) in segments[1..].iter().enumerate() {
            // `B0::p0.myDefIn` mixes the operators: the `::` step lands on `p0`, and the `.` tail is
            // a further access. Only the head is this step's member.
            let head = segment.split('.').next().unwrap_or(segment).trim();
            if self.kind[current].is_definition() {
                return true;
            }
            let members = self.member_of_for_access(current, head);
            if members.is_empty() {
                return false;
            }
            if step + 2 == segments.len()
                // A package is a namespace: `P::a` names a part by its qualified name, which the
                // pilot writes and accepts (`valid_Connector3.sysml.xt:47`).
                && !matches!(self.kind[current], Kind::Package)
                && matches!(
                    self.kind[members[0]],
                    Kind::PartUsage | Kind::PortUsage | Kind::ItemUsage | Kind::OccurrenceUsage
                )
            {
                return true;
            }
            current = members[0];
        }
        false
    }

    /// Whether a `::` path reaches its target through a feature it cannot see into.
    ///
    /// `::` reaches what belongs to a *declaration*; a part belongs to an *instance* and is
    /// reached with `.` — which is what "use dot notation for nesting" says. So a path that steps
    /// through a feature and lands on a part is the error, while one that lands on an attribute, a
    /// parameter, a multiplicity or a variant is not.
    ///
    /// Seven shapes fix the boundary, because the clause it belongs to
    /// (`validateSubsettingFeaturingTypes`, `canAccess`) is reported unsound by the reference
    /// implementation itself and says nothing about references. Errors: `f::a` (a part of `f`'s
    /// type) and `v1::n` (`feature n : Integer` declared in the type, not in `v1`). Accepted:
    /// `K::f::m` (a multiplicity), `subsystemA::subsystem2` (a variant) and
    /// `fuelCmdPort::fuelCmd` (an attribute, `4a-Functional Allocation`).
    ///
    /// A path whose first segment does not resolve is not this question — it is reported as
    /// unresolved, and saying both would give one text two answers. Neither is a path through a
    /// definition: `connect A::x to …` with `part def A { part x; }` is an error in the pilot
    /// (`Connector_Invalid.sysml.xt:45`) and stays unreported here, because judging every
    /// definition-qualified part rejected twenty-six ends in the official corpus.
    pub fn crosses_a_feature(&self, from: usize, text: &str) -> bool {
        let segments: Vec<&str> = text
            .split("::")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if segments.len() < 2 {
            return false;
        }
        // A dotted prefix before the first `::` is an instance chain: `v2.m::mm` is `v2`'s `m`,
        // then `mm` of that instance, which no declaration scope can name. Splitting on `::` alone
        // left `v2.m` as a "name" that never resolved, so the form was accepted
        // (`FeatureChain_invalid.kerml.xt:59`).
        if let Some(first) = segments.first() {
            if first.contains('.') {
                let name = sysml_model::NameRef::new(*first, 0);
                if let Resolution::Found(id) = self.resolve_path(from, &name) {
                    if self.kind[id].is_feature() {
                        return true;
                    }
                }
            }
        }
        let mut current = match self.resolve_bare(from, segments[0], false) {
            Resolution::Found(id) => id,
            _ => return false,
        };
        for segment in &segments[1..] {
            if self.kind[current].is_feature() && !self.reaches_a_declaration(current, segment) {
                return true;
            }
            let matches = self.exports_of(current, segment);
            current = match matches.len() {
                1 => matches[0],
                _ => return false,
            };
        }
        false
    }

    /// Whether `::` may name `name` inside the feature `feature`.
    ///
    /// The member has to be one the feature declares, or one the feature it redefines declares,
    /// and it has to be a kind that belongs to the declaration rather than to the instance.
    fn reaches_a_declaration(&self, feature: usize, name: &str) -> bool {
        let reachable = |id: usize| {
            // A `variant part b` keeps `Kind::PartUsage` and records the membership in
            // `is_variant`; either form is a declaration a `::` path may name.
            self.is_variant[id]
                || matches!(
                    self.kind[id],
                    Kind::AttributeUsage
                        | Kind::Parameter
                        | Kind::MultiplicityDef
                        | Kind::Variant
                        // `in ref myDefIn redefines B1::p1::myDefIn;` names a member written as a
                        // reference usage. Leaving the kind out made five legitimate redefinitions in
                        // `BindingConnector_redefine.sysml.xt` (:23, :24, :29, :30, :65) report
                        // "not accessible", while the same file's `B0::p0.myDefIn` — `::` followed by a
                        // `.` — is what the official assertions actually reject. The distinction the
                        // spec draws is the dot, not the kind of member the path names.
                        | Kind::DefaultReference
                )
        };
        if self
            .member_of(feature, name)
            .iter()
            .any(|&id| reachable(id))
        {
            return true;
        }
        // A feature's members are not all declared in its body: `port p1 : PortDefIn;` has no
        // `myDefIn` of its own, that member comes from the port definition it is typed by. A path
        // segment that names one of those reaches a declaration just as surely, which is why
        // `in ref myDefIn redefines B1::p1::myDefIn;` is written in `BindingConnector_redefine` and
        // carries no assertion — the five warnings that file does assert are all on `B0::p0.myDefIn`,
        // the spelling that mixes `::` with a dot.
        //
        // Asking the type rather than accepting every name keeps the rule's teeth: `part p : C;`
        // still does not make `p::anything` legal unless `C` actually declares it.
        for target in self.supertypes_of(feature) {
            if self.member_of(target, name).iter().any(|&id| reachable(id)) {
                return true;
            }
        }
        self.redefinition_declares(feature, name)
    }

    /// Whether a feature the element redefines declares the name.
    ///
    /// `part :>> subsystemA = subsystemA::subsystem1;` writes no body of its own; the members the
    /// notation means are the ones on the `subsystemA` it redefines. Only elements with a written
    /// specialization clause are followed, so a plain typed usage — `part f : F` — never lends its
    /// type's members, which is exactly the case `::` must not accept.
    fn redefinition_declares(&self, id: usize, name: &str) -> bool {
        if self.supertypes[id].is_empty() {
            return false;
        }
        let reachable = |id: usize| {
            self.is_variant[id]
                || matches!(
                    self.kind[id],
                    Kind::AttributeUsage | Kind::Parameter | Kind::MultiplicityDef | Kind::Variant
                )
        };
        let mut seen = std::collections::HashSet::new();
        let mut frontier: Vec<usize> = vec![id];
        while let Some(current) = frontier.pop() {
            for target in self.supertypes_of(current) {
                if !seen.insert(target) {
                    continue;
                }
                if self.member_of(target, name).iter().any(|&id| reachable(id)) {
                    return true;
                }
                if !self.supertypes[target].is_empty() {
                    frontier.push(target);
                }
            }
        }
        false
    }

    /// The member a `.` access names, as a resolution.
    ///
    /// [`Index::resolve_path`] answers a different question for a one-segment name: it resolves
    /// that name *in the scope of* the starting point, so `b` under an `a` that does not declare it
    /// walks outward and can find a `b` that has nothing to do with `a`. A `.` access means "the
    /// member of what this denotes", which is what this answers — the same walk `resolve_path`
    /// uses for the segments after the first of a dotted path.
    pub fn member_of_access(&self, container: usize, name: &str) -> Resolution {
        let found = self.member_of_for_access(container, name);
        match found.len() {
            0 => Resolution::Unresolved,
            1 => Resolution::Found(found[0]),
            _ => Resolution::Ambiguous(found),
        }
    }

    /// Members reachable through a `.` access.
    ///
    /// Differs from [`Index::member_of`] in the way that matters for predicates: a part usage
    /// written `part d : Drone;` has no children of its own, so `d.endurance` finds nothing even
    /// though `Drone` declares it. The walk therefore follows the type, and follows that type's
    /// specialization closure as well — `part def D :> Base` inherits `Base`'s features, which is
    /// the whole point of specialization. Stopping at one level made `d.m` unresolved for a
    /// feature declared on a supertype, so a correctly written requirement reported SEM004.
    ///
    /// Names are searched per level, not depth-first: the nearer declaration wins, and a feature
    /// redeclared in a subtype is not ambiguous with the one it redefines. Within one level a
    /// name reaching two different parents is genuinely ambiguous and is reported as such.
    fn member_of_for_access(&self, container: usize, name: &str) -> Vec<usize> {
        let direct = self.member_of(container, name);
        if !direct.is_empty() {
            return direct;
        }
        // An import written in this scope makes its target a member of it, so a `.` access reaches
        // what the scope imports: `public import y::y1` inside `x` is what makes `x.y1` a name.
        // Without this the `.` walk was narrower than the `::` walk, which consults `exports_of`.
        if let Some(binding) = self.binding_in(container, name) {
            return vec![binding];
        }
        // Level by level through the specialization closure of the container's type and of the
        // container itself. Both starting points matter: `part leftWheel :> rearWheel` names its
        // parent through specialization and has no type clause at all, while
        // `part :>> part5` redefines a feature declared outside it and carries no typing either.
        // Following only the type made every such feature's own members unreachable.
        let mut frontier: Vec<usize> = Vec::new();
        for start in self
            .typing_of(container)
            .into_iter()
            .chain(self.supertypes_of(container))
            .chain(self.performed_feature(container))
        {
            if !frontier.contains(&start) {
                frontier.push(start);
            }
        }
        let mut seen: std::collections::HashSet<usize> = frontier.iter().copied().collect();
        while !frontier.is_empty() {
            let mut found: Vec<usize> = Vec::new();
            for &type_id in &frontier {
                for id in self.member_of(type_id, name) {
                    if !found.contains(&id) {
                        found.push(id);
                    }
                }
            }
            if !found.is_empty() {
                return found;
            }
            let mut next: Vec<usize> = Vec::new();
            for &type_id in &frontier {
                for sup in self.supertypes_of(type_id) {
                    if seen.insert(sup) {
                        next.push(sup);
                    }
                }
            }
            frontier = next;
        }
        Vec::new()
    }

    /// The feature a `perform` / `exhibit` statement performs, resolved from the statement's
    /// own scope.
    ///
    /// A performance is written against the feature it performs, not against its owner:
    /// `perform vehicleMassTest { in part :>> testVehicle }` refines `testVehicle` on
    /// `vehicleMassTest`, and that name is not a member of `massVerificationSystem`.
    /// The lookup deliberately skips inherited members: a performance naming `X` must not find
    /// its own override of `X` through its owner's type.
    fn performed_feature(&self, container: usize) -> Option<usize> {
        let text = self.performs[container].as_ref()?;
        // Answering this walks the scopes, and that walk asks `member_of_for_access`, which
        // asks this again for the same container: `perform transportPassenger.b.drive` is
        // lowered onto a host `b` whose own clause names the path through `b`. Marked the same
        // way as a supertype clause, so the second entry answers nothing instead of looping.
        let _in_progress = InProgressGuard::enter(Question::Performs, container);
        if _in_progress.reentered() {
            return None;
        }
        let scope = self.parent[container].unwrap_or(container);
        let segments: Vec<&str> = text
            .split('.')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let head = *segments.first()?;
        // The performance is usually named after what it performs, so a plain walk from its
        // owner finds the performance itself and stops. `perform vehicleMassTest.collectData`
        // nests a same-named performance inside one, so both the container and its same-named
        // ancestors have to be skipped to reach the feature being performed.
        let mut skipped: Vec<usize> = vec![container];
        let mut walker = self.parent[container];
        while let Some(node) = walker {
            if self.name[node] == head {
                skipped.push(node);
            }
            walker = self.parent[node];
        }
        let mut current = match self.resolve_bare_skipping(scope, head, &skipped) {
            Resolution::Found(id) => id,
            _ => return None,
        };
        for segment in &segments[1..] {
            let matches = self.member_of_for_access(current, segment);
            current = match matches.len() {
                1 => matches[0],
                _ => return None,
            };
        }
        if current == container {
            return None;
        }
        Some(current)
    }

    /// Whether `id` is `target` or inherits from it, by the closure of its supertypes.
    ///
    /// Used by rules that compare two features' owners: a redefinition may only exist where the
    /// redefining feature's owner specializes the redefined feature's owner, which is exactly
    /// this question.
    pub fn is_subtype(&self, id: usize, target: usize) -> bool {
        // Fast path: the closure of `id` has already been computed, so this is one lookup.
        if let Some(closure) = self.supertype_cache.borrow().get(&id) {
            return closure.contains(&target);
        }
        // Visited set, not just the depth guard. The library states cycles the projection turns
        // into real edges — `Parts::Part` and `Ports::Port` reach each other through their
        // specializations of `Object` — so an unguarded walk recurses until the stack dies. The
        // depth guard alone is not enough either: it bounds the depth, and a cycle at depth 1
        // reaches the bound only after the stack has already grown twice per step.
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut frontier = vec![id];
        let mut closure: Vec<usize> = Vec::new();
        let mut found = false;
        while let Some(current) = frontier.pop() {
            if current == target {
                found = true;
            }
            if !seen.insert(current) {
                continue;
            }
            closure.push(current);
            frontier.extend(self.supertypes_of(current));
        }
        // Record the whole closure, not just the answer: the rule pass asks about many targets for
        // the same id (`part p : D` is checked against its declared type, its owner's type, and the
        // library roots), and each of those was a separate O(depth) walk.
        // A nested semantic query may have cut an edge at an in-progress guard. Its closure
        // is useful to the caller, but cannot become the answer for a later independent query.
        if IN_PROGRESS.with(|set| set.borrow().is_empty()) {
            self.supertype_cache.borrow_mut().insert(id, closure);
        }
        found
    }

    /// The featuring types of a feature — `Feature::featuringType`, resolved to ids.
    ///
    /// The reference derives it from the `featuring … by` statements about the feature and from
    /// the implicit featuring types (`Feature_featuringType_SettingDelegate`). Here the explicit
    /// part is [`Index::explicit_featuring_types`]. The implicit part is the owning type of a
    /// feature that has one, and, for a multiplicity, the featuring types of the feature that owns
    /// it (`FeatureAdapter.addImplicitFeaturingTypesIfNecessary`, `MultiplicityAdapter`). A
    /// multiplicity owned by a type takes no owning-type featuring, which is why
    /// `validateClassifierMultiplicityDomain` can require its featuring types to be empty.
    ///
    /// A feature owned by a package has no featuring type: a package is a `Namespace` and not a
    /// `Type`, so `owningType` is null. That is the case
    /// `validateUsageIsReferential` reports on — a composite feature nothing features.
    pub fn featuring_types_of(&self, id: usize) -> Vec<usize> {
        let mut out = self.explicit_featuring_types(id);
        let owner = self.parent.get(id).copied().flatten();
        match owner {
            Some(owner) if self.kind[id] == Kind::MultiplicityDef => {
                if self.kind[owner].is_feature() {
                    for featuring in self.featuring_types_of(owner) {
                        if !out.contains(&featuring) {
                            out.push(featuring);
                        }
                    }
                }
            }
            Some(owner) if self.kind[owner].is_type() => {
                if !out.contains(&owner) {
                    out.push(owner);
                }
            }
            _ => {}
        }
        out
    }

    /// The featuring types a `featuring of M by X;` statement gives `id`: the feature each such
    /// statement names with `by`, for every statement whose `of` resolves to `id`.
    ///
    /// The `of` end is the `to` end and the `by` end is the `from` end, the way a satisfaction
    /// keeps its requirement and its satisfier (`lower.rs`, `Kind::TypeFeaturing`).
    pub fn explicit_featuring_types(&self, id: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for &statement in &self.type_featurings {
            let (Some(of), Some(by)) = (
                self.end_text(statement, false),
                self.end_text(statement, true),
            ) else {
                continue;
            };
            let scope = self.clause_scope_of(statement);
            let Resolution::Found(featured) = self.resolve(scope, of) else {
                continue;
            };
            if featured != id {
                continue;
            }
            if let Resolution::Found(featuring) = self.resolve(scope, by) {
                if !out.contains(&featuring) {
                    out.push(featuring);
                }
            }
        }
        out
    }

    /// Whether `feature` is a featuring type of `id` — the membership test the OCL writes as
    /// `redefiningFeature.featuringType->includes(featuring)`.
    #[allow(dead_code)]
    pub fn features(&self, id: usize, feature: usize) -> bool {
        self.featuring_types_of(id).contains(&feature)
    }

    /// Whether `ancestor` is `id` or one of the elements that own it.
    ///
    /// A clause that names a path through the element itself (`perform b.a` written on the host
    /// that `b.a` lowers to) resolves to the element or to its owner, and neither is what the
    /// clause meant.
    fn is_ancestor_of(&self, ancestor: usize, id: usize) -> bool {
        let mut current = Some(id);
        while let Some(at) = current {
            if at == ancestor {
                return true;
            }
            current = self.parent[at];
        }
        false
    }

    /// The supertypes a definition or usage claims, resolved to ids.
    ///
    /// Kept separate from [`Index::typing_of`] because the two are different relations: a
    /// definition's `type_ref` is its supertype, while a usage's is its type. Both feed the same
    /// member walk, so both are read here under one name.
    /// Compute every element's direct supertypes once, so the rule pass does not repeat the work.
    ///
    /// This is the one place a cache of [`Self::supertypes_of`] is safe. That function runs *during*
    /// resolution — `resolve_path` asks what a supertype clause names, and the answer depends on how
    /// much of the model has been resolved so far — so an entry recorded mid-build holds a partial
    /// answer. Caching inside the function dropped the pilot score 350→342 with 48 spurious
    /// diagnostics, which is what a stale entry looks like: a supertype silently missing.
    ///
    /// So the cache is filled here, in one pass, and `supertypes_of` only *reads* it once this has
    /// run. Before that the flag is false and every call computes the answer, exactly as it did
    /// before the cache existed.
    ///
    /// The counters say why it is worth doing: on 400 chained definitions `supertypes_of` is entered
    /// 2,415,400 times while `is_subtype` is entered 9,600 times, so the rule modules call it directly
    /// once per element per module rather than going through the closure walk.
    pub fn enable_supertype_cache(&self) {
        // Clear first, then enable. `supertypes_of` is entered while the index is still being built
        // — `with_library` resolves as it inserts — and an answer recorded then is an answer about a
        // half-bound scope. Caching those scored 342/387 with 48 spurious diagnostics, which is why
        // this switch exists at all.
        //
        // What it does *not* do any more is compute every id up front. That is O(index) even when
        // the rules ask about a handful of ids, and the index always carries the standard library:
        // measured, `check` on a five-line model took 3,441 ms against 97 ms for `outline`, which
        // builds the same index without running the rules. The rules ask on demand, so the cache
        // fills on demand.
        self.supertype_edges_cache.borrow_mut().clear();
        self.implicit_base_cache.borrow_mut().clear();
        self.supertype_edges_ready.set(true);
    }

    fn supertypes_of(&self, id: usize) -> Vec<usize> {
        // The cache is checked first, before the `written` clone below. Measured with counters: with
        // the check after the clone, `supertypes_of` was still entered 1,996,977 times on a 400-element
        // chain even though 1,163,232 of those returned a cached answer — the entry cost was paid
        // every time, and the `written` vector was built from `type_list`/`type_text` before being
        // discarded.
        if self.supertype_edges_ready.get() {
            if let Some(hit) = self.supertype_edges_cache.borrow().get(&id) {
                return hit.clone();
            }
        }
        // A definition carries its clause in `supertypes`; a usage carries its type in `type_list`.
        // Both are followed, and they are *unioned* rather than one chosen over the other.
        //
        // Choosing `supertypes` whenever it was non-empty lost the type clause of any element that
        // also wrote a redefinition: `attribute :>> transformation : CoordinateFramePlacement { :>>
        // origin = …; }` then had `CoordinateFrame::transformation` as its only supertype, so a
        // member lookup from inside its body never reached `CoordinateFramePlacement`, where `origin`
        // is declared. Measured on the official corpus: `15_13-Discretely Sampled Function Value`
        // reported `origin` and `basisDirections` unresolved, and `sysml resolve` printed
        // `origin : origin -> unresolved`. A usage is a subtype of its type *and* of what it
        // redefines, so both edges belong here.
        let mut written: Vec<String> = self.supertypes[id].clone();
        // Every type, not only the first: `part p7 : PartDef, AttDef` inherits from both.
        for text in self.type_list.get(id).map(Vec::as_slice).unwrap_or(&[]) {
            if !written.contains(text) {
                written.push(text.clone());
            }
        }
        if written.is_empty() {
            if let Some(text) = &self.type_text[id] {
                written.push(text.clone());
            }
        }
        // Resolved from the *owner's* scope, not from the element itself.
        //
        // A supertype clause names something the owner can see. Resolving it from the element
        // would make the element's own name shadow the supertype it redefines
        // (`part frontWheel_1 subsets frontWheel` names the owner's `frontWheel`, not itself),
        // and resolving it through the inheritance-aware walk would recurse: asking "what do I
        // specialize" would consult "what does my supertype provide", which asks again.
        // The owner's scope, skipping the element itself: `part :>> subsystemA` redefines the
        // `subsystemA` its owner inherits, and resolving the clause from the owner would find
        // this very element first.
        let scope = self.parent[id].unwrap_or(id);
        // Ask "what do I specialize" from the owner's scope, but not while already answering
        // that question: a chain of redefinitions each naming the last would otherwise ask
        // forever. Once the depth is exhausted the clause is reported unresolved, which is a
        // diagnostic rather than a crash.
        let _in_progress = InProgressGuard::enter(Question::Supertypes, id);
        if _in_progress.reentered() {
            return Vec::new();
        }
        let _guard = DepthGuard::enter();
        if _guard.exhausted() {
            return Vec::new();
        }
        let redefs = self.redefines[id].clone();
        let mut out: Vec<usize> = written
            .iter()
            .filter_map(|one| {
                // A clause written as a dotted path (`perform vehicleMassTest.collectData`)
                // names a feature through its owner, which the `::` walk cannot reach.
                if one.contains('.') {
                    let name = NameRef::new(one, self.line[id]);
                    // `perform b.a` lowers to a host element called `a` under `b`, and its clause
                    // names `b.a` — the very path it sits on. Resolving from the owner finds the
                    // host's own scope first (`outer::b` has an `a`), so the answer is the element
                    // itself and the clause is dropped as a self-reference. Each enclosing scope is
                    // tried in turn; the first answer that is neither this element nor one of its
                    // ancestors is the feature the clause names, which is `P::B::a` here.
                    let mut scope = Some(scope);
                    while let Some(from) = scope {
                        if let Resolution::Found(target) = self.resolve_path(from, &name) {
                            if target != id && !self.is_ancestor_of(target, id) {
                                return Some(target);
                            }
                        }
                        scope = self.parent[from];
                    }
                    return None;
                }
                // A redefinition names a feature the *owner* inherits: `part :>> subsystemA`
                // redefines the `subsystemA` its supertype provides, not this declaration —
                // which is what a lookup from the owner finds first. Everything else is an
                // ordinary name in the owner's scope.
                let found = if redefs.iter().any(|r| r == one) {
                    match self.inherited_member(scope, one) {
                        Some(found) => found,
                        // The owner may be a performance, in which case the feature it performs
                        // supplies what is being redefined: `perform X.collectData { in part
                        // :>> testVehicle }` redefines `testVehicle` on `X.collectData`.
                        None => match self.performed_feature(scope).and_then(|performed| {
                            self.member_of_for_access(performed, one).into_iter().next()
                        }) {
                            Some(target) => Resolution::Found(target),
                            None => self.resolve_excluding(scope, one, id),
                        },
                    }
                } else {
                    self.resolve_excluding(scope, one, id)
                };
                match found {
                    Resolution::Found(target) if target != id => Some(target),
                    _ => None,
                }
            })
            .collect::<Vec<usize>>();
        // SysML gives every part an implicit supertype: the library's `Parts::Part`. The
        // specification writes no clause for it — `part p;` is a part because of the keyword —
        // and without the edge a part does not see `performedActions`, `exhibitedStates`, or the
        // rest of what `Part` provides. Member lookup asks this function, so the edge belongs
        // here rather than in each caller.
        if let Some(library) = self.implicit_library_supertype(id) {
            if !out.contains(&library) {
                out.push(library);
            }
        }
        // SysML §7.27.3: an element carrying a semantic-metadata annotation implicitly specializes
        // something the metadata's `baseType` value denotes. Two bullets apply, and which one is
        // decided by whether the annotated element is a definition or a usage.
        for derived in self.implicit_metadata_supertypes(id) {
            if !out.contains(&derived) {
                out.push(derived);
            }
        }
        // A case usage's own objective redefines the one its case definition states. SysML gives a
        // `Case` one objective, and a usage that writes its own is stating a redefinition of it —
        // `redefinition` implies subsetting, so the definition's objective is an inherited scope.
        // Without the edge the objective's body cannot see what that scope declares: measured,
        // `9-Verification-simplified.sysml:55` writes `verify vehicleMassRequirement :>>
        // massRequirement;` inside `verification vehicleMassTest : MassTest`, where
        // `massRequirement` is declared in `MassTest`'s objective, and the redefinition was
        // reported unresolved.
        if let Some(objective) = self.implicit_case_objective(id) {
            if !out.contains(&objective) {
                out.push(objective);
            }
        }
        for target in self.implicit_subsetting_targets(id) {
            if !out.contains(&target) {
                out.push(target);
            }
        }
        for target in self.implicit_redefinition_targets(id) {
            if !out.contains(&target) {
                out.push(target);
            }
        }
        // Fill on demand, and only once the index is complete. The rule pass asks about the ids it
        // needs; the model under check is usually far smaller than the index, which always carries
        // the standard library.
        if self.supertype_edges_ready.get() && IN_PROGRESS.with(|set| set.borrow().len() == 1) {
            self.supertype_edges_cache
                .borrow_mut()
                .insert(id, out.clone());
        }
        out
    }

    /// The objective a case usage's own objective redefines, resolved to an id.
    ///
    /// `objective o { … }` inside a usage typed by a case definition states that usage's objective,
    /// and the definition states one too. SysML allows a case one objective, so the usage's is a
    /// redefinition of the definition's, which is why the corpus writes `:>>` against features
    /// declared inside it: `verify vehicleMassRequirement :>> massRequirement` redefines the
    /// `massRequirement` that `MassTest`'s objective declares.
    ///
    /// Only a definition with exactly one objective answers. Two objectives is a model the case
    /// rules report, and inheriting from one of them would make name resolution depend on a defect.
    fn implicit_case_objective(&self, id: usize) -> Option<usize> {
        if id >= self.kind.len() || self.kind[id] != Kind::Objective {
            return None;
        }
        let owner = self.parent.get(id).copied().flatten()?;
        for sup in self.supertypes_of(owner) {
            let objectives: Vec<usize> = self.children[sup]
                .iter()
                .copied()
                .filter(|&child| self.kind[child] == Kind::Objective && child != id)
                .collect();
            if objectives.len() == 1 {
                return Some(objectives[0]);
            }
        }
        None
    }

    /// The specializations §7.27.3 derives from a metadata annotation, resolved to ids.
    ///
    /// "If the metadata definition … is a direct or indirect specialization of KerML metaclass
    /// SemanticMetadata …, each annotated element is then considered to implicitly specialize a
    /// definition or usage determined from the baseType value as follows: if the annotated type is a
    /// definition and the baseType is a usage … then the annotated definition implicitly
    /// subclassifies each definition … of the baseType [value]; if the annotated type is a usage and
    /// the baseType is a usage …, then the annotated usage implicitly subsets the baseType" — SysML
    /// §7.27.3, PDF p.192-193.
    ///
    /// The distinction is what keeps this from becoming a licence to inherit anything. The suite
    /// pins it down: in `MetadataTests_SemanticMetadata_invalid.kerml.xt`, `class C2 { @B; feature
    /// :>> x; }` is legal while `feature :>> y;` in the same class must be reported. `C2` is a
    /// definition, so it implicitly subclassifies `f`'s *type* `C`, which declares `x` and not `y`.
    /// A reading that gave the annotated element every member of `f` would make `y` legal too, and
    /// would turn a diagnostic the reference implementation produces into a missed one.
    fn implicit_metadata_supertypes(&self, id: usize) -> Vec<usize> {
        // Only an element that something annotated can gain these edges, and only a definition or a
        // usage is annotated at all.
        let mut out = Vec::new();
        for annotation in self.metadata_annotations_of(id) {
            let Some(definition) = self.metadata_definition_of(annotation) else {
                continue;
            };
            if !self.specializes_semantic_metadata(definition) {
                continue;
            }
            let Some(base) = self.base_type_value_of(definition) else {
                continue;
            };
            // A definition subclassifies the baseType value's type; a usage subsets the value.
            let derived = if self.kind[id].is_definition() {
                self.type_of(base).and_then(|text| {
                    let head = text.split(',').next().unwrap_or(text).trim().to_string();
                    match self.resolve(base, &head) {
                        Resolution::Found(target) => Some(target),
                        _ => None,
                    }
                })
            } else {
                Some(base)
            };
            if let Some(target) = derived {
                if target != id {
                    out.push(target);
                }
            }
        }
        out
    }

    /// The metadata usages written inside `id`, in authored order.
    fn metadata_annotations_of(&self, id: usize) -> Vec<usize> {
        // The owner's children, not a scan of the whole index. This is reached from
        // `implicit_metadata_supertypes`, which `supertypes_of` calls for every id the rules touch,
        // so the scan made the rule pass quadratic in the index size — and the index carries the
        // whole standard library, so the constant was large. `parent[child] == Some(id)` is exactly
        // membership in `children[id]`, which is already built.
        self.children
            .get(id)
            .map(|kids| {
                kids.iter()
                    .copied()
                    .filter(|&child| {
                        self.kind[child] == Kind::MetadataUsage && self.to_text(child).is_none()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The metadata definition a usage is typed by, following `resolve` on its `type_ref`.
    fn metadata_definition_of(&self, usage: usize) -> Option<usize> {
        let text = self.type_text[usage].iter().next()?.clone();
        let head = text.split(',').next().unwrap_or(&text).trim().to_string();
        match self.resolve(usage, &head) {
            Resolution::Found(target) if self.kind[target].is_definition() => Some(target),
            _ => None,
        }
    }

    /// Whether a metadata definition is `Metaobjects::SemanticMetadata` or specializes it.
    ///
    /// Asked of the supertype closure, which is how the specification words it ("a direct or
    /// indirect specialization of … SemanticMetadata"), and by qualified name, because the
    /// annotation may name a definition in any package.
    fn specializes_semantic_metadata(&self, definition: usize) -> bool {
        let mut stack = vec![definition];
        let mut seen: Vec<usize> = Vec::new();
        while let Some(current) = stack.pop() {
            if seen.contains(&current) {
                continue;
            }
            seen.push(current);
            if self.qualified.get(current).is_some_and(|q| {
                q.ends_with("SemanticMetadata") || q == "Metaobjects::SemanticMetadata"
            }) {
                return true;
            }
            stack.extend(self.supertypes_of(current));
        }
        false
    }

    /// The element a metadata definition binds its `baseType` to.
    ///
    /// `feature :>> baseType = f meta KerML::Type;` writes a metareference: the value is `f`, and
    /// `meta KerML::Type` only reflects it. So the first name in the value expression is the one
    /// that names the base — the same head `SEM003` already reports on when it names nothing.
    fn base_type_value_of(&self, definition: usize) -> Option<usize> {
        let mut stack = vec![definition];
        let mut seen: Vec<usize> = Vec::new();
        while let Some(current) = stack.pop() {
            if seen.contains(&current) {
                continue;
            }
            seen.push(current);
            for &child in self.children_of(current).iter() {
                if self.name[child] != "baseType" {
                    continue;
                }
                let Some(text) = self.value_text[child].as_ref() else {
                    continue;
                };
                let head = text
                    .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
                    .find(|s| !s.is_empty())?;
                if head == "meta" {
                    continue;
                }
                if let Resolution::Found(target) = self.resolve(child, head) {
                    return Some(target);
                }
            }
            stack.extend(self.supertypes_of(current));
        }
        None
    }

    /// The library type a kind implicitly specializes, resolved to its id.
    ///
    /// Every definition and usage kind specializes a base type the notation gives it: the
    /// specification writes no clause for it — `part p;` is a part because of the keyword. Each
    /// base declares `self`, so a usage whose own type also declares one inherits the name twice,
    /// which is the ambiguity the pilot reports as "Duplicate of inherited member name 'self'
    /// from DataValue, Part". Without these edges those warnings cannot be produced at all.
    ///
    /// The spelling is the library type's simple name plus the package it is declared in, both
    /// read from the library on disk (`core/stdlib`, the OMG 2025-02 kpar set):
    ///
    /// | written as | specializes |
    /// |---|---|
    /// | `part` / `item` / `port` | `Parts::Part`, `Items::Item`, `Ports::Port` |
    /// | `attribute` | `Base::DataValue` (as `Attributes::AttributeValue`, an alias of it) |
    /// | `occurrence` / `event` | `Occurrences::Occurrence` |
    /// | `action` | `Actions::Action` |
    /// | `state` | `States::StateAction` |
    /// | `calc` | `Calculations::Calculation` |
    /// | `case` / `analysis case` / `use case` / `verification` | `Cases::Case` and below |
    /// | `constraint` | `Constraints::ConstraintCheck` |
    /// | `requirement` | `Requirements::RequirementCheck` |
    /// | `connection` / `interface` / `allocation` | `Connections::Connection` and below |
    /// | `flow def` | `Flows::MessageAction` |
    /// | `flow` | `Flows::Message` |
    ///
    /// Most definitions and usages of one keyword share a base: `part def A` also specializes
    /// `Parts::Part`, which is why `part def B { part def B; }` has a collision only where the
    /// model writes one. Flow is the exception the library states explicitly: `MessageAction` is
    /// "the base type of all FlowDefinitions" and `Message`, below it, "the base type of all
    /// FlowUsages" (`core/stdlib/Systems Library/Flows.sysml:22-47`). Mapping a definition to
    /// `Flow` would instead give it `Message` — a usage base — as a supertype.
    fn attribute_usage_default(&self, id: usize) -> Option<usize> {
        // Inspect direct FeatureTyping targets; an illegal feature target keeps its identity.
        let scope = self.parent(id).unwrap_or(id);
        let direct: Vec<usize> = self
            .types_of(id)
            .iter()
            .flat_map(|text| text.split(','))
            .filter_map(|text| self.resolve_excluding(scope, text.trim(), id).found())
            .collect();
        let structural = direct
            .iter()
            .any(|&target| self.is_kind_of(target, "Structure"));
        let class = !structural
            && direct
                .iter()
                .any(|&target| self.is_kind_of(target, "Class"));
        let owner = self.parent(id);
        let composite_structure = self.is_composite_flag(id)
            && owner.is_some_and(|owner| {
                self.is_kind_of(owner, "Structure")
                    || self.specializes_from_library(owner, "Objects::Object")
            });
        let composite_class = self.is_composite_flag(id)
            && owner.is_some_and(|owner| {
                self.is_kind_of(owner, "Class")
                    || self.specializes_from_library(owner, "Occurrences::Occurrence")
            });
        let wanted = if structural {
            if composite_structure {
                "Objects::Object::subobjects"
            } else {
                "Objects::objects"
            }
        } else if class {
            if composite_class {
                "Occurrences::Occurrence::suboccurrences"
            } else {
                "Occurrences::occurrences"
            }
        } else {
            "Base::dataValues"
        };
        self.implicit_library_element(wanted)
            .filter(|&target| target != id)
    }

    fn implicit_library_supertype(&self, id: usize) -> Option<usize> {
        let wanted: &'static [&'static str] = match self.kind[id] {
            Kind::PartDef | Kind::PartUsage => &["Parts::Part"],
            Kind::ItemDef | Kind::ItemUsage => &["Items::Item"],
            Kind::PortDef | Kind::PortUsage => &["Ports::Port"],
            Kind::ActionDef | Kind::ActionUsage => &["Actions::Action"],
            // An accept/send action is typed by its own library action, and that is where two of
            // its input parameters come from: `AcceptPerformance` declares `inout payload` and
            // `in receiver` (`Transfers.kerml:254-265`), `SendPerformance` the payload, sender and
            // receiver. `validateAcceptActionUsageParameters` counts `inputParameters()->size()`
            // against `>= 2` and `validateSendActionParameters` against `>= 3`, so without this
            // base the count is of what the model wrote — often nothing — and the rule would
            // reject every conforming accept.
            Kind::AcceptActionUsage => &["Actions::AcceptAction"],
            Kind::SendActionUsage => &["Actions::SendAction"],
            Kind::ConnectionDef | Kind::Connection => &["Connections::Connection"],
            Kind::InterfaceDef if self.is_definition[id] => {
                if self.owned_ends_of(id).len() == 2 {
                    &["Interfaces::BinaryInterface"]
                } else {
                    &["Interfaces::Interface"]
                }
            }
            Kind::AllocationDef => &["Allocations::Allocation"],
            Kind::OccurrenceDef | Kind::OccurrenceUsage | Kind::EventOccurrenceUsage => {
                &["Occurrences::Occurrence"]
            }
            // Attribute usages obtain defaults from their direct types via implicit_subsetting_targets.
            Kind::AttributeDef => &["Base::DataValue"],
            Kind::StateDef | Kind::StateUsage => &["States::StateAction"],
            Kind::CalcDef | Kind::CalcUsage => &["Calculations::Calculation"],
            Kind::ConstraintDef | Kind::ConstraintUsage | Kind::AssertConstraint => {
                &["Constraints::ConstraintCheck"]
            }
            Kind::RequirementDef | Kind::RequirementUsage => &["Requirements::RequirementCheck"],
            // `message m : T from a to b` is a `FlowUsage` and its library base is `Message`
            // (`SysML.xtext:1245-1247` types it as `FlowUsage`; the library declares
            // `abstract flow def Message :> MessageAction, Transfer`). The pilot reports one more
            // provider than this chain supplies — it names `Action, Part`, while `Message`
            // reaches `Action` through `MessageAction` and reaches no `Part` at all. That comes
            // from the library version the pilot runs against, which this project does not have;
            // the base is still the right one.
            Kind::FlowUsage => &["Flows::Message"],
            // A `FlowDefinition` is not a `FlowUsage`, and the library states its own base:
            // `MessageAction` is "the base type of all FlowDefinitions"
            // (`core/stdlib/Systems Library/Flows.sysml:22-27`). `Flow` is a more specific usage
            // base that specializes `Message`, so mapping the definition to it would hand the
            // definition a usage base and hide the ends it inherits from `Links::BinaryLink`.
            Kind::Flow => &["Flows::MessageAction"],
            Kind::CaseDef => &["Cases::Case"],
            Kind::AnalysisCaseDef => &["AnalysisCases::AnalysisCase"],
            Kind::UseCaseDef | Kind::UseCaseUsage => &["UseCases::UseCase"],
            Kind::VerificationDef | Kind::VerificationUsage => {
                &["VerificationCases::VerificationCase"]
            }
            // The KerML classifiers. A KerML model writes `datatype D;` and the notation — not
            // the model — states what D specializes, exactly as it does for `part p;`. The
            // reference implementation keeps this mapping as a table of its own
            // (`research/upstream/sem/sysmlv2-semantics-0.1.2/src/lib.rs:1526-1536`:
            // `Classifier => Base::Anything`, `DataType => Base::DataValue`,
            // `Class => Occurrences::Occurrence`, `Structure => Objects::Object`,
            // `Association => Links::BinaryLink, Links::Link`,
            // `Behavior => Performances::Performance`, `Interaction => Transfers::Transfer`).
            //
            // Without them the whole KerML half of the suite is silent where it should warn:
            // `datatype D2 specializes D1, C1, A1` inherits `self` from `DataValue` (its own base
            // and D1's) and from `Occurrence` (C1 being a `class`), which is the ambiguity
            // `Specialization_invalid.kerml.xt:39` states as "from DataValue, Occurrence". Every
            // one of that file's four warnings is exactly this shape.
            Kind::KermlType => &["Base::Anything"],
            // "It is the most general Metaclass. All other Metaclasses must subclassify it directly
            // or indirectly" — `Metaobjects.kerml:16`. `metaclass A { … }` writes no `:>` clause for
            // it, just as `part def A` writes none for `Parts::Part`. Without the edge a metaclass
            // cannot see `annotatedElement`, which `Metaobject` declares
            // (`Metaobjects.kerml:21`), so `metaclass A { feature :>> annotatedElement : KerML::Feature; }`
            // — a line the reference implementation accepts — is reported unresolved (measured:
            // `MetadataTests_MetadataFeature_invalid.kerml.xt:42`). The library spells the same base
            // out as `abstract metaclass Metaobject specializes Object` (`:12`).
            Kind::MetadataDef => &["Metaobjects::Metaobject"],
            Kind::DataTypeDef => &["Base::DataValue"],
            Kind::ClassDef => &["Occurrences::Occurrence"],
            Kind::StructureDef => &["Objects::Object"],
            Kind::BehaviorDef => &["Performances::Performance"],
            Kind::FunctionDef => &["Performances::Evaluation"],
            // No entry for `assoc`. The reference maps `Association => Links::BinaryLink,
            // Links::Link`, and adding it here reported thirty sound models: `BinaryLink`
            // declares the ends `source` and `target`, so every `Interface` written in the suite
            // suddenly inherited two ends it did not write and the interface-end rules
            // (`validateInterfaceEnd`, which counts them) fired on all of them (measured:
            // `Interface %Example` and its six siblings). The KerML association tests do not
            // state a `self` ambiguity either, so the base buys nothing and costs thirty.
            _ => return None,
        };
        // The library type is looked up by qualified name, so two library packages that both
        // declare a `Case` cannot be confused: `Cases::Case` is one element, `AnalysisCases`'
        // `AnalysisCase` another.
        //
        // The lookup is memoised by name. It is reached from `supertypes_of`, which the rules enter
        // for every id they touch and again inside every closure walk, and the scan is over
        // `qualified` — the whole index, standard library included. Measured: `check` on a
        // five-line model took 1,400 ms with the scan and 190 ms with this memo. The memo is
        // cleared by `enable_supertype_cache`, because during construction the library is not in
        // the index yet and `None` would be recorded for every name.
        wanted.iter().find_map(|&name| {
            // The library base is itself of that kind, so it would be its own implicit
            // supertype — and is_subtype recursing through that never terminates.
            self.implicit_library_element(name)
                .filter(|&candidate| candidate != id)
        })
    }

    /// The definition a usage is typed by, resolved from its authored `type_ref`.
    fn typing_of(&self, id: usize) -> Option<usize> {
        // A usage's `type_ref` is the type it instantiates; a definition's is the supertype it
        // specializes. Both are stored in the same field, and for the member walk they are the
        // same step — "look inside what this element claims to be" — so both are followed here.
        // The distinction still matters to callers that ask for an instance's definition, which
        // use `definition_of` instead.
        let text = self.type_text[id].as_ref()?;
        // A comma list is a multi-parent clause; the first parent is the type for member
        // purposes, and `supertypes_of` supplies the rest.
        let head = text.split(',').next()?.trim();
        match self.resolve(id, head) {
            Resolution::Found(target) if target != id => Some(target),
            _ => None,
        }
    }

    /// The element a `part x : Type` is typed by, if it resolves to a definition.
    /// One step of a dotted feature path, starting from an already-resolved element.
    ///
    /// Differs from [`Index::resolve_path`] in its starting point: that one begins at a scope and
    /// walks outward, which is what a written name means. This one begins at a resolved element
    /// and looks *inside* it, which is what the next segment of a path means —
    /// `rearAxle.leftHalfAxle.axleToWheelPort` resolves `leftHalfAxle` inside `rearAxle`, not in
    /// whatever scope the statement happened to be written in.
    pub fn member_for_access(&self, container: usize, segment: &str) -> Resolution {
        let matches = self.member_of_for_access(container, segment);
        match matches.len() {
            1 => Resolution::Found(matches[0]),
            0 => Resolution::Unresolved,
            _ => Resolution::Ambiguous(matches),
        }
    }

    pub fn definition_of(&self, from: usize, reference: &str) -> Option<usize> {
        match self.resolve(from, reference) {
            Resolution::Found(id) if self.kind[id].is_definition() => Some(id),
            _ => None,
        }
    }

    /// Every reference the model writes, with its outcome: each `type_ref`, plus the two ends
    /// of a `satisfy` statement, which the view resolves through the same walk when it draws
    /// the edge — reporting them is what makes a dropped satisfy edge visible in
    /// `sysml resolve` instead of only as a missing arrow. Connection endpoints are dotted
    /// feature chains the resolver does not model, so they are not reported. Used by the CLI
    /// to report what the model depends on but does not contain. Import elements are skipped:
    /// their targets are reported through [`Index::imports`] instead, because an import
    /// target is not a feature typing.
    ///
    /// Names inside a requirement predicate are included as of the expression layer: until it
    /// existed the predicate was raw text, so `d.endurancee` was never looked up and a model
    /// could reference a feature it never declared without any tool noticing.
    pub fn references<'a>(&self, roots: &'a [Element]) -> Vec<Reference> {
        let mut out = Vec::new();
        // A clause written on an element that has no supertypes of its own can still name a member
        // the *owner* inherits: `ref :>> annotatedElement;` becomes an anonymous attribute under
        // `metadata def Sub :> SemanticMetadata`, and the member belongs to `SemanticMetadata`.
        // Asking the element searches nothing; asking its owner, then their owners, finds it.
        fn inherited_from_owners(index: &Index, id: usize, name: &str) -> Option<usize> {
            let mut owner = index.parent.get(id).copied().flatten();
            while let Some(node) = owner {
                // The whole ancestry, not one step. `member_of` answers with a scope's *direct*
                // members, and the declaration is often one or two levels up: `columnView` is
                // declared on `Views::TableView`, which the rendering's own view definition reaches
                // through its supertypes. One step found nothing and the reference was reported
                // unresolved (`11a-View-Viewpoint.sysml:50`).
                let mut frontier = index.supertypes_of(node);
                let mut seen: Vec<usize> = Vec::new();
                while let Some(sup) = frontier.pop() {
                    if seen.contains(&sup) {
                        continue;
                    }
                    seen.push(sup);
                    if let Some(found) = index.member_of(sup, name).into_iter().next() {
                        return Some(found);
                    }
                    frontier.extend(index.supertypes_of(sup));
                }
                owner = index.parent.get(node).copied().flatten();
            }
            None
        }
        fn walk(el: &Element, index: &Index, out: &mut Vec<Reference>) {
            if el.kind != Kind::Import {
                // A performance's clause, reported from the element that carries it. `perform b.a;`
                // lowers to two elements — the statement, which holds `performs = "b.a"`, and a host
                // named `a` inside it, which holds the same text as a subsetting. Only the statement
                // is the reference: the host exists to give the performance a scope, and its clause
                // is the same name written a second time.
                //
                // `performed_feature` is the walk the rest of the resolver already uses for this
                // clause, including the part that skips a host named after the feature it performs.
                // Measured before this: `perform b.a;` over a declared `b : B` drew
                // `unresolved: b.a`, while `perform b;` drew nothing — the dotted form could not be
                // answered by a single-name lookup, which is what the host's subsetting got.
                if let Some(text) = &el.performs {
                    out.push(Reference {
                        from: el.id,
                        text: text.clone(),
                        target: match index.performed_feature(el.id) {
                            Some(id) => Resolution::Found(id),
                            None => Resolution::Unresolved,
                        },
                        in_predicate: false,
                    });
                }
                if let Some(type_ref) = &el.type_ref {
                    out.push(Reference {
                        from: el.id,
                        text: type_ref.clone(),
                        target: index.resolve_typing(el.id, type_ref),
                        in_predicate: false,
                    });
                }
                // Authored order: `satisfy R by x` names the requirement first, the
                // satisfying element second.
                // `assign i.b := null` names the feature it writes through a dotted path,
                // and nothing else reports on it: `validateAssignmentActionUsageReferent` asks
                // what the referent is, which it cannot do until the name is resolved.
                if el.kind == Kind::AssignmentActionUsage {
                    if let Some(lhs) = &el.from {
                        let name = NameRef::new(lhs.clone(), el.line);
                        out.push(Reference {
                            from: el.id,
                            text: lhs.clone(),
                            target: index.resolve_path(el.id, &name),
                            in_predicate: false,
                        });
                    }
                }
                if el.kind == Kind::Satisfy {
                    for text in [&el.to, &el.from].into_iter().flatten() {
                        // The satisfying side can be a dotted feature path
                        // (`satisfy 'engine-v1 Specification' by vehicle1_c1.engine_v1`), so it
                        // goes through the same walk a predicate uses.
                        let name = NameRef::new(text, el.line);
                        out.push(Reference {
                            from: el.id,
                            text: text.clone(),
                            target: index.resolve_path(el.id, &name),
                            in_predicate: false,
                        });
                    }
                }
                if let Some(predicate) = &el.predicate {
                    for name in predicate.refs() {
                        out.push(Reference {
                            from: el.id,
                            text: name.text.clone(),
                            target: index.resolve_path(el.id, name),
                            in_predicate: true,
                        });
                    }
                }
                // The declaration head states more references than its `:` clause. `:>`,
                // `subsets`, `chains`, `conjugates`, `references` and `crosses` all name
                // something, and none of them was checked: `part def A :> Nowhere;` came back
                // silent while `ref_spans` covered the name for renames.
                //
                // `redefines` was left out on the reasoning that redefinitions "resolve by other
                // rules and are checked by whoever reads them". Measured, that is false for the
                // unresolved case: `feature :>> nosuchthing;` drew nothing at all, while the same
                // model with `:>` or `specializes` drew `unresolved: nosuchthing`. Every rule that
                // reads a redefinition — the end rule, the direction rule, the type rule — resolves
                // it only to decide something else, and none reports the name that is not there.
                // `sysml resolve` printed empty `resolved` and `not resolved` lists for it, so the
                // clause was never a reference at all.
                //
                // Including it asks the same question the other clauses ask. `supertypes` still
                // holds the redefinitions for the rules that read them; this list is about what was
                // *written*, and `:>>` writes a name as much as `:>` does.
                let mut seen: Vec<&str> = Vec::new();
                // A shorthand connector's projected end carries the endpoint as its reference,
                // and the connector-end walk already resolves that endpoint with a message that
                // names the end. Listing it here as well reported one fault twice
                // (`Connector_Invalid.sysml.xt:45`); the projected end is the one lowering
                // creates with the placeholder name, and a declared end always states its own.
                let authored_references: &[String] = if el.is_end && el.name == "<anonymous>" {
                    &[]
                } else {
                    &el.references
                };
                for text in el
                    .specializes
                    .iter()
                    .chain(el.subsets.iter())
                    .chain(el.conjugates.iter())
                    .chain(authored_references.iter())
                    .chain(el.crosses.iter())
                {
                    let name = text.split(',').next().unwrap_or(text).trim();
                    if name.is_empty() || seen.contains(&name) {
                        continue;
                    }
                    if name.is_empty() || seen.contains(&name) {
                        continue;
                    }
                    // The host of a dotted performance repeats its statement's clause as a
                    // subsetting. Reporting it would ask a second time, with a single-name lookup,
                    // the question the statement already answered with the path walk — and answer
                    // it wrongly: `perform b.a;` drew `unresolved: b.a` for a `b` that is declared.
                    if std::env::var("DSH_DEBUG_SKIP").is_ok() {
                        let par = index.parent.get(el.id).copied().flatten();
                        eprintln!(
                            "CLAUSE el={} kind={:?} name={:?} par={:?} par_performs={:?} text={:?}",
                            index.qualified[el.id],
                            index.kind[el.id],
                            index.name[el.id],
                            par.map(|p| index.qualified[p].clone()),
                            par.and_then(|p| index.performs[p].clone()),
                            name
                        );
                    }
                    // The dot in a performance clause is a path, not a nesting step, so the
                    // single-name lookup below cannot answer it. The element that carries
                    // `performs` has already been reported through the path walk; the same text
                    // also sits in its `subsets` (the `OwnedReferenceSubsetting` the grammar
                    // writes), and reporting that copy asks a second time with the wrong tool.
                    // Recorded in `seen` so the `redefines` loop below does not report it either —
                    // the host of `perform b.a;` carries the text in both clauses.
                    if index
                        .performs
                        .get(el.id)
                        .and_then(|p| p.as_ref())
                        .is_some_and(|performed| {
                            performed.split(',').next().unwrap_or(performed).trim() == name
                        })
                    {
                        seen.push(name);
                        continue;
                    }
                    seen.push(name);
                    out.push(Reference {
                        from: el.id,
                        text: name.to_string(),
                        // `crosses y.b` names a member of the opposite end: a dotted path, and the
                        // single-name lookup asks whether the whole path is declared here, which
                        // it never is. `satisfy ... by p.q` above goes the same way.
                        target: if name.contains('.') {
                            let path = NameRef::new(name, el.line);
                            index.resolve_path(el.id, &path)
                        } else {
                            // Excluding the element itself, for the same reason the redefinition
                            // loop does: `ref :> annotatedElement;` is anonymous and takes the name
                            // of what it subsets (`parser.rs:1506`), so the element is called
                            // `annotatedElement` and a plain lookup answers with itself.
                            //
                            // A subsetting often names a member of the element's own *type* — the
                            // standard library writes `ref :> annotatedElement;` inside
                            // `metadata def CausationMetadata :> SemanticMetadata`
                            // (`CauseAndEffect.sysml:37`), where `annotatedElement` is declared on
                            // `Metaobjects::SemanticMetadata`. Neither a scope lookup nor the owner's
                            // inherited members find that; asking the element's supertypes does,
                            // which is the walk `supertypes_of` uses for the same clause.
                            match index.resolve_excluding(el.id, name, el.id) {
                                Resolution::Found(target) => Resolution::Found(target),
                                other => inherited_from_owners(index, el.id, name)
                                    .map(Resolution::Found)
                                    .unwrap_or(other),
                            }
                        },
                        in_predicate: false,
                    });
                }
                // A redefinition, separately, because it is the one clause whose name can be the
                // declaration's own name. `feature :>> x;` is written without a name and takes one
                // from the redefinition (`parser.rs:1506`), so the element ends up *called* `x`
                // inside its own scope; a plain `resolve` then answers with the element itself.
                // Measured: `sysml resolve` printed `p::C1::x : x -> p::C1::x` — the clause resolved
                // to the very thing it redefines.
                //
                // `resolve_excluding` is the walk `supertypes_of` already uses for this clause
                // (`:1656`), so the verdict reported here is the one the rules believe. The name
                // the redefinition introduces is skipped and the inherited feature is found.
                //
                // A qualified or dotted target takes the path walk instead, as `supertypes_of` does
                // first (`:1622`): `redefines B1::p1::myDefIn` names a feature through its owners,
                // and `perform b.a` lowers a host whose clause is the path its statement names. The
                // single-name lookup answers `not accessible` for the first and `unresolved` for the
                // second, though both name things the model declares.
                for text in &el.redefines {
                    let name = text.split(',').next().unwrap_or(text).trim();
                    if name.is_empty() || seen.contains(&name) {
                        continue;
                    }
                    seen.push(name);
                    out.push(Reference {
                        from: el.id,
                        text: name.to_string(),
                        target: if name.contains("::") || name.contains('.') {
                            let path = NameRef::new(name, el.line);
                            index.resolve_path(el.id, &path)
                        } else {
                            // The same inheritance fallback the subsetting branch uses, and for the
                            // same reason: `ref :>> annotatedElement;` inside
                            // `metadata def Sub :> SemanticMetadata` names a member of `Sub`'s own
                            // supertype, which neither a plain scope lookup nor `resolve_excluding`
                            // answers.
                            match index.resolve_excluding(el.id, name, el.id) {
                                Resolution::Found(target) => Resolution::Found(target),
                                other => inherited_from_owners(index, el.id, name)
                                    .map(Resolution::Found)
                                    .unwrap_or(other),
                            }
                        },
                        in_predicate: false,
                    });
                }
                // `FeatureChainPrefix` is `( ownedRelationship += OwnedFeatureChaining '.' )+
                // ownedRelationship += OwnedFeatureChaining` (`KerML.xtext:1046-1047`), so every
                // segment is its own `OwnedFeatureChaining`. Resolving the whole text as a dotted
                // access asked "is `b` a member of `a`'s type", which is the wrong question — and
                // answered `unresolved` for `feature a; feature b; feature c chains a.b;`, which
                // the pilot states is legal (`FeatureChain_invalid.kerml.xt` asserts only the
                // one-segment case).
                for text in &el.chains {
                    let head = text.split(',').next().unwrap_or(text).trim();
                    if head.is_empty() {
                        continue;
                    }
                    // The first segment is a name in scope. Each later one is a member of what
                    // the previous segment denotes — `chains a.m` reads `m` out of `a` — and when
                    // that step finds nothing it falls back to the scope, because a chain between
                    // two untyped features names two features in the same namespace.
                    let mut previous: Option<usize> = None;
                    for segment in head.split('.').map(str::trim).filter(|s| !s.is_empty()) {
                        let target = match previous {
                            None => index.resolve(el.id, segment),
                            Some(from) => {
                                let step =
                                    index.resolve_path(from, &NameRef::new(segment, el.line));
                                match step {
                                    Resolution::Found(found) => Resolution::Found(found),
                                    _ => index.resolve(el.id, segment),
                                }
                            }
                        };
                        if let Resolution::Found(found) = target {
                            previous = Some(found);
                        } else {
                            previous = None;
                        }
                        if seen.contains(&segment) {
                            continue;
                        }
                        seen.push(segment);
                        out.push(Reference {
                            from: el.id,
                            text: segment.to_string(),
                            target,
                            in_predicate: false,
                        });
                    }
                }
                for child in &el.children {
                    walk(child, index, out);
                }
            }
        }
        for root in roots {
            walk(root, self, &mut out);
        }
        // `::` is for namespaces; a path that reaches a feature's member with it is a notation
        // error, reported at the path. Imports never come through here — the walk skips
        // `Kind::Import` — which is what keeps `public import y::y1` legal.
        //
        // The error is for value expressions and for connector, binding and flow ends. A typing,
        // subsetting or redefinition clause names its target as a qualified path, and the reference
        // accepts a feature's member there: `feature try : A::a1 redefines B::b;` with `A` and `B`
        // features is `SimpleImportTestsFromOtherFile_Import3_FT.kerml.xt`, a model it states clean.
        for reference in &mut out {
            let in_clause = self
                .type_text
                .get(reference.from)
                .and_then(|t| t.as_deref())
                == Some(reference.text.as_str())
                || self
                    .redefines
                    .get(reference.from)
                    .is_some_and(|r| r.contains(&reference.text))
                || self
                    .subsets
                    .get(reference.from)
                    .is_some_and(|s| s.contains(&reference.text));
            if !in_clause
                && reference.text.contains("::")
                && matches!(reference.target, Resolution::Found(_))
                && self.crosses_a_feature(reference.from, &reference.text)
            {
                reference.target = Resolution::NotAccessible;
            }
        }
        out
    }

    /// Every import statement in the index, in id order, with the names it bound.
    pub fn imports(&self) -> Vec<ImportEntry> {
        let mut out = Vec::new();
        for id in 0..self.len() {
            let Some((shape, visibility)) = self.form[id] else {
                continue;
            };
            out.push(ImportEntry {
                owner: self.parent[id],
                element: id,
                shape,
                visibility,
                target: self.resolve_target(id, &self.name[id]),
                bound: self.bound_by.get(&id).cloned().unwrap_or_default(),
            });
        }
        out
    }

    /// Qualified name of every element, keyed by id.
    pub fn qualified_names(&self) -> HashMap<usize, &str> {
        self.qualified
            .iter()
            .enumerate()
            .map(|(id, name)| (id, name.as_str()))
            .collect()
    }

    // -- import binding ------------------------------------------------------

    /// Compute every import's bindings, repeating until nothing changes.
    ///
    /// Library imports are processed before project ones: a project's `import ISQ::*`
    /// re-exports what ISQ itself publicly imported, and that has to be bound first. The
    /// fixed point removes the remaining order sensitivity, so a `public import` in a later
    /// file still reaches an earlier file's namespace import.
    ///
    /// The loop is bounded by the number of imports: each pass resolves at least one more
    /// dependency or reaches the fixed point, and there are only finitely many. The state
    /// comparison is order-sensitive because lookup takes the last match, so "unchanged"
    /// really does mean the next pass would reproduce this one.
    fn bind_imports(&mut self) {
        let order: Vec<usize> = (self.library_start..self.len())
            .chain(0..self.library_start)
            .filter(|&id| self.kind[id] == Kind::Import)
            .collect();
        if order.is_empty() {
            return;
        }
        // Every pass is a pure function of the state at its start, so the sequence either
        // reaches a fixed point or revisits a state and cycles. Comparing against every state
        // seen so far detects both, and makes the bound a safety net rather than the argument:
        // a converging chain of n imports needs n passes, which the cap allows.
        let mut seen: Vec<Vec<(usize, String, usize)>> = Vec::new();
        let mut converged = false;
        for _ in 0..order.len() + 2 {
            // Bindings accumulate across passes instead of being rebuilt each time. A pass
            // processes imports in a fixed order, so an import that runs before its own
            // dependency cannot see that dependency's bindings until the next pass; with a
            // per-pass reset the end-of-pass state would repeat and the loop would stop one
            // pass too early, exactly when a re-export chain crosses files. `Overwrite`
            // replaces rather than appends, so re-running an import is idempotent and the
            // end-of-pass state is a pure function of the start-of-pass state.
            self.bound_by.clear();
            for &id in &order {
                self.bind_one(id);
            }
            let state = self.binding_state();
            if seen.contains(&state) {
                converged = true;
                break;
            }
            seen.push(state);
        }
        debug_assert!(
            converged,
            "import binding neither settled nor repeated within {} passes",
            order.len() + 2
        );
    }

    /// All bindings as an order-preserving snapshot, so the fixed point can tell "nothing
    /// changed" from "same names, different targets".
    fn binding_state(&self) -> Vec<(usize, String, usize)> {
        let mut state: Vec<(usize, String, usize)> = Vec::new();
        for (scope, bindings) in self.scope_bindings.iter().enumerate() {
            for binding in bindings {
                state.push((scope, binding.name.clone(), binding.target));
            }
        }
        for binding in &self.root_bindings {
            state.push((usize::MAX, binding.name.clone(), binding.target));
        }
        state
    }

    fn bind_one(&mut self, id: usize) {
        let Some((shape, visibility)) = self.form[id] else {
            return;
        };
        let text = self.name[id].clone();
        let scope = self.parent[id];
        let Resolution::Found(target) = self.resolve_target(id, &text) else {
            // Unresolved target: the import binds nothing. Reported through `imports()`.
            return;
        };
        let mut bound: Vec<(String, usize)> = Vec::new();
        match shape {
            ImportShape::Membership { recursive } => {
                // `import A::B` binds B's simple name. `import A::B::**` also pulls in
                // everything owned below B, recursively.
                let simple = simple_name(&text);
                if self.bind_into(scope, simple.clone(), target, visibility, Fill::Overwrite) {
                    bound.push((simple, target));
                }
                if recursive {
                    self.bind_descendants(scope, target, visibility, &mut bound);
                }
            }
            ImportShape::Namespace { recursive } => {
                for child in self.children[target].clone() {
                    // An import is a relationship, not a membership, so the target's own
                    // imports are not names it exports. Neither is a relationship statement:
                    // a `satisfy R by x` element is named after `R` only for display, and
                    // exporting that name would shadow the real `R` for every importer.
                    if self.kind[child] == Kind::Import || !self.named[child] {
                        continue;
                    }
                    self.bind_member(scope, child, visibility, Fill::Overwrite, &mut bound);
                }
                // Re-export: names the target itself imported publicly become visible here
                // too, which is how `import ISQ::*` reaches `MassValue` in `ISQBase`.
                for binding in self.scope_bindings[target].clone() {
                    if binding.visibility != Some(Visibility::Public) {
                        continue;
                    }
                    let name = binding.name.clone();
                    if self.bind_into(
                        scope,
                        name.clone(),
                        binding.target,
                        visibility,
                        Fill::IfAbsent,
                    ) {
                        bound.push((name, binding.target));
                    }
                }
                if recursive {
                    self.bind_descendants(scope, target, visibility, &mut bound);
                }
            }
            ImportShape::Filter => {
                // A filter selects a subset by expression. The expressions are not modelled,
                // so binding anything would be a guess; the import shows up in the report
                // with an empty binding list.
            }
        }
        if !bound.is_empty() {
            self.bound_by.insert(id, bound);
        }
    }

    /// Bind every element owned below `to`, by simple name, first one wins. Import elements are
    /// skipped for the same reason as in the namespace form, and so are relationship statements.
    fn bind_descendants(
        &mut self,
        scope: Option<usize>,
        to: usize,
        visibility: Option<Visibility>,
        bound: &mut Vec<(String, usize)>,
    ) {
        for child in self.children[to].clone() {
            if self.kind[child] == Kind::Import || !self.named[child] {
                continue;
            }
            self.bind_member(scope, child, visibility, Fill::IfAbsent, bound);
            self.bind_descendants(scope, child, visibility, bound);
        }
    }

    /// Bind an element under every spelling it owns: its simple name, and its `<B>` short name
    /// when it declared one. `import Time::*` has to make `UTC` reachable, and the library
    /// writes that feature as `attribute <UTC> 'Coordinated Universal Time'`.
    fn bind_member(
        &mut self,
        scope: Option<usize>,
        child: usize,
        visibility: Option<Visibility>,
        fill: Fill,
        bound: &mut Vec<(String, usize)>,
    ) {
        let name = self.name[child].clone();
        if self.bind_into(scope, name.clone(), child, visibility, fill) {
            bound.push((name, child));
        }
        if let Some(short) = self.short_name[child].clone() {
            if short != self.name[child] {
                let recorded = self.bind_into(scope, short.clone(), child, visibility, fill);
                if recorded {
                    bound.push((short, child));
                }
            }
        }
    }

    /// Record one binding. Returns whether it was actually recorded, so callers can report
    /// only the names an import really introduced.
    fn bind_into(
        &mut self,
        scope: Option<usize>,
        name: String,
        target: usize,
        visibility: Option<Visibility>,
        fill: Fill,
    ) -> bool {
        let bindings = match scope {
            Some(scope) => &mut self.scope_bindings[scope],
            None => &mut self.root_bindings,
        };
        match bindings.iter().position(|b| b.name == name) {
            // Present with the same target: already established, which counts as bound so
            // that a later pass reports the same set as the pass that created it.
            Some(at) if bindings[at].target == target => true,
            Some(at) if fill == Fill::Overwrite => {
                bindings[at] = Binding {
                    name,
                    target,
                    visibility,
                };
                true
            }
            // Held by someone else under IfAbsent: the first binding wins, and this one is
            // not recorded. Pushing a second entry here was the bug that made import
            // binding grow without bound: every pass appended another copy of the same
            // name, so the fixed-point comparison could never see a repeated state, and a
            // model with a recursive import over a nested part tree aborted at the pass cap.
            Some(_) => false,
            None => {
                bindings.push(Binding {
                    name,
                    target,
                    visibility,
                });
                true
            }
        }
    }
}

/// The last `::`-separated segment: the simple name a membership import binds.
fn simple_name(path: &str) -> String {
    path.rsplit("::").next().unwrap_or(path).trim().to_string()
}
