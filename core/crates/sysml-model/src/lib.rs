//! Language-neutral element tree produced by the syntax layer.
//!
//! This is deliberately not a mirror of the OMG metamodel. It is the projection the
//! downstream layers (resolve, verify, view) actually consume: a named node with a kind,
//! an optional type reference, documentation, source line and ordered children.

use std::fmt;

pub mod metaclass;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Kind {
    /// The default exists only so `Element` can derive `Default`; a constructed
    /// element always gets its kind from the lowering layer.
    #[default]
    Package,
    PartDef,
    PartUsage,
    /// `actor driver;` — a participant of a use case or requirement.
    Actor,
    /// `stakeholder owner;` — someone with an interest in a requirement.
    Stakeholder,
    /// `verify r : R;` — a requirement verification inside an objective.
    ///
    /// Distinct from [`Kind::VerificationUsage`]: `verification v : T` declares a verification
    /// case usage, which may stand anywhere; `verify r : R` only makes sense inside an
    /// `objective`. Collapsing the two made the owning-type rule reject the former.
    Verification,
    PortDef,
    PortUsage,
    AttributeDef,
    AttributeUsage,
    RequirementDef,
    RequirementUsage,
    VerificationDef,
    VerificationUsage,
    Satisfy,
    Connection,
    ActionDef,
    ActionUsage,
    /// `constraint x : Hover;` — a named constraint usage.
    ConstraintDef,
    /// `constraint c { ... }` declared inside a definition body or another constraint.
    ConstraintUsage,
    /// `expose X;` inside a view definition — what the view shows.
    Expose,
    /// `rendering def Name;` — how a view is drawn.
    RenderingDef,
    /// `rendering r : R;` / `render r : R;` — one rendering a view states.
    ///
    /// Kept apart from [`Kind::RenderingDef`] because the specification bounds a *view* to one
    /// `ViewRenderingMembership` and that membership is what `render` introduces
    /// (`SysML.xtext:2335-2343`); counting a declared rendering definition towards that bound
    /// rejects a legal view body.
    RenderingUsage,
    /// One parameter of an action, calculation or constraint (`in x : Real`). Carries
    /// [`Element::direction`]; a return parameter has direction `out` and the name `return`.
    Parameter,
    /// A control node — `merge`, `decision`, `join`, `fork`. Named by its keyword so a reader can
    /// tell which kind of synchronization the model asked for.
    ControlNode,
    /// A control-flow branch: the body of an `if`/`else`, a loop body, or an assignment inside
    /// an action. The name is the construct (`if`, `else`, `while`, `for`, `loop`, `assign`).
    ControlFlow,
    /// `assign x := v` — an assignment to a feature.
    ///
    /// `AssignmentActionUsage` is a metaclass of its own, and the projection used to keep the word
    /// `assign` as a [`Kind::ControlFlow`] name, so no rule about an assignment could find one.
    AssignmentActionUsage,
    /// `accept p : T` — an action that waits for an incoming transfer.
    ///
    /// Same shape of defect as [`Kind::AssignmentActionUsage`]: the word used to be a
    /// [`Kind::ControlFlow`] name, so `validateAcceptActionUsageParameters` could not find one to
    /// count the parameters of.
    AcceptActionUsage,
    /// `send p : T to r` — an action that emits a transfer.
    ///
    /// The specification requires three input parameters (payload, sender, receiver); the first
    /// two come from the library base `SendAction` and the notation writes only the payload.
    SendActionUsage,
    /// `bind a = b` — an equality binding between two features.
    Bind,
    /// A `flow` between two features inside an action or part body.
    Flow,
    /// `flow of fuel : Fuel …` — the feature that flows, a member of the flow. The specification
    /// calls it a `PayloadFeature` and bounds a flow to one (`validateFlowPayloadFeature`).
    PayloadFeature,
    /// `flow name : T from a to b` — one flow, not the definition of one.
    FlowUsage,
    /// `ref x : T` — a reference feature, as opposed to a composite one.
    Ref,
    /// `variant part x : T` / `variant def V` — one alternative of a variability point.
    Variant,
    /// A bare `name : Type;` / `name = expr;` with no kind keyword.
    DefaultReference,
    /// `assert constraint { ... }` — a condition that must hold at this point.
    AssertConstraint,
    /// `objective { ... }` inside a requirement — what the requirement is for.
    Objective,
    /// A KerML type declaration (`datatype Real specializes Complex;`, `class Foo;`,
    /// `struct Bar { ... }`). The standard library is written in KerML, so without this kind its
    /// entire contents — every scalar type and every quantity — lowers to nothing.
    KermlType,
    /// `feature f : T;` — a KerML Feature, the notation KerML models are written in.
    ///
    /// It used to lower to [`Kind::AttributeUsage`], so a KerML feature was judged by the SysML
    /// rules about an attribute: `feature f : T` with `T` a `class` was reported as "an attribute
    /// must be typed by attribute definitions", and a feature owning a composite member broke
    /// "every feature of an attribute must be non-composite". Neither rule is about KerML, and a
    /// model written in KerML has no attribute definitions at all.
    KermlFeature,
    /// `class Foo :> Bar;` — a KerML Class.
    ///
    /// Split out of [`Kind::KermlType`] because the specification states constraints between the
    /// KerML classifiers that cannot be expressed while they share one kind: a Class must not
    /// specialize a DataType, a DataType must not specialize a Class, and an Association that is
    /// also a Structure must be an AssociationStructure. The authored keyword is the only thing
    /// that tells them apart, and the catch-all arm used to discard it.
    ClassDef,
    /// `behavior B :> S;` / `behavior def B;` — a KerML Behavior.
    ///
    /// Like `feature` before it, the keyword lowered to [`Kind::KermlType`] and its whole
    /// family was lost: `validateBehaviorSpecialization` asks whether a Behavior specializes a
    /// Structure, and with no Behavior kind both sides of the question were `KermlType`.
    BehaviorDef,
    /// `function F { 1 }` — a KerML Function.
    ///
    /// `validateFunctionResultExpressionMembership` counts the result expressions a function owns,
    /// and with `function` lowered to `KermlType` no function was a function.
    FunctionDef,
    /// `expr e : T { 1 }` — a KerML Expression.
    ExpressionDef,
    /// `datatype Real specializes Complex;` — a KerML DataType.
    DataTypeDef,
    /// `struct Bar { ... }` — a KerML Structure.
    StructureDef,
    /// `multiplicity m : [1..*];` — a KerML Multiplicity.
    ///
    /// A type may own at most one, and `validateTypeOwnedMultiplicity` is the rule that says so.
    /// It fell into the catch-all with the other KerML classifiers, so the keyword was lost and
    /// an owned multiplicity could not be counted.
    MultiplicityDef,
    /// `assoc A { end x : T; }` / `association B;` — a KerML Association.
    AssocDef,
    /// `assoc struct A` — both an Association and a Structure, with one identity.
    AssocStructureDef,
    /// `allocation def PowerRail;` — a mapping from one structure to another.
    AllocationDef,
    /// `allocation bc : BC1 allocate b to c;` — one allocation, not the definition of one.
    ///
    /// The keyword lowered to [`Kind::AllocationDef`] whether or not `def` followed, so a usage
    /// was indistinguishable from a definition and the rule that requires an allocation to be
    /// typed by an *allocation definition* could not reject `allocation :P allocate b to c;`
    /// (`AllocationUsage_Invalid.sysml.xt:53`).
    AllocationUsage,
    /// `use case def Drive;` / `use case drive : Drive;`.
    UseCaseDef,
    /// `use case name;` — one use case, not the definition of one.
    ///
    /// The lowering mapped the `use case` keyword straight to [`Kind::UseCaseDef`] whether or not
    /// `def` followed, so a model could not tell the two apart and no rule about a use-case
    /// *usage* could find one.
    UseCaseUsage,
    /// `concern def Safety;` — a stakeholder concern a requirement can descend from.
    ConcernDef,
    /// `analysis case def FuelEconomy;` — an analysis case, the container for a calculation.
    AnalysisCaseDef,
    /// `case def Mission;` — a generic case.
    CaseDef,
    /// `dependency A from B;` — a directed dependency.
    Dependency,
    /// `subtype C specializes B;` — a specialization stated on its own, rather than as the
    /// `:>` clause of a declaration (`KerML.xtext:388-398`).
    ///
    /// It relates two types that already exist, so it declares no member: the specific type is
    /// named in `from` and the general type in `to`, the way a dependency keeps its two ends.
    Specialization,
    /// `disjoint a.b from c.d;` — a disjoining stated on its own (`Disjoining`, KerML 1.0
    /// §8.2.3.2). Like [`Kind::Specialization`] it declares no member: the disjoined type is in
    /// `from` and the type it is disjoint from is in `to`.
    Disjoining,
    /// `featuring of M by X;` — a featuring stated on its own (`TypeFeaturing`, `KerML.xtext:651-655`).
    /// Like [`Kind::Disjoining`] it declares no member: the feature it is about is in `to` and the
    /// feature that features it is in `from`.
    TypeFeaturing,
    /// `comment c about x;` — a named comment that says what it is about. Like the other
    /// annotations it is a member of its owner, and its `about` targets are kept as `to`.
    Comment,
    /// `inverse A::f of B::g;` — a feature inverting stated on its own (`FeatureInverting`,
    /// `KerML.xtext:633-642`). The inverted feature is in `from`, the inverting one in `to`.
    Inverting,
    /// `filter @Safety;` — a membership filter on the enclosing namespace.
    Filter,
    CalcDef,
    /// `state def Name { ... }`. A state machine definition. The state's own behaviours
    /// (`entry` / `do` / `exit`) and its outgoing transitions are its children.
    StateDef,
    /// `state name : Def;` — one state in a machine.
    StateUsage,
    /// `first start then running;` / `transition t then s;` — an edge between two states.
    /// `from` is the source, `to` the target; a guard, when written, is kept as the
    /// element's predicate so a checker reads it the same way it reads a requirement's.
    Transition,
    /// `entry` / `do` / `exit` inside a state. The name is the keyword, so a report can say
    /// which of the three a state carries.
    StateAction,
    /// A named initial or final pseudo-state (`first`, `final`) — not a real state, but it is
    /// what transitions point at, so it has to exist for the edges to resolve.
    PseudoState,
    /// `item def Fuel { ... }` — a flow item, the type a connection carries.
    ItemDef,
    /// `item name : T` — one item, not the definition of one.
    ItemUsage,
    /// `enum def Mode { enum safe; enum nominal; }` — the value set a mode or status draws on.
    EnumDef,
    /// One `enum` value inside an `enum def`.
    EnumValue,
    /// `individual def Name :> Type;` — one specific instance's definition.
    ///
    /// The distinction from a plain definition is the point: `part def Wheel` describes any
    /// wheel, `individual def LeftWheelFront :> Wheel` names *this* bolt circle. A model that
    /// conflates them cannot state a fact about one specific asset.
    IndividualDef,
    /// `individual leftWheel : Wheel;` — a usage naming one specific instance.
    ///
    /// The same word introduces both, and the `def` is what separates them: the usage is an
    /// `OccurrenceUsage` with `isIndividual`, the definition an `OccurrenceDefinition`. The
    /// projection used to give both the definition kind, which put the usage outside every rule
    /// that asks about occurrence usages.
    IndividualUsage,
    /// `metadata def Safety { attribute level : Integer; }` — the only annotation mechanism
    /// SysML v2 has. Provenance, decision records and evidence links that live *inside* the
    /// model go through here rather than through a side ledger.
    MetadataDef,
    /// `#Safety { level = 3 }` — an applied metadata annotation.
    MetadataUsage,
    /// `alias Hub for Wheel;` — a second name for an existing element.
    Alias,
    /// `interface def Power { ... }` — a named set of ports two parts agree on.
    InterfaceDef,
    /// `interface Power { ... }` — one use of an interface, not its definition.
    ///
    /// The grammar spells both with `interface` and tells them apart by the `def` word. The
    /// projection used to fold them, which made `interface Power;` a definition: the `interface`
    /// construct is the pair of a definition and a usage, as `part`/`part def` is, and the
    /// folded form answered `is_definition` wrongly for every rule that asks.
    InterfaceUsage,
    /// `view def StructureView;` — a defined view.
    ViewDef,
    /// `view structure : StructureView;` — a view that exists, of a defined kind.
    ///
    /// Kept apart from [`Kind::ViewDef`] because only a usage names a view a consumer can draw:
    /// a definition states what a kind of view is for, a usage is the one in the model.
    ViewUsage,
    /// `occurrence def Flight;` — something that happens in time.
    OccurrenceDef,
    /// `occurrence flight : Flight;` — one occurrence, not the definition of one.
    ///
    /// A bare `snapshot x : T` / `timeslice x : T` is a portion *usage* (`PortionUsage returns
    /// SysML::OccurrenceUsage`, `SysML.xtext:854-861`), which used to lower to an attribute usage
    /// because it writes no kind keyword: the projection lost the fact that the thing happens.
    OccurrenceUsage,
    /// `event arrival : Arrival;` — a usage that references an event it does not own.
    EventOccurrenceUsage,
    /// `calc x : Hover;` — the applied form of a `calc def`.
    CalcUsage,
    /// `viewpoint def Safety;` — what a kind of view is for.
    ViewpointDef,
    /// `viewpoint v : Safety;` — one viewpoint in the model.
    ViewpointUsage,
    /// `connection def PowerLink { ... }` — a reusable connection type.
    ConnectionDef,
    /// `perform action name : Type;` — a part performing a behaviour. This is the statement
    /// that ties structure to behaviour; without it a part model says what a system is made of
    /// and not what it does.
    Perform,
    /// `exhibit state name : Type;` — a part exhibiting a state machine.
    ExhibitState,
    /// `subject d : Drone;` inside a requirement. A declared name, not a member of the
    /// enclosing package: it exists so a predicate has something to be written against.
    Subject,
    /// `frame k : Concern;` inside a requirement — a `FramedConcernMembership`.
    ///
    /// A distinct membership from [`Kind::Subject`], which is why it is a distinct kind: a
    /// `FramedConcernMembership` specializes `RequirementConstraintMembership`
    /// (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:1678`) and is not a
    /// `ParameterMembership`, so a rule counting a type's `SubjectMembership`s must not count it.
    /// Sharing one kind made `subject e : E; frame k : K;` — legal, since `RequirementBodyItem`
    /// admits both (`research/upstream/SysML.xtext:2039-2046`) — report two subjects.
    Frame,
    /// A construct the syntax layer recognised but does not model yet (a state machine, an
    /// action body, a calc). Kept as a named marker instead of being dropped, so a checker can
    /// say "this model contains something the tool did not read" rather than reporting a clean
    /// model. The name carries the construct's keyword so a report can name it.
    Unsupported,
    Import,
    Other(String),
}

