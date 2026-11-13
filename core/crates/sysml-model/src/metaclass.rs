//! The SysML/KerML metaclass hierarchy, and which metaclasses each [`Kind`] stands for.
//!
//! The specification's constraints are written against metaclasses: `oclIsKindOf(Expression)`,
//! `oclIsKindOf(FeatureMembership)`, `selectByKind(Usage)`. [`Kind`] is a projection — 72 variants
//! standing for 175 metaclasses — so a rule that asks one of those questions needs an answer that
//! follows the metamodel's inheritance, not the projection's shape. This module is that answer.
//!
//! [`METACLASSES`] is generated (`tools/metaclasses.py`) from the published metamodel schema and
//! checked edge for edge against the published crate's own hierarchy. [`Kind::metaclasses`] is
//! written by hand and pinned by tests: every kind maps to at least one metaclass or to none with a
//! recorded reason, and every name it maps to exists in the table.
//!
//! # What a projection collapse costs, and how it is recorded
//!
//! Some kinds stand for more than one metaclass because the lowering keeps one word where the
//! metamodel has two: `Kind::Connection` is both a `ConnectionUsage` (`connector c`) and a
//! KerML `Connector`, `Kind::Import` is both a `NamespaceImport` and a `MembershipImport`. Those
//! map to a list, and [`Kind::is_metaclass`] answers true if *any* of them answers true — the
//! direction that lets a rule judge the element rather than skip it.
//!
//! Five kinds map to nothing, each for a reason that is a gap rather than a decision:
//!
//!   * [`Kind::Variant`] — a bare `variant v;` writes no kind keyword, so there is no metaclass
//!     to name: the element is a variant membership and nothing else. The spellings that do write
//!     a kind (`variant part x`, `variant def V`) keep it and set
//!     [`Element::is_variant`](crate::Element::is_variant), so they answer their own metaclass.
//!   * [`Kind::PseudoState`] — a marker the lowering invents (`lower.rs:790`), not a metaclass.
//!   * [`Kind::Alias`] — `alias A for B` is an `AliasMembership` whose member is the second name;
//!     the schema this table came from has no `Alias` metaclass.
//!   * [`Kind::Unsupported`] and [`Kind::Other`] — constructs the syntax layer recognised but does
//!     not model. A rule asking about them gets `false` for every metaclass, so the rule must treat
//!     them as *inconclusive* rather than as violations. Nothing in the tree says otherwise.

use std::collections::VecDeque;

use super::Kind;

/// One metaclass and its direct supertypes.
#[derive(Debug, Clone, Copy)]
pub struct Meta {
    pub name: &'static str,
    pub supers: &'static [&'static str],
}

/// Every metaclass the SysML v2 metamodel declares, with its direct supertypes.
///
/// Generated; do not edit by hand. `python tools/metaclasses.py` rewrites it from
/// `research/upstream/SysML.json` and refuses to write when the schema and the published crate's
/// hierarchy disagree on any edge.
pub const METACLASSES: &[Meta] = include!("../data/metaclasses.rs");

/// One metaclass by name.
pub fn meta(name: &str) -> Option<&'static Meta> {
    METACLASSES.iter().find(|m| m.name == name)
}

/// Whether `name` exists in the metamodel.
pub fn is_metaclass(name: &str) -> bool {
    meta(name).is_some()
}

/// Whether `name` is `ancestor` or specializes it.
///
/// An unknown name is not an instance of anything: a typo in a rule must not silently pass every
/// element, so the answer is `false` and the tests check every name a rule passes.
///
/// Breadth-first over the direct-supertype edges with a visited set, because the closure is a
/// graph the specification states and the table is small (175 rows, depth 6).
pub fn is_a(name: &str, ancestor: &str) -> bool {
    if name == ancestor {
        return true;
    }
    let Some(first) = meta(name) else {
        return false;
    };
    let mut seen: Vec<&'static str> = Vec::new();
    let mut queue: VecDeque<&'static str> = first.supers.iter().copied().collect();
    while let Some(current) = queue.pop_front() {
        if current == ancestor {
            return true;
        }
        if seen.contains(&current) {
            continue;
        }
        seen.push(current);
        if let Some(row) = meta(current) {
            queue.extend(row.supers.iter().copied());
        }
    }
    false
}

/// Every supertype of `name`, nearest first, without duplicates.
pub fn ancestors(name: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    let mut queue: VecDeque<&'static str> = VecDeque::new();
    if let Some(row) = meta(name) {
        queue.extend(row.supers.iter().copied());
    }
    while let Some(current) = queue.pop_front() {
        if out.contains(&current) {
            continue;
        }
        out.push(current);
        if let Some(row) = meta(current) {
            queue.extend(row.supers.iter().copied());
        }
    }
    out
}