impl Kind {
    pub fn keyword(&self) -> &str {
        match self {
            Kind::Package => "package",
            Kind::PartDef => "part def",
            Kind::PartUsage => "part",
            Kind::Actor => "actor",
            Kind::Stakeholder => "stakeholder",
            Kind::Verification => "verify",
            Kind::PortDef => "port def",
            Kind::PortUsage => "port",
            Kind::AttributeDef => "attribute def",
            Kind::AttributeUsage => "attribute",
            Kind::RequirementDef => "requirement def",
            Kind::RequirementUsage => "requirement",
            Kind::VerificationDef => "verification def",
            Kind::VerificationUsage => "verification",
            Kind::Satisfy => "satisfy",
            Kind::Connection => "connection",
            Kind::ActionDef => "action def",
            Kind::ActionUsage => "action",
            Kind::ConstraintDef => "constraint def",
            Kind::ConstraintUsage => "constraint",
            Kind::Expose => "expose",
            Kind::RenderingDef => "rendering def",
            Kind::RenderingUsage => "rendering",
            Kind::Parameter => "parameter",
            Kind::ControlNode => "control node",
            Kind::ControlFlow => "control flow",
            Kind::OccurrenceUsage => "occurrence",
            Kind::EventOccurrenceUsage => "event occurrence",
            Kind::AssignmentActionUsage => "assign",
            Kind::AcceptActionUsage => "accept",
            Kind::SendActionUsage => "send",
            Kind::Bind => "bind",
            Kind::Flow => "flow def",
            Kind::FlowUsage => "flow",
            Kind::PayloadFeature => "payload",
            Kind::Ref => "ref",
            Kind::Variant => "variant",
            Kind::DefaultReference => "reference",
            Kind::AssertConstraint => "assert",
            Kind::Objective => "objective",
            Kind::KermlType => "kerml type",
            Kind::KermlFeature => "feature",
            Kind::ClassDef => "class",
            Kind::BehaviorDef => "behavior",
            Kind::FunctionDef => "function",
            Kind::ExpressionDef => "expr",
            Kind::DataTypeDef => "datatype",
            Kind::StructureDef => "struct",
            Kind::MultiplicityDef => "multiplicity",
            Kind::AssocDef => "association",
            Kind::AssocStructureDef => "assoc struct",
            Kind::AllocationDef => "allocation def",
            Kind::AllocationUsage => "allocation",
            Kind::UseCaseDef => "use case def",
            Kind::UseCaseUsage => "use case",
            Kind::ConcernDef => "concern def",
            Kind::AnalysisCaseDef => "analysis case def",
            Kind::CaseDef => "case def",
            Kind::Dependency => "dependency",
            Kind::Specialization => "specialization",
            Kind::Disjoining => "disjoining",
            Kind::TypeFeaturing => "featuring",
            Kind::Comment => "comment",
            Kind::Inverting => "inverting",
            Kind::Filter => "filter",
            Kind::CalcDef => "calc def",
            Kind::StateDef => "state def",
            Kind::StateUsage => "state",
            Kind::Transition => "transition",
            Kind::StateAction => "state action",
            Kind::PseudoState => "pseudo state",
            Kind::ItemDef => "item def",
            Kind::ItemUsage => "item",
            Kind::EnumDef => "enum def",
            Kind::EnumValue => "enum",
            Kind::IndividualDef => "individual def",
            Kind::IndividualUsage => "individual",
            Kind::MetadataDef => "metadata def",
            Kind::MetadataUsage => "metadata",
            Kind::Alias => "alias",
            Kind::InterfaceDef => "interface def",
            Kind::InterfaceUsage => "interface",
            Kind::ViewDef => "view def",
            Kind::ViewUsage => "view",
            Kind::OccurrenceDef => "occurrence def",
            Kind::CalcUsage => "calc",
            Kind::ViewpointDef => "viewpoint def",
            Kind::ViewpointUsage => "viewpoint",
            Kind::ConnectionDef => "connection def",
            Kind::Perform => "perform",
            Kind::ExhibitState => "exhibit state",
            Kind::Subject => "subject",
            Kind::Frame => "frame",
            Kind::Unsupported => "unsupported",
            Kind::Import => "import",
            Kind::Other(name) => name.as_str(),
        }
    }

    /// A relationship statement (`satisfy R by x`, `connect a to b`) lowers to an element, but
    /// not to a member that owns its name: the element's name is the reference text it was
    /// lowered from, kept for display, and must never shadow or collide with a real member of
    /// the same scope during resolution.
    pub fn is_relationship(&self) -> bool {
        matches!(self, Kind::Satisfy | Kind::Connection)
    }

    /// The keyword for an element of this kind, as the element is written: `concern def C` for a
    /// definition and `concern c : T` for a usage.
    ///
    /// Several constructs share one kind for their definition and their usage (`concern`, `case`,
    /// `analysis`, `interface`), so [`Kind::keyword`] names the definition for both. Callers that
    /// print an element pass whether it is a definition; a usage loses the word `def`.
    pub fn keyword_as(&self, is_definition: bool) -> &str {
        // `analysis` is the one construct whose kind word is longer than its notation: the grammar
        // writes `analysis def` for a definition and `analysis` for a usage (`SysML.xtext:2219-2237`),
        // where the kind word is `analysis case def`.
        if *self == Kind::AnalysisCaseDef {
            return if is_definition {
                "analysis def"
            } else {
                "analysis"
            };
        }
        let word = self.keyword();
        if is_definition {
            word
        } else {
            word.strip_suffix(" def").unwrap_or(word)
        }
    }

    pub fn is_definition(&self) -> bool {
        matches!(
            self,
            Kind::PartDef
                | Kind::PortDef
                | Kind::AttributeDef
                | Kind::RequirementDef
                | Kind::VerificationDef
                | Kind::ActionDef
                | Kind::ConstraintDef
                | Kind::ConstraintUsage
                | Kind::RenderingDef
                | Kind::KermlType
                | Kind::ClassDef
                | Kind::DataTypeDef
                | Kind::StructureDef
                | Kind::MultiplicityDef
                | Kind::AssocDef
                | Kind::AssocStructureDef
                | Kind::UseCaseDef
                | Kind::ConcernDef
                | Kind::AnalysisCaseDef
                | Kind::CaseDef
                | Kind::AllocationDef
                | Kind::CalcDef
                | Kind::StateDef
                | Kind::ItemDef
                | Kind::EnumDef
                | Kind::IndividualDef
                | Kind::MetadataDef
                | Kind::InterfaceDef
                | Kind::ViewDef
                | Kind::OccurrenceDef
        )
    }

    /// Whether an element of this kind is a `Type` — something that owns features as parts of
    /// itself, rather than a namespace that owns members.
    ///
    /// Stated as the negation of the namespace and relationship kinds, because the enum grows a
    /// usage variant far more often than it grows a namespace, and a kind this predicate has not
    /// been told about is a type — the direction a rule can read. `Feature::owningType` is typed
    /// `Type[0..1]` (`KerML.kerml:166`), so a feature declared in a package has no owning type:
    /// there is nothing it could be a part of, and nothing that features it.
    pub fn is_type(&self) -> bool {
        !matches!(
            self,
            Kind::Package
                | Kind::Import
                | Kind::Expose
                | Kind::Filter
                | Kind::Dependency
                | Kind::Specialization
                | Kind::Disjoining
                | Kind::TypeFeaturing
                | Kind::Comment
                | Kind::Inverting
                | Kind::Alias
                | Kind::Unsupported
                | Kind::Other(_)
        )
    }

    /// Whether an element of this kind is a `Feature` — a member owned through a
    /// `FeatureMembership`, which is the membership `Feature::owningFeatureMembership` names.
    ///
    /// Stated as an allowlist so a kind the projection collapses, or has not modelled yet, is
    /// skipped rather than judged. A definition, a relationship, a marker and a `Variant` are not
    /// in it: the first is a `Type` and not a `Feature`, the next two are not features, and a
    /// variant member is owned through a `VariantMembership` (`generated.rs:1772`), not a
    /// `FeatureMembership`.
    pub fn is_feature(&self) -> bool {
        matches!(
            self,
            Kind::PartUsage
                | Kind::PortUsage
                | Kind::ItemUsage
                | Kind::OccurrenceUsage
                | Kind::EventOccurrenceUsage
                | Kind::IndividualUsage
                | Kind::AssignmentActionUsage
                | Kind::AcceptActionUsage
                | Kind::SendActionUsage
                | Kind::AttributeUsage
                | Kind::KermlFeature
                | Kind::Ref
                | Kind::DefaultReference
                | Kind::Parameter
                | Kind::Subject
                | Kind::Actor
                | Kind::Stakeholder
                | Kind::RequirementUsage
                | Kind::VerificationUsage
                | Kind::Verification
                | Kind::ConstraintUsage
                | Kind::ActionUsage
                | Kind::StateUsage
                | Kind::StateAction
                | Kind::Transition
                | Kind::Perform
                | Kind::ExhibitState
                | Kind::CalcUsage
                | Kind::Connection
                | Kind::ControlNode
                | Kind::ControlFlow
                | Kind::Bind
                | Kind::FlowUsage
                | Kind::PayloadFeature
                | Kind::AssertConstraint
                | Kind::Objective
                | Kind::MetadataUsage
        )
    }

    /// Grouping used by views and by the ledger: a small closed set beats matching on
    /// every variant at each call site.
    pub fn group(&self) -> Group {
        match self {
            Kind::Package => Group::Namespace,
            Kind::PartDef | Kind::PartUsage | Kind::Actor | Kind::Stakeholder => Group::Structure,
            Kind::PortDef | Kind::PortUsage | Kind::Connection => Group::Interface,
            Kind::AttributeDef | Kind::AttributeUsage => Group::Data,
            Kind::RequirementDef | Kind::RequirementUsage => Group::Requirement,
            Kind::VerificationDef
            | Kind::VerificationUsage
            | Kind::Verification
            | Kind::Satisfy => Group::Verification,
            Kind::ActionDef
            | Kind::ActionUsage
            | Kind::AssignmentActionUsage
            | Kind::AcceptActionUsage
            | Kind::SendActionUsage => Group::Behavior,
            Kind::StateDef
            | Kind::StateUsage
            | Kind::Transition
            | Kind::StateAction
            | Kind::PseudoState => Group::Behavior,
            Kind::ConstraintDef | Kind::CalcDef => Group::Analysis,
            Kind::ConstraintUsage => Group::Analysis,
            Kind::Expose => Group::Other,
            Kind::RenderingDef | Kind::RenderingUsage => Group::Other,
            Kind::PayloadFeature => Group::Behavior,
            Kind::Parameter
            | Kind::ControlNode
            | Kind::ControlFlow
            | Kind::Bind
            | Kind::Flow
            | Kind::FlowUsage
            | Kind::AssertConstraint
            | Kind::Objective => Group::Behavior,
            // The KerML classifiers sit in the structure family for group purposes: a view draws
            // them as nodes, and `Group::Other` would make them invisible.
            Kind::KermlFeature
            | Kind::Ref
            | Kind::Variant
            | Kind::DefaultReference
            | Kind::KermlType
            | Kind::ClassDef
            | Kind::BehaviorDef
            | Kind::FunctionDef
            | Kind::ExpressionDef
            | Kind::DataTypeDef
            | Kind::StructureDef
            | Kind::AssocDef
            | Kind::AssocStructureDef
            | Kind::MultiplicityDef => Group::Structure,
            Kind::UseCaseDef
            | Kind::UseCaseUsage
            | Kind::ConcernDef
            | Kind::AnalysisCaseDef
            | Kind::CaseDef => Group::Requirement,
            Kind::AllocationDef | Kind::AllocationUsage | Kind::Dependency => Group::Interface,
            // A specialization relates two types that exist elsewhere. It belongs to no view
            // group of its own, and `Group::Other` is where this file puts what the views do not
            // lay out.
            Kind::Specialization
            | Kind::Disjoining
            | Kind::TypeFeaturing
            | Kind::Comment
            | Kind::Inverting => Group::Other,
            Kind::Filter => Group::Namespace,
            Kind::ItemDef | Kind::ItemUsage | Kind::EnumDef | Kind::EnumValue => Group::Data,
            Kind::MetadataDef | Kind::MetadataUsage => Group::Other,
            Kind::Alias => Group::Namespace,
            Kind::InterfaceDef | Kind::InterfaceUsage => Group::Interface,
            Kind::ViewDef | Kind::ViewUsage | Kind::ViewpointUsage => Group::Other,
            Kind::OccurrenceDef
            | Kind::OccurrenceUsage
            | Kind::EventOccurrenceUsage
            | Kind::IndividualUsage
            | Kind::IndividualDef => Group::Structure,
            Kind::CalcUsage => Group::Analysis,
            Kind::ConnectionDef => Group::Interface,
            Kind::ViewpointDef => Group::Other,
            Kind::Perform | Kind::ExhibitState => Group::Behavior,
            Kind::Subject => Group::Requirement,
            Kind::Frame => Group::Requirement,
            Kind::Unsupported => Group::Other,
            Kind::Import | Kind::Other(_) => Group::Other,
        }
    }

    /// Every variant but [`Kind::Other`], so a test can ask the same question of all of them.
    ///
    /// `Other` carries a payload and cannot be built in a constant; the tests that iterate this
    /// list add one themselves. A new variant has to be added here too, and the tests that read the
    /// list — every kind maps to a metaclass the metamodel declares, or to none for one of the five
    /// recorded gaps — are what makes `Kind::metaclasses` total. The length is pinned for the same
    /// reason the ledger's is: a list that silently grew shorter is the failure this project keeps
    /// finding.
    pub const ALL: &'static [Kind] = &[
        Kind::Package,
        Kind::PartDef,
        Kind::PartUsage,
        Kind::Actor,
        Kind::Stakeholder,
        Kind::Verification,
        Kind::PortDef,
        Kind::PortUsage,
        Kind::AttributeDef,
        Kind::AttributeUsage,
        Kind::RequirementDef,
        Kind::RequirementUsage,
        Kind::VerificationDef,
        Kind::VerificationUsage,
        Kind::Satisfy,
        Kind::Connection,
        Kind::ActionDef,
        Kind::ActionUsage,
        Kind::ConstraintDef,
        Kind::ConstraintUsage,
        Kind::Expose,
        Kind::RenderingDef,
        Kind::RenderingUsage,
        Kind::Parameter,
        Kind::ControlNode,
        Kind::ControlFlow,
        Kind::Bind,
        Kind::Flow,
        Kind::PayloadFeature,
        Kind::FlowUsage,
        Kind::Ref,
        Kind::Variant,
        Kind::DefaultReference,
        Kind::AssertConstraint,
        Kind::Objective,
        Kind::KermlType,
        Kind::KermlFeature,
        Kind::BehaviorDef,
        Kind::FunctionDef,
        Kind::ExpressionDef,
        Kind::ClassDef,
        Kind::DataTypeDef,
        Kind::StructureDef,
        Kind::MultiplicityDef,
        Kind::AssocDef,
        Kind::AssocStructureDef,
        Kind::AllocationDef,
        Kind::AllocationUsage,
        Kind::UseCaseDef,
        Kind::UseCaseUsage,
        Kind::ConcernDef,
        Kind::AnalysisCaseDef,
        Kind::CaseDef,
        Kind::Dependency,
        Kind::Specialization,
        Kind::Disjoining,
        Kind::TypeFeaturing,
        Kind::Comment,
        Kind::Inverting,
        Kind::Filter,
        Kind::CalcDef,
        Kind::StateDef,
        Kind::StateUsage,
        Kind::Transition,
        Kind::StateAction,
        Kind::PseudoState,
        Kind::ItemDef,
        Kind::ItemUsage,
        Kind::EnumDef,
        Kind::EnumValue,
        Kind::IndividualDef,
        Kind::IndividualUsage,
        Kind::MetadataDef,
        Kind::MetadataUsage,
        Kind::Alias,
        Kind::InterfaceDef,
        Kind::InterfaceUsage,
        Kind::OccurrenceDef,
        Kind::OccurrenceUsage,
        Kind::EventOccurrenceUsage,
        Kind::AssignmentActionUsage,
        Kind::AcceptActionUsage,
        Kind::SendActionUsage,
        Kind::CalcUsage,
        Kind::ViewpointDef,
        Kind::ConnectionDef,
        Kind::Perform,
        Kind::ExhibitState,
        Kind::Subject,
        Kind::Frame,
        Kind::Unsupported,
        Kind::Import,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Namespace,
    Structure,
    Interface,
    Data,
    Requirement,
    Verification,
    Behavior,
    Analysis,
    Other,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.keyword())
    }
}