impl Kind {
    /// The metaclasses this kind stands for.
    ///
    /// One entry for most kinds; several where the projection keeps one word for two metamodel
    /// classes; none for the five kinds the module note lists, each of which is a gap the tree
    /// cannot answer rather than a metaclass that happens to be empty.
    pub fn metaclasses(&self) -> &'static [&'static str] {
        match self {
            Kind::Package => &["Package"],
            Kind::PartDef => &["PartDefinition"],
            Kind::PartUsage => &["PartUsage"],
            // `actor driver;` is the owned member of an `ActorMembership`, which the metamodel
            // types as a `PartUsage`; `stakeholder owner;` is the same through a
            // `StakeholderMembership`.
            Kind::Actor | Kind::Stakeholder => &["PartUsage"],
            // `verify r : R` is a `RequirementVerificationMembership`'s owned requirement.
            Kind::Verification => &["RequirementUsage"],
            Kind::PortDef => &["PortDefinition"],
            Kind::PortUsage => &["PortUsage"],
            Kind::AttributeDef => &["AttributeDefinition"],
            Kind::AttributeUsage => &["AttributeUsage"],
            Kind::RequirementDef => &["RequirementDefinition"],
            Kind::RequirementUsage => &["RequirementUsage"],
            Kind::VerificationDef => &["VerificationCaseDefinition"],
            Kind::VerificationUsage => &["VerificationCaseUsage"],
            Kind::Satisfy => &["SatisfyRequirementUsage"],
            // The lowering does not keep which of the two notations a connector was written in, and
            // both are features of the owning type, so both are named.
            Kind::Connection => &["ConnectionUsage", "Connector"],
            Kind::ActionDef => &["ActionDefinition"],
            Kind::ActionUsage => &["ActionUsage"],
            Kind::ConstraintDef => &["ConstraintDefinition"],
            Kind::ConstraintUsage => &["ConstraintUsage"],
            // `expose X;` is a membership, not a usage: `MembershipExpose specializes Expose`.
            Kind::Expose => &["MembershipExpose"],
            Kind::RenderingDef => &["RenderingDefinition"],
            Kind::RenderingUsage => &["RenderingUsage"],
            // A parameter is the owned member of a `ParameterMembership`, which the metamodel types
            // as a `Feature`; a value parameter is a `ReferenceUsage`.
            Kind::Parameter => &["ReferenceUsage"],
            // The keyword decides which node; the table names the common supertype because the
            // lowering keeps the keyword in the element's name rather than in a kind of its own.
            Kind::ControlNode => &["ControlNode"],
            // An `if` / `while` / `for` body and an assignment are action usages owned by the
            // action that contains them.
            Kind::ControlFlow => &["ActionUsage"],
            Kind::Bind => &["BindingConnectorAsUsage"],
            Kind::Flow => &["FlowDefinition"],
            // `flow of fuel : Fuel` — the feature the flow carries.
            Kind::PayloadFeature => &["PayloadFeature"],
            Kind::FlowUsage => &["FlowUsage"],
            Kind::Ref => &["ReferenceUsage"],
            // See the module note: the word `variant` is the kind, and what follows it is lost.
            Kind::Variant => &[],
            Kind::DefaultReference => &["ReferenceUsage"],
            Kind::AssertConstraint => &["AssertConstraintUsage"],
            // `objective { ... }` is a `RequirementUsage` inside an `ObjectiveMembership`.
            Kind::Objective => &["RequirementUsage"],
            // A KerML type declaration whose classifier the lowering did not keep. `Type` is true
            // of all of them and the narrower answer is not in the tree.
            Kind::KermlType => &["Type"],
            Kind::KermlFeature => &["Feature"],
            Kind::ClassDef => &["Class"],
            Kind::BehaviorDef => &["Behavior"],
            Kind::FunctionDef => &["Function"],
            Kind::ExpressionDef => &["Expression"],
            Kind::DataTypeDef => &["DataType"],
            Kind::StructureDef => &["Structure"],
            Kind::MultiplicityDef => &["Multiplicity"],
            Kind::AssocDef => &["Association"],
            Kind::AssocStructureDef => &["AssociationStructure"],
            Kind::AllocationDef => &["AllocationDefinition"],
            Kind::AllocationUsage => &["AllocationUsage"],
            Kind::UseCaseDef => &["UseCaseDefinition"],
            Kind::UseCaseUsage => &["UseCaseUsage"],
            Kind::ConcernDef => &["ConcernDefinition", "ConcernUsage"],
            Kind::AnalysisCaseDef => &["AnalysisCaseDefinition"],
            Kind::CaseDef => &["CaseDefinition"],
            Kind::Dependency => &["Dependency"],
            Kind::Specialization => &["Specialization"],
            Kind::Disjoining => &["Disjoining"],
            Kind::TypeFeaturing => &["TypeFeaturing"],
            Kind::Comment => &["Comment"],
            Kind::Inverting => &["FeatureInverting"],
            // `filter @Safety;` is an `ElementFilterMembership` on the enclosing namespace.
            Kind::Filter => &["ElementFilterMembership"],
            Kind::CalcDef => &["CalculationDefinition"],
            Kind::StateDef => &["StateDefinition"],
            Kind::StateUsage => &["StateUsage"],
            Kind::Transition => &["TransitionUsage"],
            // `entry` / `do` / `exit` are action usages owned through a `StateSubactionMembership`.
            Kind::StateAction => &["ActionUsage"],
            Kind::PseudoState => &[],
            Kind::ItemDef => &["ItemDefinition"],
            Kind::ItemUsage => &["ItemUsage"],
            Kind::EnumDef => &["EnumerationDefinition"],
            Kind::EnumValue => &["EnumerationUsage"],
            // `isIndividual` is a boolean on `Definition` in the metamodel, not a class of its own,
            // and the notation's individual definitions are occurrence definitions: the corpus
            // declares `individual def VehicleA_ID1 :> VehicleA` and owns snapshots and timeslices
            // with them (`6-Individual and Snapshots.sysml:50`, `:113`). The kind keeps the flag
            // and loses the definition it specializes, so `OccurrenceDefinition` is the reading
            // that answers both what the flag means and what those models state.
            Kind::IndividualDef => &["OccurrenceDefinition"],
            // The table carries the classes the rules ask about, and `IndividualUsage` is not one
            // of them: no rule names it, and the two that judge it ask `OccurrenceUsage`. Its
            // supertype answers both — an `individual x : T` is an occurrence usage whose
            // `isIndividual` is set.
            Kind::IndividualUsage => &["OccurrenceUsage"],
            Kind::MetadataDef => &["MetadataDefinition"],
            Kind::MetadataUsage => &["MetadataUsage"],
            Kind::Alias => &[],
            Kind::InterfaceDef => &["InterfaceDefinition"],
            Kind::InterfaceUsage => &["InterfaceUsage"],
            // The projection keeps one kind for `view def V` and `view v : V`; both spellings
            // have to answer, or a rule about a usage reports the legal model.
            Kind::ViewDef => &["ViewDefinition", "ViewUsage"],
            Kind::ViewUsage => &["ViewUsage"],
            Kind::ViewpointUsage => &["ViewpointUsage"],
            Kind::OccurrenceDef => &["OccurrenceDefinition"],
            Kind::OccurrenceUsage => &["OccurrenceUsage"],
            // `event e : E` is `OccurrenceUsagePrefix 'event' (OwnedReferenceSubsetting | …)`
            // (`SysML.xtext:865-870`): an `EventOccurrenceUsage`, which references the event.
            Kind::EventOccurrenceUsage => &["EventOccurrenceUsage"],
            // `assign x := v` is an `AssignmentActionUsage`, an action usage that writes a feature.
            Kind::AssignmentActionUsage => &["AssignmentActionUsage"],
            // `accept p : T` / `send p : T to r` are action usages of their own; the keyword in the
            // notation is the metaclass name.
            Kind::AcceptActionUsage => &["AcceptActionUsage"],
            Kind::SendActionUsage => &["SendActionUsage"],
            Kind::CalcUsage => &["CalculationUsage"],
            // The same collapse: `viewpoint def V` and `viewpoint v : V` are one kind, and a
            // viewpoint usage is what a `satisfy` may name.
            Kind::ViewpointDef => &["ViewpointDefinition", "ViewpointUsage"],
            Kind::ConnectionDef => &["ConnectionDefinition"],
            Kind::Perform => &["PerformActionUsage"],
            Kind::ExhibitState => &["ExhibitStateUsage"],
            // `subject d : Drone` is a `SubjectMembership`'s owned parameter, a `ReferenceUsage`.
            Kind::Subject => &["ReferenceUsage"],
            // `frame k : Concern` is a `FramedConcernMembership`'s owned concern, a
            // `RequirementUsage`.
            Kind::Frame => &["RequirementUsage"],
            Kind::Unsupported => &[],
            // The lowering does not keep whether an import brought names (`::*`) or one membership;
            // `Import` is the supertype of both, so it is the one true answer.
            Kind::Import => &["Import"],
            Kind::Other(_) => &[],
        }
    }

    /// Whether this kind stands for `metaclass` or one of its subtypes.
    ///
    /// True when any metaclass the kind stands for has `metaclass` in its supertype closure. A
    /// kind that stands for nothing answers `false` — see the module note on the five that do.
    pub fn is_metaclass(&self, metaclass: &str) -> bool {
        self.metaclasses().iter().any(|m| is_a(m, metaclass))
    }
}