/// Direction carried by an `in` / `out` / `inout` prefix. On a port usage the direction is
/// not written at the usage; it comes from the directed item of the port's definition, which
/// is why the view layer reads it through the typing chain rather than off the port itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
    InOut,
}

impl Direction {
    pub fn keyword(self) -> &'static str {
        match self {
            Direction::In => "in",
            Direction::Out => "out",
            Direction::InOut => "inout",
        }
    }
}

/// Visibility prefix as authored: `private` / `protected` / `public`. Absent means the
/// language default, which for an import is `private` (KerML `Import.visibility`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Protected,
    Public,
}

impl Visibility {
    pub fn keyword(self) -> &'static str {
        match self {
            Visibility::Private => "private",
            Visibility::Protected => "protected",
            Visibility::Public => "public",
        }
    }
}

/// Which form of `import` was written. Mirrors the parser's `ImportShape` minus spans.
///
/// The recursive flag covers both spellings that recurse into owned sub-namespaces:
/// `import A::B::**` and `import A::*::**`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportShape {
    /// `import A::B` — one membership of `A`, bound under its simple name.
    Membership { recursive: bool },
    /// `import A::*` — every membership of the namespace `A`, each under its simple name.
    Namespace { recursive: bool },
    /// `import A::[filter]` — a filtered subset. Lowered so the outline can show it; the
    /// resolver binds nothing from it, because a filter's selection is not modelled yet.
    Filter,
}

impl ImportShape {
    /// The suffix that reproduces the authored form, for outline and report output.
    pub fn suffix(self) -> &'static str {
        match self {
            ImportShape::Membership { recursive: false } => "",
            ImportShape::Membership { recursive: true } => "::**",
            ImportShape::Namespace { recursive: false } => "::*",
            ImportShape::Namespace { recursive: true } => "::*::**",
            ImportShape::Filter => "::[filter]",
        }
    }
}

/// A name as it was written inside an expression, kept with the authored spelling so a
/// diagnostic can quote the source rather than a rendered form.
///
/// `text` is the whole dotted path (`engine.fuelRate`); `segments` splits it so a checker can
/// walk the path one member at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameRef {
    pub text: String,
    pub segments: Vec<String>,
    pub line: u32,
}

/// A reference's text as the notation writes it: every segment that is not a plain identifier is
/// quoted again.
///
/// Names are stored unquoted, which is right for resolution — `'vehicle-trailer system'` and the
/// name it declares are one name — and wrong for anything shown or written back. Rendering the
/// stored text made `->reduce '+'` read `->reduce(+)` and `'Start Signal'` read `Start Signal`,
/// neither of which parses.
pub fn quote_name(text: &str) -> String {
    fn plain(segment: &str) -> bool {
        let mut chars = segment.chars();
        matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }
    // Some reference texts carry the end's own notation around the name: a bound
    // (`[2] d`), a conjugation (`~p`) or a reference subsetting (`cd ::> b.c`). Those parts are
    // written as they are and only the names inside are quoted.
    if let Some(close) = text.strip_prefix('[').and_then(|rest| rest.find(']')) {
        let (bound, rest) = text.split_at(close + 2);
        let name = rest.trim_start();
        let gap = &rest[..rest.len() - name.len()];
        return format!("{bound}{gap}{}", quote_name(name));
    }
    if let Some(rest) = text.strip_prefix('~') {
        return format!("~{}", quote_name(rest));
    }
    if let Some((declared, target)) = text.split_once(" ::> ") {
        return format!("{} ::> {}", quote_name(declared), quote_name(target));
    }
    let mut out = String::with_capacity(text.len());
    let mut segment = String::new();
    let mut rest = text;
    let flush = |segment: &mut String, out: &mut String| {
        if !segment.is_empty() {
            if plain(segment) || segment == "*" || segment == "**" {
                out.push_str(segment);
            } else {
                out.push('\'');
                out.push_str(&segment.replace('\'', "\\'"));
                out.push('\'');
            }
            segment.clear();
        }
    };
    while let Some(c) = rest.chars().next() {
        if rest.starts_with("::") {
            flush(&mut segment, &mut out);
            out.push_str("::");
            rest = &rest[2..];
        } else if c == '.' {
            flush(&mut segment, &mut out);
            out.push('.');
            rest = &rest[1..];
        } else {
            segment.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    flush(&mut segment, &mut out);
    out
}

impl NameRef {
    pub fn new(text: impl Into<String>, line: u32) -> Self {
        let text = text.into();
        let segments = text
            .split(['.', ':'])
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Self {
            text,
            segments,
            line,
        }
    }

    /// The last segment, which is what a scope lookup starts from.
    pub fn leaf(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }
}

/// Binary operators this layer keeps distinct. Anything the parser accepts but that carries no
/// meaning here (sequence operators, `->collect`, metadata access) lowers to
/// [`BinaryOp::Other`] or is flattened away, because a checker that guessed at their semantics
/// would report faults it cannot justify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    Eq,
    NotEq,
    And,
    Or,
    Xor,
    Implies,
    /// `[unit]` on a literal, e.g. `20 [min]`. Kept as a distinct node rather than folded into
    /// the literal so a later units layer has a place to hang the exponent vector.
    Unit,
    /// `a ?? b` — `a`, or `b` when `a` is empty.
    NullCoalesce,
    Other,
}

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Rem => "%",
            BinaryOp::Pow => "**",
            BinaryOp::Less => "<",
            BinaryOp::LessEq => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterEq => ">=",
            BinaryOp::Eq => "==",
            BinaryOp::NotEq => "!=",
            BinaryOp::And => "and",
            BinaryOp::Or => "or",
            BinaryOp::Xor => "xor",
            BinaryOp::Implies => "implies",
            BinaryOp::Unit => "[]",
            BinaryOp::NullCoalesce => "??",
            BinaryOp::Other => "?",
        }
    }

    /// True for the operators whose result is a truth value. A requirement's predicate has to
    /// be one of these at the top, or it is not a condition at all.
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinaryOp::Less
                | BinaryOp::LessEq
                | BinaryOp::Greater
                | BinaryOp::GreaterEq
                | BinaryOp::Eq
                | BinaryOp::NotEq
        )
    }

    pub fn is_logical(self) -> bool {
        matches!(
            self,
            BinaryOp::And | BinaryOp::Or | BinaryOp::Xor | BinaryOp::Implies
        )
    }

    /// The dimension an operand must carry for the operation to be well formed. `Add` and the
    /// comparisons require both sides to agree; `Mul` and `Div` combine into something new and
    /// so impose no requirement here — that is the units layer's business, not this one's.
    pub fn requires_same_dimension(self) -> bool {
        self.is_comparison() || matches!(self, BinaryOp::Add | BinaryOp::Sub)
    }
}

/// Which type test was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeCheckKind {
    /// `operand istype Type` — the operand's type conforms to `Type`.
    IsType,
    /// `operand hastype Type` — the operand has `Type` among its actual types.
    HasType,
    /// `operand as Type` — a *cast*, not a test.
    ///
    /// Kept apart from [`TypeCheckKind::IsType`] because the two answer different questions. A test
    /// yields a truth value; a cast yields the operand, read as `Type`. Folding the cast into
    /// `IsType` made `(x as T).member` untypeable: the base came back as `Boolean`, so the member
    /// was looked for in `Boolean` and never found — which silently turned every cast-and-access
    /// into "no such member", or, before the member check existed, into no check at all.
    As,
    /// `operand meta Type` — the operand's metadata, cast to the metaclass `Type`.
    Meta,
    /// `operand @@ Type` — whether the operand's metadata is classified by `Type`.
    MetaTest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Plus,
    Minus,
    Not,
    /// `~x` written where a value belongs.
    ///
    /// The notation spells `~` as a conjugation (`KerML.xtext:336`, `:412`, `:480`, `:730`) and
    /// never as an expression operator, but the pilot's own models write it in a value position
    /// and judge it there, so the tree has to keep it. Dropping the operator left `~3` looking
    /// like the literal `3` beside an anonymous reference.
    Conjugate,
    /// `all T` — every instance of `T`.
    All,
    Other,
}

impl UnaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            UnaryOp::Plus => "+",
            UnaryOp::Minus => "-",
            UnaryOp::Not => "not",
            UnaryOp::Conjugate => "~",
            UnaryOp::All => "all",
            UnaryOp::Other => "?",
        }
    }
}

/// Language-neutral expression tree.
///
/// Deliberately smaller than the parser's `Expression`: this is the shape the checking layers
/// consume. Variants the checker cannot reason about keep their children but lose their
/// operator identity, so a name inside them is still checked even though the operation is not.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Real(String),
    Str(String),
    Bool(bool),
    /// `null` — the null literal (`NullExpression` in the specification).
    ///
    /// Kept rather than folded into `Opaque`: an empty `Opaque` is what a construct the front end
    /// could not read becomes, and `null` is a value the grammar states explicitly. The two
    /// answer differently when a report asks why something is undecided.
    Null,
    /// A bare name or dotted path: `d.endurance`, `SI::kg`.
    ///
    /// A dotted path is kept as one text **only when every segment is a name**. Access whose base is
    /// not a name — `D().b`, `(x as T).b` — is [`Expr::Member`] instead, because the base has to be
    /// resolved as an expression before the member can mean anything.
    Ref(NameRef),
    /// `base.member` where the base is not itself a name.
    ///
    /// `D().b` reads a feature of the constructor's type, and `(x as T).b` reads one of the cast's
    /// type. Flattening either into a dotted string made the resolver answer the wrong question: the
    /// text `D().b` is not a name and resolves to nothing, so legal code was rejected, while the
    /// cast form was dropped from the expression entirely, so an invented member was accepted. This
    /// variant keeps the two halves apart, so the base is resolved first and the member is then
    /// looked for in the base's type.
    Member {
        base: Box<Expr>,
        member: String,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    /// `callee(args)`.
    ///
    /// `arg_names` is parallel to `args`: a named argument writes the feature it redefines
    /// (`f(mass = 1)`), and the rules about invocations are stated over exactly that —
    /// `inp.ownedRedefinition.redefinedFeature->intersection(parameters)`. Dropping the name at
    /// parse time made every argument an anonymous value and left those rules with nothing to
    /// read.
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        arg_names: Vec<Option<String>>,
    },
    /// `base ? then : else`, `if`.
    Conditional {
        test: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    /// A grouping or comma sequence. Kept as a node so the authored bracketing survives.
    Sequence(Vec<Expr>),
    /// `operand istype Type` / `hastype` — a type test.
    ///
    /// Kept as its own node rather than folded into `Opaque` because the checker can answer it:
    /// it resolves both the operand's type and the named type and asks the specialization graph.
    /// Treating it as opaque made `assert constraint { engine istype V6 }` unevaluable, which is
    /// a condition the model very much can settle.
    TypeCheck {
        kind: TypeCheckKind,
        operand: Box<Expr>,
        type_name: NameRef,
    },
    /// A lambda: `->forAll { in w; w.mass > 0 }`.
    ///
    /// The parameter names are bound inside the body, so a reference to one is not a reference
    /// to a model element. Keeping them here lets the name checker skip exactly those names
    /// instead of reporting every lambda parameter as unresolved.
    Lambda {
        params: Vec<String>,
        body: Vec<Expr>,
    },
    /// An expression form this layer does not interpret. Children are still walked.
    /// `base->op(args)` — a sequence operation. The operator name is kept because the
    /// specification constrains it: `select` for a selection, `collect` for a projection.
    Chain {
        op: String,
        base: Box<Expr>,
        args: Vec<Expr>,
        /// Names the braced form binds: `->forAll { in w; ... }` binds `w`.
        params: Vec<String>,
    },
    /// `new T(a, b)` — an instance is created. Kept apart from [`Expr::Call`] because the
    /// rule set asks different questions of it: an invocation supplies arguments to a
    /// behaviour, a constructor allocates an instance of a type.
    ///
    /// `arg_names` is parallel to `args`, for the same reason as on [`Expr::Call`].
    Constructor {
        type_ref: NameRef,
        args: Vec<Expr>,
        arg_names: Vec<Option<String>>,
    },
    Opaque(Vec<Expr>),
}

impl Expr {
    /// Every name reference in the tree, in source order. This is what a checker walks; it does
    /// not need to know which construct the name sat inside.
    ///
    /// Names inside a unit bracket are excluded: `20 [min]` writes a unit identifier, and
    /// `min` is not a model element to be resolved. Whether the unit itself is known is the
    /// units layer's question, not this one's.
    pub fn refs(&self) -> Vec<&NameRef> {
        let mut out = Vec::new();
        self.collect_refs(&mut out);
        out
    }

    fn collect_refs<'a>(&'a self, out: &mut Vec<&'a NameRef>) {
        match self {
            Expr::Ref(name) => out.push(name),
            // The member name is not a reference from where the expression sits: `D().b` resolves
            // `b` inside `D`, not in the enclosing scope. Pushing it here would make every member
            // access a scope-wide name lookup, which is the defect `Expr::Member` exists to remove.
            // Only the base carries references.
            Expr::Member { base, .. } => base.collect_refs(out),
            Expr::Binary { op, left, right } => {
                left.collect_refs(out);
                if *op != BinaryOp::Unit {
                    right.collect_refs(out);
                }
            }
            Expr::Unary { operand, .. } => operand.collect_refs(out),
            Expr::Call { callee, args, .. } => {
                callee.collect_refs(out);
                for arg in args {
                    arg.collect_refs(out);
                }
            }
            Expr::Chain {
                base, args, params, ..
            } => {
                base.collect_refs(out);
                // The braced form binds its parameters: `->forAll { in w; w > 0 }` binds `w`, so
                // `w` is not a reference to a model element. Reporting it as unresolved turned
                // four conforming models into REF002/SEM004 diagnostics the moment the operator
                // name was kept.
                let mut inner = Vec::new();
                for arg in args {
                    arg.collect_refs(&mut inner);
                }
                for r in inner {
                    let leaf = r.segments.first().map(String::as_str).unwrap_or(&r.text);
                    if !params.iter().any(|p| p == leaf) {
                        out.push(r);
                    }
                }
            }
            Expr::Constructor { type_ref, args, .. } => {
                out.push(type_ref);
                for arg in args {
                    arg.collect_refs(out);
                }
            }
            Expr::Conditional {
                test,
                then_expr,
                else_expr,
            } => {
                test.collect_refs(out);
                then_expr.collect_refs(out);
                else_expr.collect_refs(out);
            }
            Expr::TypeCheck { operand, .. } => operand.collect_refs(out),
            Expr::Lambda { params, body } => {
                let mut inner = Vec::new();
                for e in body {
                    e.collect_refs(&mut inner);
                }
                for r in inner {
                    let leaf = r.segments.first().map(String::as_str).unwrap_or(&r.text);
                    if !params.iter().any(|p| p == leaf) {
                        out.push(r);
                    }
                }
            }
            Expr::Sequence(items) | Expr::Opaque(items) => {
                for item in items {
                    item.collect_refs(out);
                }
            }
            Expr::Int(_) | Expr::Real(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => {}
        }
    }

    /// The names an expression instantiates or invokes, in authored order: the type of each
    /// `new T(…)` and the bare callee of each call. An instantiation expression reads its
    /// instantiated type from these names (`validateInstantiationExpressionInstantiatedType`).
    pub fn instantiated_refs(&self) -> Vec<&NameRef> {
        let mut out = Vec::new();
        self.collect_instantiated(&mut out);
        out
    }

    fn collect_instantiated<'a>(&'a self, out: &mut Vec<&'a NameRef>) {
        match self {
            Expr::Constructor { type_ref, args, .. } => {
                out.push(type_ref);
                for arg in args {
                    arg.collect_instantiated(out);
                }
            }
            Expr::Call { callee, args, .. } => {
                match callee.as_ref() {
                    Expr::Ref(name) => out.push(name),
                    other => other.collect_instantiated(out),
                }
                for arg in args {
                    arg.collect_instantiated(out);
                }
            }
            Expr::Member { base, .. } => base.collect_instantiated(out),
            Expr::Binary { left, right, .. } => {
                left.collect_instantiated(out);
                right.collect_instantiated(out);
            }
            Expr::Unary { operand, .. } | Expr::TypeCheck { operand, .. } => {
                operand.collect_instantiated(out)
            }
            Expr::Conditional {
                test,
                then_expr,
                else_expr,
            } => {
                test.collect_instantiated(out);
                then_expr.collect_instantiated(out);
                else_expr.collect_instantiated(out);
            }
            Expr::Sequence(items) | Expr::Opaque(items) => {
                for item in items {
                    item.collect_instantiated(out);
                }
            }
            Expr::Lambda { body, .. } => {
                for e in body {
                    e.collect_instantiated(out);
                }
            }
            Expr::Chain { base, args, .. } => {
                base.collect_instantiated(out);
                for arg in args {
                    arg.collect_instantiated(out);
                }
            }
            Expr::Ref(_)
            | Expr::Int(_)
            | Expr::Real(_)
            | Expr::Str(_)
            | Expr::Bool(_)
            | Expr::Null => {}
        }
    }

    /// Every member access in the tree, as `(base, member)`.
    ///
    /// A `Ref` holds its dotted path as text, so those are not reported here: their segments are
    /// resolved by the path walk. This reports the accesses whose base is an expression —
    /// `new D().b`, `(x as T).b` — because those need the base's *type* before the member means
    /// anything, and no string walk can supply it.
    pub fn member_accesses(&self) -> Vec<(&Expr, &str)> {
        let mut out = Vec::new();
        self.collect_members(&mut out);
        out
    }

    fn collect_members<'a>(&'a self, out: &mut Vec<(&'a Expr, &'a str)>) {
        match self {
            Expr::Member { base, member } => {
                out.push((base, member.as_str()));
                base.collect_members(out);
            }
            Expr::Binary { left, right, .. } => {
                left.collect_members(out);
                right.collect_members(out);
            }
            Expr::Unary { operand, .. } => operand.collect_members(out),
            Expr::Call { callee, args, .. } => {
                callee.collect_members(out);
                for arg in args {
                    arg.collect_members(out);
                }
            }
            Expr::Chain { base, args, .. } => {
                base.collect_members(out);
                for arg in args {
                    arg.collect_members(out);
                }
            }
            Expr::Conditional {
                test,
                then_expr,
                else_expr,
            } => {
                test.collect_members(out);
                then_expr.collect_members(out);
                else_expr.collect_members(out);
            }
            Expr::Constructor { args, .. } => {
                for arg in args {
                    arg.collect_members(out);
                }
            }
            Expr::Sequence(items) => {
                for item in items {
                    item.collect_members(out);
                }
            }
            Expr::TypeCheck { operand, .. } => operand.collect_members(out),
            Expr::Lambda { body, .. } => {
                for item in body {
                    item.collect_members(out);
                }
            }
            Expr::Opaque(items) => {
                for item in items {
                    item.collect_members(out);
                }
            }
            Expr::Int(_)
            | Expr::Real(_)
            | Expr::Str(_)
            | Expr::Bool(_)
            | Expr::Null
            | Expr::Ref(_) => {}
        }
    }

    /// The text of a unit bracket's content (`m/s`, `°C`). A unit is a unit expression, not a name
    /// to quote: the brackets already delimit it, and the unit reader parses this spelling.
    pub fn render_unit(&self) -> String {
        match self {
            Expr::Ref(name) => name.text.clone(),
            other => other.render(),
        }
    }

    pub fn render(&self) -> String {
        match self {
            Expr::Int(v) => v.to_string(),
            Expr::Real(v) => v.clone(),
            Expr::Str(v) => format!("\"{v}\""),
            Expr::Bool(v) => v.to_string(),
            Expr::Null => "null".to_string(),
            Expr::Ref(name) => quote_name(&name.text),
            // Rendered as written (`D().b`, `(x as T).y`): the base already renders to its own
            // source form, and a `(` is added only where the base is a form that would otherwise
            // re-associate — `(x as T).y` without the brackets reads as `(x as T.y)`.
            Expr::Member { base, member } => {
                let inner = base.render();
                let needs_parens = matches!(
                    **base,
                    Expr::Binary { .. } | Expr::Conditional { .. } | Expr::TypeCheck { .. }
                );
                if needs_parens {
                    format!("({inner}).{member}")
                } else {
                    format!("{inner}.{member}")
                }
            }
            Expr::Binary { op, left, right } => {
                if *op == BinaryOp::Unit {
                    // A unit bracket holds a unit expression (`[km/h]`, `[°C]`), not a name to be
                    // quoted: the brackets already delimit it.
                    format!("{} [{}]", left.render(), right.render_unit())
                } else {
                    format!("{} {} {}", left.render(), op.symbol(), right.render())
                }
            }
            Expr::Unary {
                op: UnaryOp::All,
                operand,
            } => format!("all {}", operand.render()),
            Expr::Unary { op, operand } => format!("{}{}", op.symbol(), operand.render()),
            Expr::Call {
                callee,
                args,
                arg_names,
            } => {
                let rendered: Vec<String> = args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| match arg_names.get(i).and_then(Option::as_deref) {
                        Some(name) => format!("{name} = {}", a.render()),
                        None => a.render(),
                    })
                    .collect();
                format!("{}({})", callee.render(), rendered.join(", "))
            }
            Expr::Chain { op, base, args, .. } => {
                let rendered: Vec<String> = args.iter().map(Expr::render).collect();
                format!("{}->{}({})", base.render(), op, rendered.join(", "))
            }
            Expr::Constructor {
                type_ref,
                args,
                arg_names,
            } => {
                let rendered: Vec<String> = args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| match arg_names.get(i).and_then(Option::as_deref) {
                        Some(name) => format!("{name} = {}", a.render()),
                        None => a.render(),
                    })
                    .collect();
                format!("new {}({})", type_ref.text, rendered.join(", "))
            }
            Expr::Conditional {
                test,
                then_expr,
                else_expr,
            } => format!(
                "if {} ? {} else {}",
                test.render(),
                then_expr.render(),
                else_expr.render()
            ),
            Expr::TypeCheck {
                kind,
                operand,
                type_name,
            } => {
                let kw = match kind {
                    TypeCheckKind::IsType => "istype",
                    TypeCheckKind::HasType => "hastype",
                    TypeCheckKind::As => "as",
                    TypeCheckKind::Meta => "meta",
                    TypeCheckKind::MetaTest => "@@",
                };
                format!("{} {} {}", operand.render(), kw, type_name.text)
            }
            Expr::Sequence(items) => {
                let rendered: Vec<String> = items.iter().map(Expr::render).collect();
                format!("({})", rendered.join(", "))
            }
            Expr::Lambda { params, body } => {
                let rendered: Vec<String> = body.iter().map(Expr::render).collect();
                format!("{{ in {}; {} }}", params.join(", "), rendered.join("; "))
            }
            Expr::Opaque(items) => {
                let rendered: Vec<String> = items.iter().map(Expr::render).collect();
                rendered.join(" ")
            }
        }
    }
}

/// Half-open byte range inside one of the buffers a project was loaded from.
///
/// Only the editor uses this. It exists so a change made on the canvas can become a byte edit in
/// the text the person actually maintains, rather than a second copy of the model that has to be
/// kept in step with the first. `file` is the index into the `sources` slice given to
/// `load_project`, and offsets are UTF-8 byte offsets into that buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    pub file: usize,
    pub start: usize,
    pub end: usize,
}

impl TextRange {
    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

#[derive(Debug, Clone, Default)]
pub struct Element {
    /// Pre-order position in the loaded tree, assigned once by the syntax layer. The resolver
    /// and every view index by this, so they do not have to agree on a traversal of their own.
    pub id: usize,
    pub kind: Kind,
    pub name: String,
    pub type_ref: Option<String>,
    /// Every type a usage names, in authored order.
    ///
    /// `part p : A, B` names two, and only the first reached `type_ref`: the rest were dropped
    /// by `target_of`, so a model that stated two types had a tree carrying one. Rules that
    /// count typed supertypes (`...IndividualDefinition`, for one) had nothing to count.
    pub type_refs: Vec<String>,
    /// Supertype named by a specialization clause (`part def X :> Y`), kept because whether two
    /// elements are related through a type hierarchy is the question a `satisfy` check has to
    /// answer. Without it, `part b : B` and `requirement R { subject s : A }` look unrelated
    /// even when B specializes A.
    pub supertype_ref: Option<String>,
    /// The targets written after `:>>` / `redefines`, in authored order.
    ///
    /// Kept apart from `supertypes` because a redefinition is resolved differently: it names a
    /// feature *the owner inherits*, so the lookup must look in the owner's supertypes and not in
    /// the owner itself — where this very declaration lives and would shadow the inherited one.
    pub redefines: Vec<String>,
    /// Whether a metadata usage was written as a `MemberPrefix` of the declaration that follows.
    ///
    /// `@Safety { level = 3 } part def Brake;` is one `FeatureElement` whose `MemberPrefix` is the
    /// annotation (`KerML.xtext:384`), so the annotation belongs to `Brake`. `@B;` ends at its own
    /// `;` and is a member of the body on its own, which SysML §7.27.2 gives to the containing
    /// element. Position cannot distinguish them — both are an annotation followed by a declaration
    /// — so the parser records the terminator and the lowering carries it here.
    pub annotation_leads: bool,
    /// The `subsets` / `subset` targets, in authored order.
    ///
    /// Kept apart from `supertypes` because `Subsetting` has its own constraints in the
    /// specification (`validateSubsettingConstantConformance` and two others) while a plain
    /// `:>` specialization does not. The lowering adds these to `supertypes` as well, so a
    /// subsetting feature still inherits the members of what it subsets.
    pub subsets: Vec<String>,
    /// The type-relationship parts of the declaration head, by operator.
    ///
    /// `datatype M unions A` states that `M`'s values are `A`'s, and the specification constrains
    /// each operator separately. Kept as their own fields rather than folded into `supertypes`: a
    /// union is not a supertype, and treating one as one would put a wrong edge in the subtype
    /// graph.
    pub unions: Vec<String>,
    pub intersects: Vec<String>,
    pub differences: Vec<String>,
    pub disjoints: Vec<String>,
    /// The `~T` / `conjugates T` targets. A type may state at most one.
    pub conjugates: Vec<String>,
    /// The `references T` targets. At most one per feature.
    pub references: Vec<String>,
    /// The `crosses T` targets. At most one per feature.
    pub crosses: Vec<String>,
    /// The `chains a.b` targets — the features a chain passes through.
    pub chains: Vec<String>,
    /// Ends a binary connector stated beyond its two (`bind a = b = c`). A binding is binary, so
    /// these are recorded for the rule to report rather than read as more ends.
    pub extra_ends: Vec<String>,
    /// `accept <payload>` on a transition, as authored. The specification bounds when a trigger
    /// may be written at all, so the text is kept apart from the guard (`predicate`).
    pub trigger_text: Option<String>,
    /// `do <action>` on a transition, as authored.
    pub effect_text: Option<String>,
    /// The feature a message is addressed to: `send ping to ground` records `ground`.
    ///
    /// Kept apart from `to`, which holds the payload. A message with no far end cannot be placed
    /// on a lifeline, and nothing could ask whether the receiver exists.
    pub addressed: Option<String>,
    /// A send's `via` end, as authored: `SendActionUsage::senderArgument`.
    pub sender: Option<String>,
    /// A send's `to` end or an accept's `via` end, as authored: the `receiverArgument` of either.
    pub receiver: Option<String>,
    /// Whether an accept or send wrote its payload. `send via p;`, `send to q;` and `send;` write
    /// none.
    pub has_payload: bool,
    /// The trigger's expression, kept because the rules ask what it *is*: a `when` argument must be
    /// a Boolean-valued feature reference, an `at` argument a time instant. The text above is for
    /// messages; this is what a rule resolves and types.
    pub trigger_expr: Option<Expr>,
    /// The effect's expression, kept for the same reason: a `do` names a perform, and what a
    /// perform references has a rule of its own.
    pub effect_expr: Option<Expr>,
    /// Which word introduced the trigger — `accept`, `at`, `when` or `after`. The keyword decides
    /// which rule the argument has to satisfy.
    pub trigger_kind: Option<String>,
    /// `accept p : T` on a transition: the type of the accepted payload.
    pub trigger_type: Option<String>,
    /// The clause keyword that introduced this member (`require` / `assume`), when one did.
    ///
    /// A named `require constraint c;` and a plain `constraint c { ... }` both lower to
    /// `Kind::ConstraintUsage`, and a rule about who may own the former must not judge the latter.
    pub clause: Option<String>,
    /// `x := 1` — the value is an initial value. Only a variable may carry one.
    pub is_initial: bool,
    /// The value was written with `default` (`default = 42`, `default 42`).
    ///
    /// This is the counterpart of `is_initial` for the ordinary `FeatureValue`. The specification's
    /// only rule that reads it is `validateFeatureValueOverriding`, whose OCL asks whether every
    /// feature a redefinition transitively redefines carries *only default* values
    /// (`...->closure(redefinition.redefinedFeature).valuation->forAll(isDefault)`). Without the flag
    /// `= 42` and `default = 42` are the same element and that rule cannot be evaluated at all.
    pub is_default: bool,
    /// Every supertype written in the specialization clause, in authored order.
    ///
    /// SysML allows a comma list (`part def C :> A, B`). Keeping only the first one made the
    /// second parent invisible: subtype checks rejected a valid `satisfy`, and the view drew no
    /// edge for it. An empty vector means the element writes no specialization clause; a single
    /// entry is the ordinary case. [`Element::supertype_ref`] remains the first entry, so every
    /// existing single-parent caller keeps working.
    /// The targets written after `:>` or `specializes`, in authored order — and nothing else.
    ///
    /// [`Element::supertypes`] is the derived union the member walk uses: it also carries the
    /// `subsets` targets, the `redefines` targets, and the feature a `perform` names. Those four
    /// are different relations that happen to feed the same walk, and each resolves by a
    /// different scope rule, so a reader that needs to check one of them needs it on its own.
    /// This field is the written specialization clause alone.
    pub specializes: Vec<String>,
    /// The derived supertype set: written specializations, subsettings, redefinitions and the
    /// performed feature, unioned for member lookup. Not a record of what was written — see
    /// [`Element::specializes`], [`Element::subsets`], [`Element::redefines`] and
    /// [`Element::performs`] for that.
    pub supertypes: Vec<String>,
    /// Prose from `doc /* ... */`.
    pub doc: Option<String>,
    /// Raw predicate text of `require constraint { ... }`, braces and keyword stripped.
    ///
    /// Kept alongside [`Element::predicate`] because the diagram draws this exact text: the
    /// rendered expression would normalise spacing and change every existing snapshot.
    pub constraint: Option<String>,
    /// The same predicate, parsed. Absent when the source had none, or when the syntax layer
    /// could not build a tree for it — in which case `constraint` still holds the raw text and
    /// the checker reports nothing rather than guessing.
    pub predicate: Option<Expr>,
    /// The line the predicate's own text starts on, when it was written.
    ///
    /// [`Element::line`] is the line the *declaration* starts on, and for a construct whose
    /// predicate sits several lines below it the two differ by as much as the whole statement:
    /// `transition` … `if "test"` … `then` writes its guard on its own line, and a rule about the
    /// guard has to point there. `validateTransitionFeatureMembershipGuardExpression` is exactly
    /// that rule — Xpect asserts it `at "if \"test\""`, which is the guard's line, not the
    /// `transition` keyword's.
    ///
    /// Part of the model rather than of the expression tree because `Expr`'s literals carry no
    /// position and adding one to every variant would touch sixty-odd construction sites across
    /// five crates. The one position a rule needs is the predicate's head.
    pub predicate_line: Option<u32>,
    /// Predicates written with `assume constraint` rather than `require constraint`, conjoined.
    ///
    /// An assumption and a requirement are different claims: a requirement is what the system
    /// must achieve, an assumption is what the model takes as given. Conjoining them into one
    /// predicate made a violated assumption read as a failed requirement, which is the wrong
    /// thing to report to whoever has to fix it. Keeping them apart lets `check` say which of
    /// the two is false; an assumption that does not hold means the model's basis is broken, not
    /// that the design fails.
    pub assumptions: Option<Expr>,
    /// Authored text of the `assume` clauses, the counterpart of [`Element::constraint`].
    pub assumption_text: Option<String>,
    /// A feature's value clause (`attribute mass = 10 [kg]`), parsed. Until this existed the
    /// right-hand side was dropped entirely, so `attribute x : Real = undefinedName / 2`
    /// passed every check.
    pub value_expr: Option<Expr>,
    /// Raw text of the value clause, kept for reports that need to quote what was written.
    pub value_text: Option<String>,
    pub children: Vec<Element>,
    pub line: u32,
    pub direction: Option<Direction>,
    /// For a control node, the keyword that introduced it (`merge`, `decision`, `fork`, `join`).
    ///
    /// The element's own name is the declared name — `merge continue` declares a node called
    /// `continue`, and a succession names it that way — so the kind of node is not recoverable
    /// from `name`. The degree rules ask which kind a node is, and this is the only place that
    /// answer survives.
    pub control_kind: Option<String>,
    /// Source end of a relationship-like element (`satisfy R by drone` -> `drone`).
    pub from: Option<String>,
    /// Target end of a relationship-like element (`satisfy R by drone` -> `R`).
    pub to: Option<String>,
    /// Set on a feature that was copied onto a usage from its type rather than written there.
    /// The reference implementation passes attributes down this way
    /// (boxes_view.py:1310-1336) and ports too (boxes_view.py:1350-1365), so a view has to be
    /// able to tell an inherited feature from an authored one.
    pub inherited: bool,
    /// Set on an import element (`Kind::Import`): which form was written. `None` on every
    /// other kind.
    pub import_shape: Option<ImportShape>,
    /// Set on an import element: the authored visibility prefix. `None` means the language
    /// default, which for an import is private (KerML `Import.visibility`).
    pub visibility: Option<Visibility>,
    /// The element's declaration name as authored, for editing.
    ///
    /// `None` when the syntax layer has no span for it (synthesised elements, or kinds whose
    /// name is not a single token). A rename is applied only when this is present: without the
    /// exact authored range, rewriting a name means guessing which occurrence in the line is the
    /// declaration, and a guess that lands on a reference silently renames the wrong thing.
    pub name_span: Option<TextRange>,
    /// The value clause (`= 360 [W*h]`), operator included, for editing.
    ///
    /// This is the range a person is changing when they type a new number on the canvas, so it
    /// has to cover exactly what the source wrote — replace it with `= <new value>` and the
    /// feature keeps its type, its placement and its ledger trail.
    pub value_span: Option<TextRange>,
    /// Every authored reference this element carries, with its source range, as
    /// `(text, range)`.
    ///
    /// A rename has to rewrite each place a name is used, and the places differ by kind: a typing
    /// clause, both ends of a `satisfy`, a `subject` type. Collecting them here means the rename
    /// path asks one question ("which references point at this element?") instead of knowing the
    /// grammar of every statement that can name one.
    ///
    /// `type_span` remains separate because the view layer treats the type reference specially —
    /// it is what decides a box's label.
    pub ref_spans: Vec<(String, TextRange)>,
    /// The type reference as authored (`Battery` in `part battery : Battery`), for editing.
    ///
    /// A rename has to rewrite every declaration that names the old element, and a text search
    /// cannot tell which `Battery` in a file refers to the renamed definition. This range is
    /// where the parser already made that decision.
    pub type_span: Option<TextRange>,
    /// The `<B>` short name, when the declaration wrote one.
    ///
    /// A short name is a second spelling of the same element and is what references in other
    /// files use: the library declares `attribute <UTC> 'Coordinated Universal Time'`, and a
    /// model writes `:>> source = UTC`. Keeping only the long name left every such reference
    /// unresolved.
    pub short_name: Option<String>,
    /// The feature a `perform` / `exhibit` performs, when it names an existing one.
    ///
    /// `perform vehicleMassTest { ... }` declares a performance of `vehicleMassTest`; the
    /// performance has no type clause of its own, but the feature it performs supplies the
    /// members the body refines (`in part :>> testVehicle`). Without this link the body's names
    /// resolved against the enclosing part and found nothing.
    pub performs: Option<String>,
    /// The authored multiplicity, brackets included (`[4]`, `[0..*]`, `[1..*]`).
    ///
    /// Kept as written rather than reduced to a number: `*` is an unbounded upper limit, and a
    /// model legitimately writes bounds whose value only a solver could settle. The well-formedness
    /// rules read it (`validateFeatureEndMultiplicity`, `validateMultiplicityRangeBoundResultTypes`),
    /// and until this field existed every bound in the corpus was silently dropped.
    pub multiplicity: Option<String>,
    /// The name written for the owned cross feature, when the model gives one.
    ///
    /// `end connectingHub [0..1] ordered part hub : Hub` names the cross feature `connectingHub`
    /// and the end `hub`. The name matters: `device.connectingHub` in a cross subsetting resolves
    /// to it, so reading it as the end's own name both mislabels the end and loses the target.
    pub cross_name: Option<String>,
    /// The bound written between `end` and the kind keyword, which belongs to the owned cross
    /// feature rather than to the end.
    ///
    /// `end [0..1] part hub : Hub` states how many connections an end may take part in; it does
    /// not state the end's own multiplicity, which is 1..1. The 2024-12 pilot release notes say it
    /// outright: "with the introduction of cross features, the multiplicity of end features is now
    /// required to be 1..1… in either case, the multiplicity should be moved to the position of a
    /// cross multiplicity". Kept apart from [`Element::multiplicity`] so that
    /// `validateFeatureEndMultiplicity` reads the end's own bound and does not reject the form the
    /// specification defines.
    ///
    /// No rule reads this field yet. It exists because dropping the authored bound loses a fact the
    /// model stated, and because `end [1..*] feature y` used to lower as two members: one called
    /// `end` and the feature next to it.
    pub cross_multiplicity: Option<String>,
    /// True for a feature declared with the `end` prefix.
    ///
    /// `end` marks a feature as an end of the association it belongs to, which is a different
    /// claim from `in`/`out` (a parameter of a behaviour). The two together are contradictory,
    /// and `validateFeatureEndNoDirection` is the rule that says so.
    pub is_end: bool,
    /// True for a member written `render r : R` — a view rendering membership.
    ///
    /// `render` is a `MemberPrefix` introducing a `ViewRenderingMembership`
    /// (`SysML.xtext:2335-2343`); `rendering r : R` is an ordinary usage member and leaves this
    /// false. Only the first is a rendering a view may state at most one of, so the two cannot be
    /// the same thing on the tree.
    pub is_render: bool,
    /// True for a feature written with no kind keyword: `x : T;` or `in x : T;`.
    ///
    /// The lowering keeps such a feature as an attribute usage, because every rule about values and
    /// units reads that kind. The specification reads it differently: a bare declaration is a
    /// `DefaultReferenceUsage` (`SysML.xtext:630-633`), a reference, so the type it may take is
    /// any classifier and not only a data type. The typing rule reads this flag to tell the two
    /// apart. Only an attribute usage carries it.
    pub is_bare: bool,
    /// The feature modifiers the declaration wrote, as flags.
    ///
    /// These are the words the specification's own constraints are written against
    /// (`isAbstract`, `isDerived`, `isConstant`, `isVariable`, `isReference`, `isComposite`,
    /// `isPortion`, `isVariation`, `isIndividual`). The parser has always read them into
    /// `Prefixes`; the lowering layer then dropped all of them, so a rule asking
    /// "is this abstract" had nothing to read and could not be decided. Keeping them is what
    /// lets those rules run at all.
    pub modifiers: Modifiers,
    /// Effective `isComposite` — whether the feature's values are part of what its owner is made
    /// of, rather than something the owner only refers to.
    ///
    /// [`Modifiers::is_composite`] records only that the word `composite` appeared, and SysML
    /// usage notation has no such word (`SysML.xtext:556-566` is
    /// `direction? 'derived'? ('abstract' | 'variation')? 'constant'? 'ref'?`), so a rule reading
    /// the written word alone decides almost nothing: `part wheel : Wheel;` carries no modifier
    /// and would be neither composite nor referential. The specification derives
    /// `isReference = not isComposite` (`deriveUsageIsReference`, `generated.rs:8957-8961`) and
    /// states six constraints against that value, so the default has to come from the derivation
    /// rather than from the metamodel's `false`. The specification's own
    /// `validatePortDefinitionOwnedUsagesNotComposite` proves the point: it rejects the pilot
    /// model's `part b2 : B;` inside `port def pd1` *because* an owned non-port usage is
    /// composite, so a tree that leaves it non-composite cannot state the violation the
    /// authoritative verdict states.
    ///
    /// The lowering's `fill_feature_defaults` fills this field, and documents the arm behind each
    /// case.
    pub is_composite: bool,
    /// The effective `isReference`: written `ref`, or `true` by definition on a metaclass that
    /// redefines it.
    ///
    /// Two sources and one flag, the same split as [`Element::is_composite`]. The notation sets it
    /// with the `ref` word, and [`Modifiers::is_reference`] keeps only that word so a rule can ask
    /// what the declaration actually wrote. But three metaclasses redefine the property to a
    /// constant `true` with `derived: true` — `AttributeUsage` (`generated.rs:2033`),
    /// `EventOccurrenceUsage` (`:2212`) and `ReferenceUsage` (`:2492`) — and for those the answer
    /// does not depend on the word at all.
    ///
    /// The two must not be conflated. `typed_by_definition` used `modifiers.is_reference` to mean
    /// "the author wrote `ref`", because a reference names a thing that already exists and so is not
    /// bound by the rule that a usage's type must be a definition. Reading the *metaclass* fact in
    /// that position made `event occurrence e : Real;` report as "a reference typed by a data type"
    /// instead of "an occurrence typed by a data type", which is the defect that split produced.
    ///
    /// `fill_feature_defaults` fills this field; [`Modifiers::is_reference`] keeps the authored word.
    pub is_reference: bool,
    /// Which membership introduced this element into its owner.
    ///
    /// The metamodel states its constraints about members through the membership that owns them:
    /// `ownedMemberParameter.direction = parameterDirection()`, `ownedConstraint.isComposite`,
    /// `featureMembership->selectByKind(ReturnParameterMembership)->size()`. The projection used to
    /// keep that fact in three unrelated places — [`Kind::Parameter`] for a parameter,
    /// [`Kind::Objective`] for an objective, [`Element::clause`] for `require` / `return` — so a rule
    /// asking "how was this member introduced" had to know which of the three to look at, and a
    /// rule asking "how many return parameters" had nothing to count.
    ///
    /// One field, one relation: everything a member is introduced by is here, and `Kind` keeps
    /// saying what the member *is*.
    pub membership: Membership,

    /// Whether the declaration was written with `def`.
    ///
    /// `Kind` cannot answer this: an `interface def I` and an `interface i` both lower to
    /// [`Kind::InterfaceDef`], and so do a `case` and a `case def`, a `connection` and a
    /// `connection def`. The reference distinguishes them — "An interface must be typed by
    /// interface definitions" is an error for `interface opposite1 : fi` where `fi` is an
    /// interface usage (`InterfaceUsage_Invalid.sysml.xt:75`) — and this is the field that lets a
    /// rule ask the same question.
    pub is_definition: bool,

    /// Whether the declaration was written `variant …` — one alternative of a variability point.
    ///
    /// `variant` is a membership prefix, not a kind: `variant part car` is a part usage that is a
    /// variant, and its kind says `part`. The flag is what the variant rules read. A `variant`
    /// written with no kind keyword (`variant highPerformanceRequirement;`) keeps
    /// [`Kind::Variant`](Kind::Variant), since there is no inner kind to preserve.
    pub is_variant: bool,

    /// Whether a `library package` was written `standard library package`.
    ///
    /// The kind is the same either way (`Kind::LibraryPackage`), and the reference warns about the
    /// second spelling: a user's library is not the standard one
    /// (`LibraryPackage_invalid_notStandard.kerml.xt:15`).
    pub is_standard: bool,
}

/// The membership that owns a member, as the metamodel names it.
///
/// Only the memberships the notation can write are listed. `Feature` is the default: a feature
/// declared in a type's body is owned through an implicit `FeatureMembership`, which is the
/// ordinary case and the one that carries no keyword.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Membership {
    /// The ordinary case: a feature of a type, owned through a `FeatureMembership`.
    #[default]
    Feature,
    /// `member feature` or an owned crossing feature, not a FeatureMembership.
    Owning,
    /// A `ParameterMembership`, whose `parameterDirection()` is `in`.
    ///
    /// The projection does not build one yet: the notation writes a `ParameterMembership` for a
    /// message payload and for a call argument (`PayloadParameterMember`, `ArgumentExpressionMember`,
    /// `NodeParameterMember`), and a directed feature in a behaviour is a *parameter* by its
    /// direction (`Step::parameter redefines directedFeature`), owned through an ordinary
    /// `FeatureMembership`. Named so the enum is the metamodel's, filled when those elements exist.
    Parameter,
    /// `return x` — a `ReturnParameterMembership`, whose `parameterDirection()` is `out`.
    ReturnParameter,
    /// `objective { … }` — an `ObjectiveMembership`.
    Objective,
    /// `require c` / `assume c` — a `RequirementConstraintMembership`.
    RequirementConstraint,
    /// `frame k : K` — a `FramedConcernMembership`.
    FramedConcern,
    /// `entry` / `do` / `exit` — a `StateSubactionMembership`.
    StateSubaction,
    /// `end x : T` — an `EndFeatureMembership`.
    EndFeature,
    /// `subject s : S` — a `SubjectMembership`.
    Subject,
    /// `actor a` — an `ActorMembership`.
    Actor,
    /// `stakeholder s` — a `StakeholderMembership`.
    Stakeholder,
    /// `variant part v : V` — a `VariantMembership`.
    Variant,
}

/// The feature modifiers a declaration writes before its name.
///
/// `portion` is a two-value word (`snapshot` / `timeslice`) with `composite` as the default kind,
/// so it is kept as a name rather than a flag: a rule asks which kind, not whether one was written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub is_abstract: bool,
    pub is_variation: bool,
    pub is_individual: bool,
    pub is_derived: bool,
    pub is_constant: bool,
    pub is_readonly: bool,
    /// `var` — the feature is a variable, which is what makes `constant` and `portion`
    /// contradictions rather than descriptions.
    pub is_variable: bool,
    pub is_reference: bool,
    /// `all` — the type is sufficient. A `connection def` must state it.
    pub is_sufficient: bool,
    /// `parallel` — a state's substates run concurrently.
    pub is_parallel: bool,
    /// `ordered` — the feature's values have an order.
    pub is_ordered: bool,
    /// `nonunique` — the feature's values may repeat. The default is unique.
    pub is_nonunique: bool,
    /// `composite` written explicitly. The word belongs to KerML notation: a SysML usage states
    /// the same thing by *not* writing `ref`, which is why the effective value lives on
    /// [`Element::is_composite`] and this flag keeps only the word the declaration wrote.
    pub is_composite: bool,
    /// `portion` / `snapshot` / `timeslice`, as authored.
    pub portion: Option<String>,
}

impl Modifiers {
    /// Whether any modifier was written.
    pub fn is_empty(&self) -> bool {
        self == &Modifiers::default()
    }
}

impl Element {
    pub fn new(kind: Kind, name: impl Into<String>, line: u32) -> Self {
        // `..Default::default()` rather than 44 named fields: adding a field used to mean editing
        // this initializer too, and that edit was got wrong five times in one session — a missing
        // field, a duplicated one, twice a field silently deleted. The compiler catches every one,
        // but each catch costs a round trip. Now a new field needs no change here at all.
        Self {
            id: 0,
            kind,
            name: name.into(),
            line,
            ..Default::default()
        }
    }

    pub fn is_relationship(&self) -> bool {
        self.kind.is_relationship()
    }

    /// Whether the feature is referential — `isReference`, which the specification derives as
    /// `not isComposite` for a `Usage` (`deriveUsageIsReference`, `generated.rs:8957-8961`).
    pub fn is_referential(&self) -> bool {
        !self.is_composite
    }

    /// Every supertype this element claims, in authored order.
    ///
    /// Falls back to [`Element::supertype_ref`] for elements built before `supertypes` existed
    /// and for the lowering paths that still set only the single field, so a caller asking
    /// "what does this specialize" never silently sees an empty list.
    pub fn supertype_list(&self) -> Vec<&str> {
        if !self.supertypes.is_empty() {
            return self.supertypes.iter().map(String::as_str).collect();
        }
        self.supertype_ref.as_deref().into_iter().collect()
    }

    /// Record a specialization clause. The first entry also lands in `supertype_ref` so single
    /// parent callers keep their meaning.
    pub fn with_supertypes(mut self, sup: Vec<String>) -> Self {
        self.supertype_ref = sup.first().cloned();
        self.supertypes = sup;
        self
    }

    pub fn with_type(mut self, type_ref: Option<String>) -> Self {
        self.type_ref = type_ref;
        self
    }

    /// Attach the import form and visibility to an element of `Kind::Import`.
    pub fn with_import(mut self, shape: ImportShape, visibility: Option<Visibility>) -> Self {
        self.import_shape = Some(shape);
        self.visibility = visibility;
        self
    }

    /// Total node count including this element.
    pub fn size(&self) -> usize {
        1 + self.children.iter().map(Element::size).sum::<usize>()
    }

    /// Depth-first walk with the element's depth, root being depth 0.
    pub fn walk(&self, depth: usize, f: &mut impl FnMut(&Element, usize)) {
        f(self, depth);
        for child in &self.children {
            child.walk(depth + 1, f);
        }
    }
}
