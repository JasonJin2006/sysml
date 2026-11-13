//! The specification's well-formedness constraints, and how many of them this kernel enforces.
//!
//! SysML v2 states its own validation rules as OCL in the metamodel: 180 `validate*` constraints
//! across 91 metaclasses. The semantic layer here is not that; it is four checks (connection ends,
//! value-expression names, predicates, satisfy typing). Reporting "56/56 models clean" without
//! saying which rules were applied reads as "the language is implemented", which it is not.
//!
//! This module makes the gap measurable. `RULES` is the specification's list, compiled in from
//! `data/spec_rules.json`; `implemented()` names the ones a check actually enforces. The
//! `sysml conformance` command prints the two side by side, so a number in a report can always be
//! traced to the rules behind it.
//!
//! A rule goes in `implemented()` only when a test in `tests/rules_*.rs` shows it rejecting a model
//! that violates it. Listing a rule whose check cannot fail would be the same defect as a silent
//! drop, one level up.

use std::collections::BTreeSet;

use sysml_model::Element;

/// One rule as the specification states it.
#[derive(Debug, Clone)]
pub struct SpecRule {
    pub name: &'static str,
    pub metaclass: &'static str,
    pub ocl: &'static str,
    pub says: &'static str,
}

/// The specification's rules, extracted from the metamodel's OCL.
///
/// The source is the published `sysml-model` crate's `RULES` table: each entry carries the rule's
/// name, the metaclass that owns it, its OCL text, and the sentence the specification writes.
pub const RULES: &[SpecRule] = include!("../data/spec_rules.rs");

/// A metamodel property whose declared type this kernel compares against the values it finds.
///
/// Not every requirement the reference implementation reports is an OCL constraint. A few are the
/// declared type of a derived property — `ConnectionDefinition::connectionEnd` is `Usage[*]`,
/// `FlowDefinition::flowEnd` is `Usage[*]`, `InterfaceDefinition::interfaceEnd` is
/// `PortUsage[*]` (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:2094`, `:2293`,
/// `:2341`) — and the reference reports a value of the wrong metaclass as an error
/// ("A connection definition end must be a usage."). None of those is among the 180, so they are
/// not in [`RULES`] and cannot appear in [`implemented()`]; listing them there would make the rule
/// arithmetic count something the specification does not state as a rule.
///
/// They get this table instead, and their own line in `sysml conformance`, so the 180 stays the
/// 180 and a reader can still see what is checked.
#[derive(Debug, Clone, Copy)]
pub struct PropertyCheck {
    /// The code the report carries.
    pub code: &'static str,
    /// The metaclass the check runs on, as the metamodel names it.
    pub owner: &'static str,
    /// A second metaclass the check runs on, when the reference does.
    ///
    /// `interface bad { end part … }` is an error in `InterfaceUsage_Invalid.sysml.xt:79` under
    /// the definition's property name, though `InterfaceUsage` inherits `connectionEnd : Usage[*]`
    /// rather than declaring `interfaceEnd : PortUsage[*]`. The reference reports the stricter
    /// type for both, so the check runs on both; the property and its message stay the
    /// definition's, which is the wording the reference uses.
    pub also_owner: Option<&'static str>,
    /// The property the value belongs to, as the metamodel names it.
    pub property: &'static str,
    /// What the check asks of the property's values.
    pub constraint: PropertyConstraint,
    /// What the report calls the property's values — "end", "flow end".
    pub noun: &'static str,
    /// Where the rule is stated, as the report cites it. Kept as a sentence rather than assembled
    /// from the fields above because one entry runs on a metaclass that inherits the property
    /// instead of declaring it.
    pub says: &'static str,
    /// Whether the check is skipped when the element writes ends of its own.
    ///
    /// Two models bracket the connection half and they disagree: `abstract connection def B :>
    /// Links::BinaryLink;` is an error with no ends of its own
    /// (`ConnectionUsage_End_Invalid.sysml.xt:33`), while `connection def Causation :>
    /// Occurrences::HappensBefore { end [*] ref cause : Situation; … }` inherits four KerML
    /// features the same way and is a model the corpus states clean
    /// (`14-Language Extensions/14c-Language Extensions.sysml:38`). The difference the two show is
    /// whether the definition writes ends itself, so that is the line this kernel draws. A third
    /// model could move it; nothing in the specification states the narrower rule.
    pub requires_no_own_ends: bool,
}

/// What a property check asks of the property's values.
#[derive(Debug, Clone, Copy)]
pub enum PropertyConstraint {
    /// Every value must be an instance of the named metaclass.
    Each(&'static str),
    /// The property must hold at least this many values.
    ///
    /// A multiplicity, not a constraint: `Relationship::relatedElement` is `Element[2..*]`, no
    /// `validate*` states it, and the reference reports a one-ended `connection { end ::> b0; }`
    /// as "Must have at least two related elements"
    /// (`Relationship_invalid_relatedElement0.sysml.xt:32`). Its neighbour on line 35 is the same
    /// declaration written `abstract`, and the pilot states no error there — an abstract connector
    /// may be incomplete — so the count is asked only of the ones that are not abstract.
    AtLeast(usize),
    /// The property must hold no more than this many values.
    ///
    /// Also a multiplicity the reference enforces rather than states: an interface joins exactly
    /// two things, and `interface def FuelInterface3` with a third `end` is reported as "Cannot
    /// have more than two ends" at that third end (`InterfaceUsage_Invalid.sysml.xt:51`). Nothing
    /// in the 180 says so; `InterfaceDefinition::interfaceEnd` is only `PortUsage[*]`.
    AtMost(usize),
}

/// The property checks this kernel runs, each with the metaclass and the question it asks.
pub const PROPERTY_CHECKS: &[PropertyCheck] = &[
    PropertyCheck {
        code: "SEM096",
        owner: "ConnectionDefinition",
        also_owner: None,
        property: "connectionEnd",
        constraint: PropertyConstraint::Each("Usage"),
        noun: "end",
        says: "`ConnectionDefinition::connectionEnd` is declared `Usage`",
        requires_no_own_ends: true,
    },
    PropertyCheck {
        code: "SEM096",
        owner: "FlowDefinition",
        also_owner: None,
        property: "flowEnd",
        constraint: PropertyConstraint::Each("Usage"),
        noun: "flow end",
        says: "`FlowDefinition::flowEnd` is declared `Usage`",
        requires_no_own_ends: true,
    },
    PropertyCheck {
        code: "SEM096",
        owner: "InterfaceDefinition",
        also_owner: Some("InterfaceUsage"),
        property: "interfaceEnd",
        constraint: PropertyConstraint::Each("PortUsage"),
        noun: "interface end",
        says: "`InterfaceDefinition::interfaceEnd` is declared `PortUsage`",
        requires_no_own_ends: false,
    },
    PropertyCheck {
        code: "SEM096",
        owner: "InterfaceDefinition",
        also_owner: Some("InterfaceUsage"),
        property: "interfaceEnd",
        constraint: PropertyConstraint::AtMost(2),
        noun: "interface end",
        says: "an interface joins two things",
        requires_no_own_ends: false,
    },
    PropertyCheck {
        code: "SEM096",
        owner: "Connector",
        also_owner: None,
        property: "relatedElement",
        constraint: PropertyConstraint::AtLeast(2),
        noun: "related element",
        // Not "`Relationship::relatedElement` holds at least two elements": that names a lower
        // bound the metamodel does not carry. `generated.rs:2495` records the property as
        // `many: true` only, and `FeatureMeta` writes `many` for every property whose upper bound
        // is not 1 (`:2776-2786`) — it cannot distinguish `Element[*]` from `Element[2..*]`. The
        // "at least two" is stated in OCL only for `Connector::relatedFeature`
        // (`validateConnectorRelatedFeatures`), which is what the message cites instead.
        says: "`Connector::relatedFeature` must relate at least two features (validateConnectorRelatedFeatures)",
        requires_no_own_ends: false,
    },
];

// -- rules the specification does not state ----------------------------------

/// One rule this kernel enforces whose authority is **not** a normative OCL constraint.
///
/// Most of what a conforming kernel must reject is written down: 180 `validate*` constraints in the
/// metamodel, each with an OCL body and an English sentence. Those are [`RULES`], and [`implemented`]
/// says which of them run.
///
/// But the reference implementation also rejects models under rules it never cites to an OCL
/// constraint — the "must be typed by <kind> definitions" family across 25 kinds, a handful of
/// derived-property types, and its own positional redefinition of parameters. Those rules are real:
/// the pilot's test models assert them, and a kernel that ignored them would accept models the
/// reference rejects. They are simply not *specification* rules, and putting them in [`implemented`]
/// would inflate the conformance count with work the specification never asked for.
///
/// So they get this ledger. The distinction it preserves is the one that makes the conformance number
/// mean something: 180 are what the specification states, and these are what the reference does.
#[derive(Debug, Clone, Copy)]
pub struct ExtendedRule {
    /// The rule's name, as the reference implementation calls it, or (where it has none) a name
    /// this kernel uses. The `validate*` spelling is kept so a reader can search the reference.
    pub name: &'static str,
    /// The report code the kernel emits for a violation.
    pub code: &'static str,
    /// Where the behaviour is recorded — a pilot model path with line, or a reference source
    /// symbol. Not a specification section, because none states it.
    pub basis: &'static str,
    /// What goes wrong if this rule is wrong. Stated per rule because the answers differ: one rule
    /// rejecting a legal model is a false positive on every use of a legal construct, while another
    /// quietly accepting one is a missed diagnostic nobody notices.
    pub if_wrong: &'static str,
    /// What the kernel does *not* implement of the rule, if anything. Empty when the rule is
    /// complete. This is the field that keeps the ledger honest — a rule half-done is recorded as
    /// half-done rather than counted as covered.
    pub incomplete: &'static str,
}

/// The reference-authority rules this kernel enforces, each with its basis and its risk.
///
/// Every entry was checked against `generated.rs`: the rule's phrasing is absent from the 180. The
/// check for the largest family — "must be typed by <kind> definitions" across all 25 kinds — finds
/// none of them, with `must be typed by` as a positive control.
pub const EXTENDED: &[ExtendedRule] = &[
    ExtendedRule {
        name: "validateUsageType_ (25 kinds)",
        code: "SEM090",
        basis: "reference `SysMLValidator.xtend`, `INVALID_USAGE_TYPE` and its 24 siblings; \
                asserted by 82 pilot assertions over 18 messages, e.g. \
                `PortUsage_Invalid.sysml.xt` on `port p1 : B;`",
        if_wrong:
            "rejects every model that types a usage by the wrong kind of definition — the single \
             most common typing mistake, so a false positive here would be loud; but a false \
             negative accepts `port p1 : Tank;` silently",
        incomplete:
            "the table follows the reference's metaclass checks (`checkAllTypes`, `checkOneType`) \
             for every usage kind the tree types, pinned by `tests/rules_usage_type_metaclass.rs`. \
             Not judged: an `include use case` declaration, which this parser does not read. An \
             untyped usage is accepted, as the reference accepts it; `part` is left to \
             `validatePartUsagePartDefinition`, and a metadata usage to \
             `validateMetadataFeatureMetaclass`",
    },

    ExtendedRule {
        name: "validateSubsettletingConformance (subsetting constant)",
        code: "SEM050",
        basis: "reference `KerMLValidator.xtend`, `INVALID_SUBSETTING_CONSTANT_CONFORMANCE_MSG`; \
                `Subsetting_constant_invalid.kerml.xt:24`",
        if_wrong:
            "a constant feature subsetting a variable one is a contradiction the model cannot \
             satisfy; missing it lets a frozen value claim to track a changing one",
        incomplete: "",
    },
        ExtendedRule {
        name: "deriveUsageMayTimeVary (assignment referent)",
        code: "SEM044",
        basis: "`deriveUsageMayTimeVary` in `generated.rs` is normative and implemented in full; the \
                reference additionally reports it under \
                `validateAssignmentActionUsageReferentIsTimeVarying`, a rule the 180 do not state",
        if_wrong:
            "accepting an assignment to a feature that cannot change leaves a model whose behaviour \
             has no meaning; rejecting a legal one blocks the ordinary case of updating an attribute \
             inside an occurrence",
        incomplete: "",
    },
    ExtendedRule {
        name: "validateNamespaceDistinguishability (inherited half)",
        code: "SEM091",
        basis: "the specification states `validateNamespaceDistinguishibility`, and \
                `deriveTypeInheritedMembership` derives `inheritedMembership` — so the rule itself \
                rests on stated derivations (`generated.rs`, both entries). The reference reports it \
                in three messages (`KerMLValidator.xtend:132-136`), of which `MSG_2` (\"Duplicate of \
                inherited member name\") is the inherited branch at `:441-448`: only for a `Type`, \
                against `namesp.inheritedMembership`, and again for inherited-versus-inherited",
        if_wrong:
            "missing implicit subsetting or positional end redefinition hides distinct inherited \
             providers; replacing a direct illegal typing target by its final classifier instead \
             changes that target's identity and selects the wrong default relationship",
        incomplete:
            "declaration-site conflicts include anonymous flow and end namespaces. Attribute defaults \
             use the direct typing target's metaclass, and binary interfaces contribute positional \
             end redefinitions independently of typing validity. An anonymous `perform`, `exhibit` \
             or reference-form `event` is checked as the namespace it is, against the feature it \
             names, and a feature chain it writes as one more; a feature chain written anywhere \
             else is not yet a namespace of its own",
    },
    ExtendedRule {
        name: "validateFeatureParameterRedefinition (positional)",
        code: "SEM081",
        basis: "reference `FeatureAdapter.addRedefinitions` (`:634-651`), which pairs a feature with \
                the feature in the *same index* of each general type's relevant-feature list — its own \
                doc comment says so (`:629-632`) and `:638-643` does the index arithmetic \
                (`indexOf(target)` then `features.get(i)`). For a parameter that list is \
                `TypeUtil.getAllParametersOf(type)` (`FeatureAdapter.getRelevantParameters`, `:728-733`, \
                tagged `@satisfies checkFeatureParameterRedefinition` at `:726`). The specification \
                states no counterpart: `deriveFeatureOwnedRedefinition` is exactly \
                `ownedSubsetting->selectByKind(Redefinition)`, no stated derivation builds a redefinition \
                from an index, and the two `validate*ParameterRedefinition` rules it does state are about \
                `InvocationExpression`, a different metaclass",
        if_wrong:
            "the reference pairs a behaviour's parameters with its general type's parameters by \
             position, which is architecture rather than a stated constraint; adding it would apply \
             a non-normative rule to every behaviour in every model",
        incomplete:
            "`Index::positional_parameter_targets` pairs a behavior's parameters with its general \
             type's, ignoring result parameters the way `isIgnoredParameter` does, and a result \
             parameter with the general's result. The pairing is reported through the direction rule \
             (`SEM081`) and feeds member lookup. Not emitted: the reference's own \
             `checkFeatureParameterRedefinition` message. Its text is not in this repository — the \
             upstream tree here carries the model crate and the two `.xtext` grammars, not \
             `FeatureAdapter.java` — so writing it would be a guess at the reference's wording. \
             Constructor-result features are not paired",
    },
    ExtendedRule {
        name: "ShadowingTests name resolution (import versus enclosing member)",
        code: "—",
        basis: "`inner` imports `OuterPackage::*`, so the `A` that `classifier B specializes A` names is \
                the imported `OuterPackage::A`, which has no `a2`. `a2` inside `B` is therefore \
                unresolved, as the reference reports it at \
                `ShadowingTests_ImportAndInnerClassesNamesAreTheSameGoodCase3.kerml.xt:28` and its `_FT` \
                twin at line 30. Both setups load `/src/DependencyOuterPackage.kerml`, vendored from \
                upstream into `research/pilot/models/src` with its git blob id checked. An earlier reading \
                of this entry, that the kernel ignores the import, was measured without that file",
        if_wrong:
            "if an enclosing `A` won over the import, `a2` would resolve through `test::A`, both \
             assertions would go unmatched, and the kernel would accept a model the reference rejects",
        incomplete: "",
    },
    ExtendedRule {
        name: "validateFeatureCrossingSpecialization",
        code: "SEM067",
        basis: "SysML specification section 7.13.2 prohibits an end's owned cross feature from \
                explicitly cross-subsetting a different unowned cross feature; the comparison is \
                between the two Feature identities, not their names or types",
        if_wrong:
            "a different crossing feature with the same type could be accepted, or an import that \
             resolves back to the owned feature could be rejected if identity were approximated by \
             spelling or ownership structure",
        incomplete:
            "named and anonymous prefix crossing features and body `member feature` are represented; \
             full owned-crossing declaration grammar and implicit inherited crossing chains beyond \
             the explicit-target case remain outside the current element projection",
    },
    ExtendedRule {
        name: "validateBindingConnectorTypeConformance",
        code: "SEM060",
        basis: "BindingConnector's normative same-values semantics; the check is absent from the \
                180 validation OCL rules. Reference `KerMLValidator.xtend:932-973` implements it \
                despite an obsolete TODO comment, including bindings implicit in expressions",
        if_wrong:
            "unrelated types on explicit binding ends or implicit invocation argument bindings \
             receive a warning; `BindingConnector_Invalid2.sysml.xt:43` exercises the latter with \
             `rearWheel+1`, whose left argument cannot bind to DataFunctions::'+'::x: DataValue",
        incomplete:
            "explicit binding ends and arithmetic/comparison/equality/unary operator arguments are \
             checked against resolved library parameter types. Not checked: unit application \
             (`20 [km]`), short-circuit control operators, and the other implicit binding shapes. \
             Each has a binding shape of its own, and implementing it means matching what \
             `KerMLValidator.xtend:932-973` does for that shape; that file is not in this \
             repository, so a check here would be a guess at the reference's behaviour. Unknown \
             operand or parameter types remain silent rather than being treated as incompatible",
    },
    ExtendedRule {
        name: "validateConnectorTypeFeaturing, validateConnectorRelatedFeatures",
        code: "REF001, SEM001, SEM096",
        basis: "reference `KerMLValidator.xtend:269,271`, messages \
                `INVALID_CONNECTOR_RELATED_FEATURES_MSG` (\"Must have at least two related \
                elements\") and `INVALID_CONNECTOR_TYPE_FEATURING_MSG` (\"Must be an accessible \
                feature (use dot notation for nesting)\"). Absent from the specification's 180 under \
                every name searched; the messages are shared verbatim with \
                `validateSubsettingFeaturingTypes`, which is a stated rule and is enforced here",
        if_wrong:
            "these are the reference's own connector checks, not stated constraints; the risk is \
             that a model whose flow ends are wrong in the way these describe gets fewer \
             diagnostics than the reference gives it",
        incomplete:
            "connection, binding and flow ends that are not accessible are reported under \
             `REF001` with the reference's wording, pinned by `tests/rules_connector_featuring.rs`; \
             a flow end reached with `::` also draws the reference's `Cannot identify flow end` \
             (`validateFlowEndSubsetting`), as `Relationship_invalid_relatedElement1.sysml.xt:47-54` \
             asserts. A flow's arity counts only the ends that resolve (`SEM001`), and a \
             connection's is the property check (`SEM096`). Not covered: the ends of a KerML \
             `connector` declaration and of a succession are not checked for accessibility, and an \
             n-ary connection (`connect (a, b, c)`) is a syntax error in this parser",
    },
];

/// The extended rules, by count and by how many are complete.
///
/// Reported beside the 180 rather than inside it, so the two numbers cannot be added together by
/// mistake.
#[derive(Debug, Clone, Copy)]
pub struct ExtendedCoverage {
    pub total: usize,
    pub complete: usize,
    pub incomplete: usize,
}

pub fn extended_coverage() -> ExtendedCoverage {
    let incomplete = EXTENDED.iter().filter(|r| !r.incomplete.is_empty()).count();
    ExtendedCoverage {
        total: EXTENDED.len(),
        complete: EXTENDED.len() - incomplete,
        incomplete,
    }
}

/// What a check enforces, named by the specification's own rule names.
///
/// A diagnostic is listed only when it decides a rule the specification actually states. Name
/// resolution (`SEM003`, `SEM004`) has no counterpart in the rule table — the specification
/// assumes names resolve and states no constraint about it — so it is not listed here. Listing it
/// would claim coverage of a rule that does not exist.
pub fn implemented() -> &'static [(&'static str, &'static str)] {
    &[
        // connection ends must name features of the right sort
        ("SEM001", "validateConnectorRelatedFeatures"),
        ("SEM002", "validateConnectorRelatedFeatures"),
        (
            "SEM002",
            "validateFeatureReferenceExpressionReferentIsFeature",
        ),
        // a bound must be a non-negative whole number, and a range must not be inverted
        ("SEM013", "validateMultiplicityRangeBounds"),
        ("SEM013", "validateMultiplicityRangeBoundResultTypes"),
        // an `end` feature carries no direction
        ("SEM014", "validateFeatureEndNoDirection"),
        // `var` is a variable; `constant` and `portion` contradict it
        ("SEM034", "validateFeatureEndIsConstant"),
        ("SEM034", "validateFeatureConstantIsVariable"),
        ("SEM034", "validateFeaturePortionNotVariable"),
        ("SEM034", "validateRedefinitionEndConformance"),
        // a variable may not subset a constant without itself being constant
        ("SEM036", "validateSubsettingConstantConformance"),
        // a subsetted feature must be featured within something the subsetting feature reaches
        ("SEM036", "validateSubsettingFeaturingTypes"),
        // a membership's owner must be the metaclass its rule names
        ("SEM038", "validateObjectiveMembershipOwningType"),
        ("SEM038", "validateSubjectMembershipOwningType"),
        // a control node lives in an action
        ("SEM040", "validateControlNodeOwningType"),
        ("SEM040", "validateControlNodeIsComposite"),
        // an end of an association names exactly one type
        ("SEM041", "validateAssociationEndTypes"),
        // a view states at most one rendering
        ("SEM043", "validateViewUsageOnlyOneViewRendering"),
        ("SEM043", "validateViewDefinitionOnlyOneViewRendering"),
        ("SEM043", "validateViewRenderingMembershipOwningType"),
        // a perform names an action and an exhibit a state
        ("SEM044", "validatePerformActionUsageReference"),
        ("SEM044", "validateAssignmentActionUsageReferent"),
        ("SEM044", "validateAssignmentActionUsage"),
        ("SEM044", "validateEventOccurrenceUsageReference"),
        ("SEM044", "validateAssertConstraintUsageReference"),
        ("SEM044", "validateExhibitStateUsageReference"),
        // a metadata usage names a metadata definition
        ("SEM045", "validateMetadataFeatureMetaclass"),
        ("SEM045", "validateMetadataFeatureBody"),
        // a type's unions / intersects / differences clause
        ("SEM046", "validateTypeUnioningTypesNotSelf"),
        ("SEM046", "validateTypeIntersectingTypesNotSelf"),
        ("SEM046", "validateTypeDifferencingTypesNotSelf"),
        ("SEM046", "validateTypeOwnedUnioningNotOne"),
        ("SEM046", "validateTypeOwnedIntersectingNotOne"),
        ("SEM046", "validateTypeOwnedDifferencingNotOne"),
        // how many successions may meet at a control node
        ("SEM047", "validateDecisionNodeIncomingSuccessions"),
        ("SEM047", "validateForkNodeIncomingSuccessions"),
        ("SEM047", "validateMergeNodeOutgoingSuccessions"),
        ("SEM047", "validateJoinNodeOutgoingSuccessions"),
        ("SEM047", "validateDecisionNodeOutgoingSuccessions"),
        ("SEM047", "validateMergeNodeIncomingSuccessions"),
        ("SEM047", "validateControlNodeIncomingSuccessions"),
        ("SEM047", "validateControlNodeOutgoingSuccessions"),
        // a variation may not specialize another variation
        ("SEM048", "validateDefinitionVariationSpecialization"),
        ("SEM048", "validateUsageVariationSpecialization"),
        // a concrete association relates at least two types
        ("SEM049", "validateAssociationRelatedTypes"),
        // a flow definition has at most two ends
        ("SEM050", "validateFlowDefinitionFlowEnds"),
        // entry / do / exit belong to a state
        ("SEM051", "validateStateSubactionMembershipOwningType"),
        // a variant belongs to a variation
        ("SEM054", "validateVariantMembershipOwningNamespace"),
        // who may be an actor or a stakeholder
        ("SEM055", "validateActorMembershipOwningType"),
        ("SEM055", "validateStakeholderMembershipOwningType"),
        // at most one reference and one cross subsetting per feature
        ("SEM056", "validateFeatureOwnedReferenceSubsetting"),
        ("SEM056", "validateFeatureOwnedCrossSubsetting"),
        // a type has at most one conjugator
        ("SEM057", "validateTypeAtMostOneConjugator"),
        // a function states at most one result
        ("SEM058", "validateExpressionResultExpressionMembership"),
        ("SEM058", "validateFunctionResultExpressionMembership"),
        // and owns at most one return parameter
        ("SEM058", "validateExpressionResultParameterMembership"),
        ("SEM058", "validateFunctionResultParameterMembership"),
        // a return parameter is `out`
        ("SEM058", "validateParameterMembershipParameterDirection"),
        // a parallel state does not sequence its substates
        ("SEM059", "validateStateDefinitionParallelSubactions"),
        ("SEM059", "validateStateUsageParallelSubactions"),
        // a binding joins exactly two features
        ("SEM060", "validateBindingConnectorIsBinary"),
        // a trigger belongs to a transition out of a state
        ("SEM061", "validateTransitionUsageTriggerActions"),
        // a transition's guard is a Boolean, and its `when`/`at` argument is what the keyword says
        (
            "SEM061",
            "validateTransitionFeatureMembershipGuardExpression",
        ),
        ("SEM061", "validateTriggerInvocationExpressionWhenArgument"),
        ("SEM061", "validateTriggerInvocationExpressionAtArgument"),
        // an `after` waits for a duration: its unit specializes `ISQBase::DurationUnit`
        ("SEM061", "validateTriggerInvocationExpressionAfterArgument"),
        // a filter selects with a predicate, and the predicate is a constant
        (
            "SEM087",
            "validateElementFilterMembershipConditionIsBoolean",
        ),
        (
            "SEM087",
            "validateElementFilterMembershipConditionIsModelLevelEvaluable",
        ),
        // a require / assume clause belongs to a requirement
        (
            "SEM062",
            "validateRequirementConstraintMembershipOwningType",
        ),
        // a type may not specialize a conjugated type
        ("SEM063", "validateSpecificationSpecificNotConjugated"),
        // a subset may not drop uniqueness its target has
        ("SEM064", "validateSubsettingUniquenessConformance"),
        // a metadata usage may not name an abstract definition
        ("SEM065", "validateMetadataFeatureMetaclassNotAbstract"),
        // a port's non-port members are not composite
        ("SEM066", "validatePortDefinitionOwnedUsagesNotComposite"),
        ("SEM066", "validatePortUsageNestedUsagesNotComposite"),
        // a feature that crosses another is an end
        ("SEM067", "validateCrossSubsettingCrossingFeature"),
        ("SEM067", "validateCrossSubsettingCrossedFeature"),
        // an annotation may only describe what its definition says it describes
        ("SEM094", "validateMetadataFeatureAnnotatedElement"),
        // a chained access reads a feature the base provides
        ("SEM095", "validateFeatureChainExpressionConformance"),
        // an argument redefines exactly one parameter of what is invoked, and no two the same
        (
            "SEM095",
            "validateInvocationExpressionParameterRedefinition",
        ),
        (
            "SEM095",
            "validateInvocationExpressionNoDuplicateParameterRedefinition",
        ),
        (
            "SEM095",
            "validateConstructorExpressionNoDuplicateFeatureRedefinition",
        ),
        // a verification belongs to the objective of a case
        (
            "SEM068",
            "validateRequirementVerificationMembershipOwningType",
        ),
        // a result membership belongs to a function or a case
        ("SEM069", "validateReturnParameterMembershipOwningType"),
        ("SEM069", "validateResultExpressionMembershipOwningType"),
        // a variation's members are all variants
        (
            "SEM071",
            "validateDefinitionVariationOwnedFeatureMembership",
        ),
        ("SEM071", "validateUsageVariationOwnedFeatureMembership"),
        // an occurrence usage names at most one individual definition
        ("SEM072", "validateOccurrenceUsageIndividualDefinition"),
        // a feature and the feature it crosses share a type
        ("SEM073", "validateFeatureCrossFeatureType"),
        ("SEM073", "validateFeatureCrossFeatureSpecialization"),
        // a redefinition lives where its target does, or below it
        ("SEM075", "validateRedefinitionFeaturingTypes"),
        // `include` names a use case
        ("SEM076", "validateIncludeUseCaseUsageReference"),
        // `var` needs an owner with a lifetime
        ("SEM077", "validateFeatureIsVariable"),
        // `:=` assigns, so what it assigns to has to be a variable; and a value over a redefinition
        // may only restate a default
        ("SEM077", "validateFeatureValueIsInitial"),
        ("SEM077", "validateFeatureValueOverriding"),
        // a link with more than two ends is not a binary link
        ("SEM078", "validateAssociationBinarySpecialization"),
        ("SEM078", "validateConnectorBinarySpecialization"),
        // a part's type must be an occurrence definition
        ("SEM079", "validatePartUsagePartDefinition"),
        // a redefinition keeps its target's direction
        ("SEM081", "validateRedefinitionDirectionConformance"),
        // a chain passes through at least two features
        ("SEM082", "validateFeatureChainingFeatureNotOne"),
        ("SEM082", "validateFeatureChainingFeaturesNotSelf"),
        ("SEM082", "validateFeatureChainingFeatureConformance"),
        // a call names something callable
        ("SEM083", "validateInvocationExpressionInstantiatedType"),
        // a behaviour and a structure may not specialize each other
        ("SEM035", "validateBehaviorSpecialization"),
        ("SEM035", "validateStructureSpecialization"),
        ("SEM035", "validateClassSpecialization"),
        ("SEM035", "validateDataTypeSpecialization"),
        // a case or a requirement declares at most one subject, and the subject is its first input
        ("SEM020", "validateCaseUsageOnlyOneSubject"),
        ("SEM020", "validateCaseDefinitionOnlyOneSubject"),
        ("SEM020", "validateRequirementUsageOnlyOneSubject"),
        ("SEM020", "validateRequirementDefinitionOnlyOneSubject"),
        ("SEM020", "validateCaseUsageSubjectParameterPosition"),
        ("SEM020", "validateCaseDefinitionSubjectParameterPosition"),
        ("SEM020", "validateRequirementUsageSubjectParameterPosition"),
        (
            "SEM020",
            "validateRequirementDefinitionSubjectParameterPosition",
        ),
        ("SEM020", "validateCaseUsageOnlyOneObjective"),
        ("SEM020", "validateCaseDefinitionOnlyOneObjective"),
        // an `end` feature states no bound other than 1..1
        //
        // The same rule is enforced twice: `rules::feature_multiplicity` reports it under SEM021
        // and `rules::type_multiplicity` under SEM033, with the same sentence. Both checks run, so
        // a violating model carries two diagnostics until one of the two codes is retired; the
        // duplication is recorded rather than hidden, and no legal model gains a diagnostic from
        // it.
        ("SEM021", "validateFeatureEndMultiplicity"),
        ("SEM033", "validateFeatureEndMultiplicity"),
        ("SEM033", "validateTypeOwnedMultiplicity"),
        ("SEM119", "validateFeatureMultiplicityDomain"),
        ("SEM120", "validateClassifierMultiplicityDomain"),
        ("SEM121", "validateAnnotationAnnotatedElementOwnership"),
        ("SEM122", "validateInstantiationExpressionInstantiatedType"),
        ("SEM123", "validateSendActionParameters"),
        // one entry, one do and one exit per state; a transition ends at an action usage
        ("SEM023", "validateStateUsageStateSubactionKind"),
        ("SEM023", "validateStateDefinitionStateSubactionKind"),
        ("SEM023", "validateTransitionUsageSuccession"),
        // an expose is owned by a view; an import at the root of the model is private
        ("SEM025", "validateExposeOwningNamespace"),
        ("SEM025", "validateImportTopLevelVisibility"),
        // an `end` carries no derived, abstract, composite or portion modifier; an individual
        // occurrence usage names an individual definition; a portion kind is owned by an
        // occurrence type
        (
            "SEM030",
            "validateFeatureEndNotDerivedAbstractCompositeOrPortion",
        ),
        ("SEM030", "validateOccurrenceUsageIndividualUsage"),
        ("SEM030", "validateOccurrenceUsagePortionKind"),
        // a port or an attribute is referential, and an attribute owns no composite feature
        ("SEM031", "validateUsageIsReferential"),
        ("SEM031", "validatePortUsageIsReference"),
        ("SEM031", "validateAttributeUsageIsReference"),
        ("SEM031", "validateAttributeUsageFeatures"),
        ("SEM031", "validateAttributeDefinitionFeatures"),
        // an actor, a stakeholder and a satisfy name what their rule says they name
        ("SEM055", "validateSatisfyRequirementUsageReference"),
    ]
}

/// Rules the specification states that cannot reject any model, with the reason each one cannot.
///
/// A rule is listed here only when the conclusion follows from the specification's own derivation
/// or from the notation it fixes — not when a check is merely missing. Those stay out of both the
/// numerator and the denominator, which keeps them visible as outstanding work. Each reason cites
/// the declaration or sentence that decides the case.
///
/// This is the third state of the ledger: `implemented()` is *enforced*, this list is
/// *not applicable*, and a rule in neither is *blocked* — waiting on a metaclass or a derived
/// property, which is what the next layer builds.
pub const NOT_APPLICABLE: &[(&str, &str)] = &[
    (
        "validateEnumerationDefinitionIsVariation",
        "`EnumerationDefinition` declares `isVariation` with `default: Some(true)` \
         (sysmlv2-model `generated.rs:2203-2206`), and the notation cannot write it false. The \
         enumeration definition takes no prefix (`SysML.xtext:767-769`), and the parser refuses \
         `abstract` and `variation` before `enum def` (`sysml-parser/src/parser.rs`, the enumeration \
         check), so every enumeration definition the kernel reads is a variation by the metamodel. \
         The tree does not carry that default: `is_variation` is false on enumeration definitions, \
         as `tests/rules_variation_rules.rs` records, so a check read against the tree would reject \
         every one. Filling the default is the step a future check needs first.",
    ),
    (
        "validateConnectionDefinitionIsSufficient",
        "The OCL is the bare expression `isSufficient`, and `ConnectionDefinition` redefines that \
         flag to a constant `true` in the specification's own metaclass (sysmlv2-model \
         `generated.rs:2093-2096`, `default: Some(true)`). No writable model can make it false: the \
         KerML spelling `all` sets it true, and no SysML production writes it. The kernel fills it for \
         `Kind::ConnectionDef` (`lower.rs:2552`), so a `connection def` presents as sufficient without \
         `all`. It does not fill it for `interface def` or `allocation def`, which are connection \
         definitions too (`generated.rs:1610`, `:1686`). Those read false in the tree: a gap for a \
         future check over those kinds, but not a source of violations. The rule is inapplicable, not \
         unimplemented, and `tests/rules_connection_sufficient.rs` checks both directions.",
    ),
    (
        "validateExposeVisibility",
        "`SysML.xtext:2366-2372` gives the expose prefix one word and one value: \
         `enum ExposeVisibilityKind returns SysML::VisibilityKind : protected = 'expose';`. So \
         `visibility = VisibilityKind::protected` is the only meaning the notation has, and `public \
         expose` or `private expose` are not spellings. The parser refuses a visibility keyword before \
         `expose` (`sysml-parser/src/parser.rs`, the `expose` branch; `tests/grammar.rs`). Before that \
         refusal it accepted `public expose` and dropped the visibility, which left the rule nothing to read.",
    ),
    (
        "validateEndFeatureMembershipIsEnd",
        "An `EndFeatureMembership` is created where the notation writes an end. Six SysML productions \
         and two KerML productions return it (`SysML.xtext:883, 891, 994, 1085, 1178, 1314`; \
         `KerML.xtext:850, 1031`), and none writes the word `end`. The reference's end post-processor \
         sets `isEnd` on each such member (`EndFeatureMembershipParserPostProcessor.java:35`). This \
         kernel sets it where it builds the end membership (`sysml-syntax/src/lower.rs:583`). The \
         metaclass does not fix the value: `Feature::isEnd` defaults to false (`generated.rs:2235`). The \
         rule therefore holds because the construction that creates the membership also sets the flag, \
         which is a construction fact rather than a metaclass fact.",
    ),
    (
        "validateExposeIsImportAll",
        "`Expose` declares `isImportAll` with `default: Some(true)` (sysmlv2-model \
         `generated.rs:2217-2220`), and `ExposePrefix` (`SysML.xtext:2366-2368`) writes no \
         `isImportAll` of its own: an `expose` imports all of what it names.",
    ),
    (
        "validateElementIsImpliedIncluded",
        "`isImpliedIncluded` records whether the implied relationships the semantics need have been \
         added to `ownedRelationship` (KerML `8.3.2.1.3`, which is not in this repository). Neither \
         grammar writes an implied relationship: `implied` is not a production. The reference adds \
         them during completion and sets the flag in the same branch (pilot sources: `ElementUtil.java:359-364`, \
         `TypeUtil.java:478, 502`, `FeatureUtil.java:350`). Its resource validator runs completion \
         without adding implicit elements (`KerMLResourceValidator.java:36`), so no element built from \
         text carries the flag. This tree has no `isImplied` field and never synthesises a \
         relationship, so the antecedent is false of every element the kernel reads, and a check would \
         have nothing to reject.",
    ),
    (
        "validateUsageVariationIsAbstract",
        "`variation` and `abstract` are alternatives of one prefix slot — `RefPrefix` is \
         `( isAbstract ?= 'abstract' | isVariation ?= 'variation' )?` (`SysML.xtext:559`) — so a \
         usage writes one word or the other. The lowering fills `is_abstract` from `is_variation` \
         (`derive_modifiers`, `sysml-syntax/src/lower.rs`) because that is what the notation \
         states and what the official models show: they write `variation part v;` with no \
         `abstract` and are legal. With the flag filled, `isVariation implies isAbstract` has no \
         model to reject.",
    ),
    (
        "validateDefinitionVariationIsAbstract",
        "The same slot in `DefinitionPrefix` (`SysML.xtext:491`): `variation def V` and \
         `abstract def V` are one choice, and `derive_modifiers` fills `is_abstract` from \
         `is_variation` for a definition exactly as it does for a usage.",
    ),
    (
        "validateTransitionFeatureMembershipOwningType",
        "The three memberships are only written inside a transition. Four productions write them — \
         `TransitionUsage` (`SysML.xtext:1854-1864`), `TargetTransitionUsage` (`:1866-1878`), \
         `GuardedTargetSuccession` (`:1708-1712`) and `GuardedSuccession` (`:1719-1726`) — and all \
         four return `SysML::TransitionUsage`, so their owning type is a TransitionUsage by \
         production. An earlier version of this reason named only the first two; the other two are \
         what a reader checking the claim would have found first.",
    ),
    (
        "validateFramedConcernMembershipConstraintKind",
        "`FramedConcernMember` is `MemberPrefix kind = FramedConcernKind`, and \
         `enum FramedConcernKind returns SysML::RequirementConstraintKind : requirement = 'frame';` \
         (`SysML.xtext:2073-2080`) has one literal: the word `frame` is the kind.",
    ),
    (
        "validateRequirementVerificationMembershipKind",
        "`RequirementVerificationMember` is `MemberPrefix kind = RequirementVerificationKind`, and \
         `RequirementVerificationKind` has one literal, `requirement = 'verify'` \
         (`SysML.xtext:2262-2269`): the word `verify` is the kind.",
    ),
    (
        "validatePortDefinitionConjugatedPortDefinition",
        "`PortDefinition` ends with `ownedRelationship += ConjugatedPortDefinitionMember` \
         (`SysML.xtext:954-958`), which is not optional: every port definition the notation writes \
         owns exactly one conjugated port definition, so the rule's `size() = 1` restates the \
         production.",
    ),
    (
        "validateConjugatedPortDefinitionConjugatedPortDefinitionIsEmpty",
        "`derivePortDefinitionConjugatedPortDefinition` is the first owned \
         `ConjugatedPortDefinition` of the element, and a `ConjugatedPortDefinition` owns a \
         `PortConjugation` (`SysML.xtext:964-966`) and nothing else — so the derivation yields null \
         for a conjugated port definition and `conjugatedPortDefinition = null` cannot be false.",
    ),
    (
        "validateTransitionFeatureMembershipEffectAction",
        "`EffectBehaviorMember` owns an `EffectBehaviorUsage`, and `EffectBehaviorUsage returns \
         SysML::ActionUsage` (`SysML.xtext:1909-1921`): the `do` clause of a transition is an \
         action usage by production, so the rule's consequent holds of every model the notation \
         can write. What a model can get wrong is the *reference* the clause makes, and \
         `validatePerformActionUsageReference` is the rule that says so — this checker reports it \
         there (SEM044).",
    ),
    (
        "validateTransitionFeatureMembershipTriggerAction",
        "`TriggerActionMember` owns a `TriggerAction`, and `TriggerAction returns \
         SysML::AcceptActionUsage: AcceptParameterPart` (`SysML.xtext:1889-1899`): the trigger of a \
         transition is an accept action usage by production, whatever word introduced it.",
    ),
    (
        "validateFlowEndIsEnd",
        "`FlowEndMember returns SysML::EndFeatureMembership` (`SysML.xtext:1314-1316`, `KerML.xtext:1031-1033`), \
         so every flow end is owned through an end feature membership. The grammar writes `isEnd` nowhere \
         on a flow end (`SysML.xtext:1318-1321`), and the metamodel default is false (`generated.rs:2235`). \
         The reference sets the flag in the end-membership post-processor \
         (`EndFeatureMembershipParserPostProcessor.java:35`, reached through \
         `SysMLParserPostProcessorFactory.java:30`). The rule holds because that post-processor runs on \
         every flow end the notation writes, not because of the membership alone.",
    ),
    (
        "validateFlowEndNestedFeature",
        "`FlowEnd` is `( ownedRelationship += FlowEndSubsetting )? ownedRelationship += \
         FlowFeatureMember` (`SysML.xtext:1318-1321`): the feature member is mandatory and owns one \
         feature, while the subsetting is a relationship rather than an owned feature. Exactly one \
         owned feature follows from the production.",
    ),
    (
        "validateFlowEndOwningType",
        "`FlowEndMember` is written only inside `FlowDeclaration` — the `from`/`to` pair or the \
         bare `to` form (`KerML.xtext:1005-1013`, `SysML.xtext:1283-1290`) — so the owning type of \
         a FlowEnd is a Flow by production.",
    ),
    (
        "validateFlowPayloadFeature",
        "`FlowDeclaration` writes `'of' ownedRelationship += PayloadFeatureMember` at most once, \
         and the alternative form writes none (`KerML.xtext:1005-1013`): a flow has at most one \
         payload feature because the production has at most one such member.",
    ),
    (
        "validateConjugatedPortDefinitionOriginalPortDefinition",
        "`ownedPortConjugator.originalPortDefinition = originalPortDefinition`, where the right-hand side is \
         declared `derived: true` on `ConjugatedPortDefinition` (`generated.rs`), and the pilot's getter reads \
         the conjugator (`ConjugatedPortDefinition_originalPortDefinition_SettingDelegate.java:38`). The \
         conjugator's stored value is set to the owning port definition by \
         `PortConjugationParserPostProcessor.java:46`, and no grammar line assigns it (`SysML.xtext:968-970`), \
         so both sides are one value.",
    ),
    // -- a bound is written inside the feature, so it has the feature's domain ---------------


    // -- the notation writes one of the two sides, never both and never neither --------------
    (
        "validateAnnotationAnnotatingElement",
        "`ownedAnnotatingElement <> null xor owningAnnotatingElement <> null`: both forms the notation \
         has write exactly one side. `OwnedAnnotation` is `ownedRelatedElement += AnnotatingElement` \
         (`SysML.xtext:67-69`), so the annotation owns the annotating element. The other form is \
         `MetadataUsage (',' ownedRelationship += Annotation)*` (`SysML.xtext:143-144`), where the annotation \
         follows an annotating element and is owned by it. `AnnotatedElement::annotation` builds the list \
         from both ends (`generated.rs`), so the two never meet on one annotation.",
    ),

    (
        "validateOccurrenceUsageIsPortion",
        "`portionKind <> null implies isPortion`. The previous reason gave two claims and both were \
         wrong. First it cited `KerML.xtext:518` for `isPortion ?= 'portion'`; that line is \
         `( isComposite ?= 'composite' | isPortion ?= 'portion' )?` inside `fragment \
         BasicFeaturePrefix` — the *prefix* fragment, which has nothing to do with the \
         `snapshot`/`timeslice` words the rule is about, so the citation did not support the claim. \
         Then it said the blocker was that `portionKind` is not a structured field here and the \
         authored word is kept as a string. Measured, that is false: the word *is* reachable, and \
         `validateOccurrenceUsagePortionKind` — a rule with the same antecedent — is enforced in this \
         kernel and fires on a `snapshot`/`timeslice` usage. The true \
         reason is the one this entry was reaching for: the consequent holds by construction. \
         `Index::is_portion` (`sysml-resolve/src/lib.rs:764-768`) answers `portion.is_some()`, and the \
         grammar writes `portion` into that field only for the words that also set a portion kind, so \
         no model can have a non-null `portionKind` with `isPortion` false. Nothing is missing from \
         the projection; the rule is a tautology over the writable language, exactly as \
         `validateConnectionDefinitionIsSufficient` is",
    ),
    // -- the four operator rules --------------------------------------------------------------
    //
    // Each class fixes its operator, so no writable model carries another value. The notation writes
    // `SelectExpression`, `CollectExpression`, `FeatureChainExpression` and `IndexExpression` with
    // `operand` only (`KerMLExpressions.xtext`), but the metamodel defines each class by its operator
    // (`kerml1-spec.txt:10891-10894`), and the upstream converter sets that operator when it builds the
    // class (`build.rs:865-884`). The reference's validators test the stored field, which the grammar
    // never sets; that is a property of the reference, not a model that can be written.
    (
        "validateSelectExpressionOperator",
        "The class fixes the operator: the metamodel defines `SelectExpression` by `operator = select` \
         (`kerml1-spec.txt:10891-10894`), and the upstream converter sets it when it builds the class \
         (`build.rs:883`). The notation writes the class with `operand` only \
         (`KerMLExpressions.xtext:316-317`), so no writable select carries another operator. The kernel \
         has no `Kind` for the class and builds no node for it.",
    ),
    (
        "validateCollectExpressionOperator",
        "The class fixes the operator: a `CollectExpression` is an `OperatorExpression` whose operator \
         is `collect` (`generated.rs:152-155`), and the converter sets it at construction \
         (`build.rs:884`). The notation writes the class with `operand` only, so no writable collect \
         carries another operator. The kernel has no `Kind` for the class and builds no node for it.",
    ),
    (
        "validateFeatureChainExpressionOperator",
        "The class fixes the operator: a feature chain is an `OperatorExpression` whose operator is `.` \
         (`generated.rs:383-388`), and the converter sets it at construction (`build.rs:871`). The \
         notation writes the class with `operand` only (`KerMLExpressions.xtext:301`). The kernel has \
         no `Kind` for the class and builds no node for it.",
    ),
    (
        "validateIndexExpressionOperator",
        "The class fixes the operator: an index is an `OperatorExpression` whose operator is `#` \
         (`generated.rs:489-492`), and the converter sets it at construction (`build.rs:865-869`). \
         `SysML.xtext:1576-1577` is the bracket form, which builds an `OperatorExpression` with `[`, not \
         an index. The kernel has no `Kind` for the class.",
    ),
    (
        "validateInvocationExpressionOwnedFeatures",
        "`ownedFeature->forAll(f | f <> result implies f.direction = in)`: an invocation's arguments
         are owned through `ArgumentExpressionMember`, which is a `ParameterMembership`
         (`SysML.xtext:1487-1488`) whose `parameterDirection()` is `_'in'` (`generated.rs:9294`),
         and the projection reads an argument's direction from exactly that. A model writes no
         direction on an argument, so no model states the contrary.",
    ),
    // -- entailed by the derivation the lowering fills -----------------------------------------
    (
        "validateRequirementConstraintMembershipIsComposite",
        "`ownedConstraint.isComposite`: a usage is composite unless it says otherwise, and the only \
         words that say so are `ref`, `end`, a declared direction, or being an attribute usage \
         (`build.rs:2242-2256`). `RequirementConstraintUsage` has no slot for any of them: its two \
         alternatives are a reference subsetting written as a bare name and `constraint` plus a \
         declaration (`SysML.xtext:2066-2071`). The bare-name form does not clear the flag either — \
         `require #g r1;` (`SemanticMetadata_valid.sysml.xt:92`, a model the pilot calls valid) \
         references `r1` and is still composite, because the converter makes it composite (`build.rs:2242-2256`; the metamodel defaults `isComposite` to false, `generated.rs:2234`) \
         and `ref`, an end, a direction or a reference-kinded usage clears it. So no model states the contrary, and \
         an earlier version of this reason was wrong about why: it said the notation cannot write a \
         referential form, and it can.",
    ),
    (
        "validateObjectiveMembershipIsComposite",
        "The same entailment for `ownedObjectiveRequirement`: an `objective` block's requirement is \
         a member of the requirement that owns it, and nothing the notation can write makes it \
         referential. `ObjectiveRequirementUsage` is `UsageExtensionKeyword* \
         ConstraintUsageDeclaration RequirementBody` (`SysML.xtext:2202-2204`) with no reference \
         subsetting alternative, so the bare `objective o1;` the corpus writes declares an owned \
         usage rather than referencing one.",
    ),
    // -- the specification's own OCL contradicts itself ---------------------------------------
    (
        "validateEventOccurrenceUsageIsReference",
        "`isReference`, whose value this kernel now derives: `Element::is_reference` is filled by \
         `lower::effective_reference`, which answers `true` for `Kind::EventOccurrenceUsage` because \
         the metaclass redefines the property to a constant `true` with `derived: true` \
         (`generated.rs:2212`), and the first production alternative writes \
         `ownedRelationship += OwnedReferenceSubsetting` (`SysML.xtext:867`); the other alternative (`SysML.xtext:868`) writes none, so the metaclass default is the basis. So the rule's \
         consequent holds of every element it can select, and a check would be a check that cannot \
         fail. The earlier workaround for this — a hard-coded `Kind::EventOccurrenceUsage` arm in \
         `effective_composite` (`lower.rs:2296`) that named this rule — has been retired: the flag is \
         now the single place the fact lives, and the composite arm remains only because a reference \
         is by definition not composite. This is a tautology over the writable language, the same \
         shape as `validateConnectionDefinitionIsSufficient`, not a representation gap",
    ),
    (
        "validateReferenceUsageIsReference",
        "`isReference` is a constant here: `ReferenceUsage` redefines the property to `true` with \
         `derived: true` (`generated.rs:2492`), and the rule says the same (`generated.rs:7714-7717`), \
         so the rule cannot be violated. Four kinds carry the `ReferenceUsage` metaclass (`Parameter`, \
         `Ref`, `DefaultReference` and `Subject`, `metaclass.rs:150,162,165,246`), and \
         `lower::effective_reference` (`lower.rs:2576-2584`) leaves their flag false unless `ref` is \
         written. That contradicts the metamodel for those kinds. It is a defect in the flag, and no \
         verdict here depends on it",
    ),
    (
        "validateAssociationStructureIntersection",
        "The OCL asks whether an Association that is also a Structure is an `AssociationStructure`. \
         The projection now gives that combination one identity: `assoc struct` lowers to \
         `Kind::AssocStructureDef`, whose metaclass is `AssociationStructure` and which is a kind of \
         both `Association` and `Structure`. Plain `assoc def` remains `Kind::AssocDef` and is not a \
         `Structure`; ordinary structures are not Associations. The kind mapping therefore makes the \
         equality true for every representable Association, so there is no failing model for a \
         runtime check to reject. This remains not applicable by construction; the combined-kind \
         mapping and its regression tests are in `sysml-model/tests/metaclass.rs` and \
         `sysml-syntax/tests/loading.rs`.",
    ),

    (
        "validateParameterMembershipOwningType",
        "A `ParameterMembership` cannot be owned by anything but a Behavior or a Step, and in this \
         projection that holds by construction: a directed feature becomes `Kind::Parameter` only when \
         its owner can own parameters (`sysml-syntax/src/lower.rs:410-431`), and every kind in that \
         list is a behavior or a step. `part def Q { in x; }` lowers to `attribute x (in)`, and \
         `action def A { in y; }` to `parameter y`. The owners that need a second look are the \
         requirement and the subject. A requirement definition specializes a constraint definition, \
         which is a behavior (`generated.rs:1739`), and a requirement usage a constraint usage, which is \
         a step (`generated.rs:1740`), so their parameters pass the owner test. `SubjectMembership` is a \
         `ParameterMembership` (`generated.rs:1755`), but the tree records a subject as `Kind::Subject`, \
         not as a parameter, so it carries no `ParameterMembership` for the rule to read. The third \
         disjunct covers the parameters of expressions such as calls and constructors. The tree keeps \
         those as `Expr` values rather than members, so there is no `ParameterMembership` for it to read.",
    ),
    (
        "validateFeatureReferenceExpressionResult",
        "`result.owningType = self`. The reference adds a result parameter to every feature reference \
         expression: `FeatureReferenceExpressionParserPostProcessor` calls `TypeUtil.addResultParameterTo` \
         (`FeatureReferenceExpressionParserPostProcessor.java:31`), and the factory dispatches to it on the \
         class's own case (`KerMLParserPostProcessorFactory.java:102-103`). So a result owned by the \
         expression exists in every model the reference reads. The kernel builds no expression element and \
         has no result to check, and no writable text omits the result, so the rule is inapplicable.",
    ),


    (
        "validateMetadataAccessExpressionReferencedElement",
        "A `MetadataAccessExpression` is written `A.metadata` (spec corpus `kerml1-spec.txt:5763-5766`: \
         `ownedRelationship += ElementReferenceMember '.' 'metadata'`). Its referenced element is what \
         `ElementReferenceMember` names, and that member is a plain `Membership` \
         (`KerMLExpressions.xtext:413-415`), not a feature membership, so the exists-test holds of every \
         metadata access the notation writes. The earlier example, `(as A).z`, is not a metadata access. \
         It is a feature chain on a cast (`KerMLExpressions.xtext:193`), and this parser reads `as` as a \
         type test (`sysml-parser/src/parser.rs:2265-2271`). The tree has no kind for metadata-access \
         expressions. That is a construction gap, not a source of violations.",
    ),
    (
        "validateIfActionUsageParameters",
        "`IfNode` is `'if' ownedRelationship += ExpressionParameterMember ownedRelationship += \
         ActionBodyParameterMember ( 'else' … )?` (`SysML.xtext:1596-1601`), and the parser now \
         rejects `if then b;` and `if c;` rather than lowering them to a control flow with the \
         member missing (`sysml-parser/src/parser.rs`, `control_flow`). So an IfActionUsage has two \
         or three owned input parameters by construction, and the rule's `>= 2` restates the \
         production.",
    ),
    (
        "validateWhileLoopActionUsage",
        "`WhileLoopNode` writes `'while' ExpressionParameterMember` or `'loop' EmptyParameterMember`, \
         then the body (`SysML.xtext:1615-1622`), and the parser now requires the body of both \
         spellings and the condition of `while`. Two or three parameter memberships, by \
         production.",
    ),
    (
        "validateForLoopActionUsageParameters",
        "`ForLoopNode` writes `'for' ForVariableDeclarationMember 'in' NodeParameterMember`, then the \
         body (`SysML.xtext:1624-1630`). `for in 1..3 { }` used to lower to a control flow with no \
         variable; the parser now rejects it, so the count is exactly two by construction.",
    ),
    (
        "validateForLoopActionUsageLoopVariable",
        "`ForVariableDeclaration` returns `SysML::ReferenceUsage` (`SysML.xtext:1637-1638`) and is written \
         before the sequence expression, so `ownedFeature->at(1)` is the loop variable and is a \
         ReferenceUsage by construction, once the parser requires the variable, which it now does. Prefix \
         metadata and `end` prefixes are owning memberships, not feature memberships \
         (`SysML.xtext:131-132, 568-577, 584-586`), so they do not shift `at(1)`.",
    ),
    (
        "validateConstructorExpressionOwnedFeatures",
        "`ownedFeatures->excluding(result)->isEmpty()`. A `ConstructorExpression` is written `new T(args)`, \
         which is `'new' InstantiatedTypeMember ConstructorResultMember` (`KerMLExpressions.xtext:423-426`). \
         The instantiated type sits under a plain membership, not a feature membership (`:436-439`), so \
         the constructor's only owned feature is its result. The argument list belongs to that result \
         (`:432-433`), not to the constructor, so the rule holds of every constructor the notation writes.",
    ),
    (
        "validateTransitionUsageParameters",
        "`if triggerAction->isEmpty() then inputParameters()->size() >= 1 else >= 2 endif`. Both \
         branches hold by construction: `TransitionUsage` writes `ownedRelationship += \
         EmptyParameterMember` before the source and again before an optional trigger \
         (`SysML.xtext:1854-1864`), and `TargetTransitionUsage` writes one before anything else \
         (`:1866-1872`). An `EmptyParameterMember` *is* a parameter membership, so a transition \
         with no trigger has at least one input parameter and one with a trigger at least two. The \
         rule restates the production.",
    ),

];

/// What became of one rule the specification states.
///
/// Four states. `Enforced` carries the diagnostic code that reports the rule, so a number in a
/// report traces to the check behind it. `NotApplicable` carries the reason a rule left the
/// denominator. `Conflict` carries the disagreement that kept a rule from being enforced: the
/// specification states something checkable, the reference implementation does something else, and
/// this kernel followed the reference. `Blocked` carries the one thing the rule is waiting on — a
/// metaclass, a derived property, or an element the notation implies — so "not done yet" is a fact
/// about the tree rather than a label.
///
/// `Conflict` exists because none of the other three fits that case. A rule the reference reports at
/// a lower severity can be broken by a model (so `NotApplicable` is false), is not rejected (so
/// `Enforced` is false), and waits on nothing (so `Blocked` is false). Filing it as not applicable
/// would have claimed no model can break it, which the corpus contradicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Enforced(&'static str),
    NotApplicable(&'static str),
    Conflict(&'static str),
    Blocked(&'static str),
    /// Nothing is recorded about it, which the guard test forbids.
    Unclassified,
}

/// What a not-applicable reason rests on.
///
/// The classes are not the same kind of thing, and a claim is only as strong as the class behind
/// it. `Grammar` and `Metamodel` are text: anyone can open the cited line and disagree with the
/// reading, which is what makes them the strongest evidence the ledger can offer. `Construction` is
/// a fact about this kernel, true of this tree and of no specification. `Reference` is another
/// program — its corpus verdicts and its builder — and it is the weakest, because that program has
/// deviations of its own on record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// A production in `SysML.xtext` or `KerML.xtext`.
    Grammar,
    /// A default or derivation in the metamodel (`generated.rs`, `SysML.json`, the libraries).
    Metamodel,
    /// A fact about what this kernel builds, not about the specification.
    Construction,
    /// A verdict or a construction of the reference implementation, observed or read from source.
    Reference,
}

impl Evidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Evidence::Grammar => "grammar",
            Evidence::Metamodel => "metamodel",
            Evidence::Construction => "construction",
            Evidence::Reference => "reference",
        }
    }
}

/// What each not-applicable reason rests on, one entry per name in [`NOT_APPLICABLE`].
///
/// Kept beside the reasons rather than inside them because a reason is prose: a class written into a
/// sentence cannot be counted or printed. The guard test in `tests/ledger.rs` asserts the two name
/// sets are equal, so an entry cannot be added without saying what it rests on.
pub const EVIDENCE: &[(&str, Evidence)] = &[
    ("validateReferenceUsageIsReference", Evidence::Grammar),
    (
        "validateEnumerationDefinitionIsVariation",
        Evidence::Metamodel,
    ),
    (
        "validateConnectionDefinitionIsSufficient",
        Evidence::Metamodel,
    ),
    ("validateExposeVisibility", Evidence::Grammar),
    ("validateEndFeatureMembershipIsEnd", Evidence::Grammar),
    ("validateExposeIsImportAll", Evidence::Grammar),
    ("validateElementIsImpliedIncluded", Evidence::Construction),
    ("validateUsageVariationIsAbstract", Evidence::Grammar),
    ("validateDefinitionVariationIsAbstract", Evidence::Grammar),
    (
        "validateTransitionFeatureMembershipOwningType",
        Evidence::Grammar,
    ),
    (
        "validateFramedConcernMembershipConstraintKind",
        Evidence::Grammar,
    ),
    (
        "validateRequirementVerificationMembershipKind",
        Evidence::Grammar,
    ),
    (
        "validatePortDefinitionConjugatedPortDefinition",
        Evidence::Grammar,
    ),
    (
        "validateConjugatedPortDefinitionConjugatedPortDefinitionIsEmpty",
        Evidence::Grammar,
    ),
    (
        "validateTransitionFeatureMembershipEffectAction",
        Evidence::Grammar,
    ),
    (
        "validateTransitionFeatureMembershipTriggerAction",
        Evidence::Grammar,
    ),
    ("validateFlowEndIsEnd", Evidence::Grammar),
    ("validateFlowEndNestedFeature", Evidence::Grammar),
    ("validateFlowEndOwningType", Evidence::Grammar),
    ("validateFlowPayloadFeature", Evidence::Grammar),
    (
        "validateConjugatedPortDefinitionOriginalPortDefinition",
        Evidence::Grammar,
    ),
    ("validateAnnotationAnnotatingElement", Evidence::Grammar),
    ("validateOccurrenceUsageIsPortion", Evidence::Grammar),
    ("validateSelectExpressionOperator", Evidence::Grammar),
    ("validateCollectExpressionOperator", Evidence::Grammar),
    ("validateFeatureChainExpressionOperator", Evidence::Grammar),
    ("validateIndexExpressionOperator", Evidence::Grammar),
    (
        "validateInvocationExpressionOwnedFeatures",
        Evidence::Grammar,
    ),
    (
        "validateRequirementConstraintMembershipIsComposite",
        Evidence::Reference,
    ),
    ("validateObjectiveMembershipIsComposite", Evidence::Grammar),
    ("validateEventOccurrenceUsageIsReference", Evidence::Grammar),
    (
        "validateAssociationStructureIntersection",
        Evidence::Construction,
    ),
    (
        "validateParameterMembershipOwningType",
        Evidence::Construction,
    ),
    (
        "validateFeatureReferenceExpressionResult",
        Evidence::Metamodel,
    ),
    (
        "validateMetadataAccessExpressionReferencedElement",
        Evidence::Construction,
    ),
    ("validateIfActionUsageParameters", Evidence::Grammar),
    ("validateWhileLoopActionUsage", Evidence::Grammar),
    ("validateForLoopActionUsageParameters", Evidence::Grammar),
    ("validateForLoopActionUsageLoopVariable", Evidence::Grammar),
    (
        "validateConstructorExpressionOwnedFeatures",
        Evidence::Construction,
    ),
    ("validateTransitionUsageParameters", Evidence::Grammar),
];

/// The class of evidence behind a not-applicable reason, or `None` for a rule that is not in that
/// list.
pub fn evidence_of(name: &str) -> Option<Evidence> {
    EVIDENCE
        .iter()
        .find(|(rule, _)| *rule == name)
        .map(|(_, class)| *class)
}

/// Rules where the specification and the reference implementation disagree, and this kernel
/// follows the reference.
///
/// A rule belongs here when the specification states something checkable, the reference does
/// something else, and following the specification would reject models the reference accepts. The
/// entry names the disagreement and says which side this kernel took; it does not settle which side
/// is right. Whoever does settle it moves the rule to `implemented()` or back to `NOT_APPLICABLE`,
/// with the reason.
pub const CONFLICT: &[(&str, &str)] = &[
    (
        "validateNamespaceDistinguishibility",
        "The specification states it as an error — `membership->forAll(m1 | membership->forAll(m2 | \
         m1 <> m2 implies m1.isDistinguishableFrom(m2)))` (`generated.rs:7296`) — and \
         `isDistinguishableFrom` has a stated body (`:9731-9735`): two members are distinguishable \
         when neither element is of the other's metaclass, or when their names differ. The model \
         that pins the rule declares four members called `A` — a classifier, two packages and a \
         feature — and expects four warnings, not errors. Two of the four are the same metaclass \
         with the same name, so the rule rejects exactly those two, while the reference warns on \
         all four. This kernel follows the reference: `SEM091` groups by name across metaclasses and \
         warns, which is wider than the rule. Neither direction works. Narrowing the warning to the \
         rule's shape drops the two cross-metaclass warnings the corpus asserts; raising the rule to \
         an error turns the two `package A` warnings into errors the corpus does not. Both would \
         fall below the pilot floor, because `pilot.py` compares severity and a warning assertion is \
         not satisfied by an error — that is the tool's mechanism, read rather than run; neither \
         direction was built. The disagreement is recorded here; a reader who settles it moves the \
         rule, not the entry.",
    ),
    (
        "validateAcceptActionUsageParameters",
        "The literal rule counts owned inputs (`input->select(f | f.owner = self)`, `generated.rs:9816-9818`) \
         and asks for two. `accept s : Signal;` has one owned input, the payload, because its receiver is \
         inherited from `AcceptAction` (`Transfers.kerml:254-265`), so the literal rule rejects a form the \
         notation writes. The reference checks `usg.inputParameters.empty` (`SysMLValidator.xtend:781-784`), \
         which every accept meets, and the kernel's own count includes the inherited receiver, so it \
         reports nothing either. Enforcing the literal count would reject the bare accept form, which no \
         `noErrors` pilot model writes. The choice is open; a reader who settles it moves the rule, not the \
         entry.",
    ),
];

/// Every rule the kernel does not enforce and cannot dismiss, with what it is waiting on.
///
/// Generated by `tools/blocked_by.py` from the rule table, the enforced list and this list; the
/// generator refuses to write when a rule is unclassified or when a blocker is not a name from one
/// of the first three layers.
pub const BLOCKED_BY: &[(&str, &str)] = include!("../data/blocked_by.rs");

/// Derived properties the specification reads that the tree does not carry.
///
/// The second layer builds these. A `BLOCKED_BY` value is either one of these names, a metaclass
/// from [`sysml_model::metaclass::METACLASSES`], or a name in [`IMPLIED_FACTS`], and the guard test
/// in `tests/ledger.rs` enforces that: a blocker outside those three would be a rule labelled
/// "blocked" by something that is not a fact about the tree.
///
/// Every name here is a property `research/upstream/SysML.json` declares — not an OCL helper
/// operation. A rule that reads an operation instead is recorded as not applicable, because no
/// field can carry it and the specification states no algorithm for it.
pub const DERIVED_PROPERTIES: &[&str] = &[
    // `ElementFilterMembership::condition` — the expression a `filter` clause writes.
    "condition",
    "chainingFeature",
    "crossFeature",
    "featureTarget",
    "featuringType",
    // `Feature::featureValue` — the `= value` a metadata body writes, which `MetadataFeatureBody`
    // reads. The OCL calls the value a `valuation`; the metamodel names the property `featureValue`.
    "featureValue",
    "instantiatedType",
    "isAbstract",
    "isImplied",
    "isModelLevelEvaluable",
    "isPortion",
    "multiplicity",
    "portionKind",
    "referent",
    "referencedElement",
    "result",
    "visibility",
];

/// Facts the notation or the library implies, which the lowering does not build yet.
///
/// A name here is not a property: it is a thing the model shows and the tree lacks. `inputParameters`
/// is the OCL's `ActionUsage::inputParameters()`, which `derived::input_parameters` already answers
/// for what the model wrote; counting the parameters an action's library base implies
/// (`AcceptAction.payload`, `SendAction.sender`/`receiver`) needs those members to exist first.
pub const IMPLIED_FACTS: &[&str] = &[
    "inputParameters",
    "parameterMembership",
    "grammarEnforcement",
];

/// The state of one rule, by the specification's own name.
pub fn status(name: &str) -> Status {
    if let Some((code, _)) = implemented().iter().find(|(_, n)| *n == name) {
        return Status::Enforced(code);
    }
    if let Some((_, why)) = NOT_APPLICABLE.iter().find(|(n, _)| *n == name) {
        return Status::NotApplicable(why);
    }
    if let Some((_, why)) = CONFLICT.iter().find(|(n, _)| *n == name) {
        return Status::Conflict(why);
    }
    if let Some((_, what)) = BLOCKED_BY.iter().find(|(n, _)| *n == name) {
        return Status::Blocked(what);
    }
    Status::Unclassified
}

/// Rules the specification states, that this kernel does not check.
pub fn unimplemented() -> Vec<&'static SpecRule> {
    let done: BTreeSet<&str> = implemented().iter().map(|(_, name)| *name).collect();
    RULES.iter().filter(|r| !done.contains(r.name)).collect()
}

/// How the corpus's own models stand against the rules that are enforced.
#[derive(Debug, Clone)]
pub struct Coverage {
    pub total: usize,
    pub enforced: usize,
    /// Rules the specification states that cannot reject any model.
    pub not_applicable: usize,
    /// Rules the specification states that this kernel reports at the reference's lower severity,
    /// so neither enforced nor inapplicable.
    pub conflict: usize,
    /// Rules waiting on a metaclass, a derived property or an implied element.
    pub blocked: usize,
    /// `total` — the denominator progress is measured against.
    ///
    /// This used to be `total - not_applicable`, which made the reported ratio self-certifying:
    /// labelling a rule "not applicable" shrank the denominator and raised the score, and nothing
    /// stopped a rule being filed that way to avoid implementing it. An adversarial audit of the 52
    /// entries found one that was unimplemented work rather than an impossible rule
    /// (`validateSubsettingFeaturingTypes`, now enforced), which is 1 in the 9 rows that had been
    /// tested — enough that the `127/(180-52) = 99.2%` figure could not be defended.
    ///
    /// The field is kept as a separate name rather than deleted so that a caller reading `applicable`
    /// gets the honest number instead of silently losing the field.
    pub applicable: usize,
}

pub fn coverage() -> Coverage {
    let enforced = implemented()
        .iter()
        .map(|(_, name)| *name)
        .collect::<BTreeSet<_>>()
        .len();
    // Count only names the specification actually states: an entry that has drifted from the rule
    // table would otherwise shrink the denominator (or grow it) by naming nothing.
    let stated = |name: &str| RULES.iter().any(|r| r.name == name);
    let not_applicable = NOT_APPLICABLE
        .iter()
        .filter(|(name, _)| stated(name))
        .count();
    let blocked = BLOCKED_BY.iter().filter(|(name, _)| stated(name)).count();
    let conflict = CONFLICT.iter().filter(|(name, _)| stated(name)).count();
    Coverage {
        total: RULES.len(),
        enforced,
        not_applicable,
        conflict,
        blocked,
        applicable: RULES.len(),
    }
}

// -- rules that run ----------------------------------------------------------

/// One rule violation, with the element it is about.
#[derive(Debug, Clone)]
pub struct RuleViolation {
    pub element: String,
    pub line: u32,
    pub message: String,
}

impl RuleViolation {
    fn new(el: &Element, message: String) -> Self {
        Self {
            element: el.name.clone(),
            line: el.line,
            message,
        }
    }
}

/// The bounds a multiplicity string states, or `None` when it cannot be read.
///
/// `[4]` is 4..4, `[2..*]` is 2..unbounded, `[0..1]` is 0..1. A bound written as an expression
/// rather than a literal is not read here: deciding it needs the evaluator, and guessing would
/// report a violation the model may not have.
fn bounds_of(multiplicity: &str) -> Option<(Option<i64>, Option<i64>)> {
    let inner = multiplicity
        .trim()
        .strip_prefix('[')?
        .strip_suffix(']')?
        .trim();
    let parse = |text: &str| -> Option<Option<i64>> {
        let t = text.trim();
        if t == "*" {
            Some(None)
        } else {
            t.parse::<i64>().ok().map(Some)
        }
    };
    match inner.split_once("..") {
        Some((lo, hi)) => {
            let lower = parse(lo)?;
            let upper = parse(hi)?;
            Some((lower, upper))
        }
        None => {
            let only = parse(inner)?;
            Some((only, only))
        }
    }
}

/// `validateMultiplicityRangeBounds` — a bound must be a non-negative whole number, and a range's
/// lower bound must not exceed its upper bound.
///
/// A model that says `[4..2]` states a range that cannot be populated. The specification's own
/// rule is about the ordering of the range's owned members; the part a model can be wrong about
/// without any abstract syntax is the numbers, so that is what runs.
pub fn multiplicity_bounds_issues(roots: &[Element]) -> Vec<RuleViolation> {
    let mut out = Vec::new();
    fn walk(el: &Element, out: &mut Vec<RuleViolation>) {
        if let Some(m) = &el.multiplicity {
            if let Some((lo, hi)) = bounds_of(m) {
                if let Some(lo) = lo {
                    if lo < 0 {
                        out.push(RuleViolation::new(
                            el,
                            format!("multiplicity `{m}` has a negative lower bound"),
                        ));
                    }
                }
                if let (Some(lo), Some(hi)) = (lo, hi) {
                    if lo > hi {
                        out.push(RuleViolation::new(
                            el,
                            format!("multiplicity `{m}` has a lower bound above its upper bound"),
                        ));
                    }
                }
            }
        }
        for child in &el.children {
            walk(child, out);
        }
    }
    for root in roots {
        walk(root, &mut out);
    }
    out
}

/// `validateMultiplicityRangeBoundResultTypes` — every multiplicity bound must be a whole,
/// non-negative number.
///
/// The rule's own text is "The results of the bound Expression(s) of a MultiplicityRange must be
/// typed by ScalarValues::Integer. If a bound is model-level evaluable, then it must evaluate to a
/// non-negative value." Three shapes are decided here:
///
///   * A written literal that is not a whole number — `[2.5]`, `[1..true]`, `["x"..*]`. A Boolean
///     or a string is typed by something other than `Integer`, so the first clause alone rejects it.
///   * A written literal below zero — `[-1]`. `valueOf` settles it, and the rule rejects it.
///   * A name whose value this layer can compute — `[n..b]` where `b = n > 3` yields a Boolean, and
///     `[m1..m2]` where `m1 = n - 1` and `n = 0` yields `-1`. Both are the pilot's own models
///     (`MultiplicityRange_invalid.kerml.xt:37`, `:42`).
///
/// A name this layer cannot evaluate is skipped: the rule only constrains bounds it can settle, and
/// guessing would reject models that may be right.
pub fn multiplicity_type_issues<F, B>(
    roots: &[Element],
    value_of: F,
    is_boolean: B,
) -> Vec<RuleViolation>
where
    F: Fn(usize, &str) -> Option<crate::Value>,
    B: Fn(usize, &str) -> bool,
{
    let mut out = Vec::new();
    fn walk(
        el: &Element,
        out: &mut Vec<RuleViolation>,
        value_of: &dyn Fn(usize, &str) -> Option<crate::Value>,
        is_boolean: &dyn Fn(usize, &str) -> bool,
    ) {
        if let Some(m) = &el.multiplicity {
            if let Some(inner) = m.trim().strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                for part in inner.split("..") {
                    let t = part.trim();
                    if t.is_empty() || t == "*" {
                        continue;
                    }
                    // A literal the grammar accepts but the type rejects: a real fraction, a
                    // Boolean, a quoted string.
                    if let Ok(real) = t.parse::<f64>() {
                        if t.parse::<i64>().is_err() {
                            out.push(RuleViolation::new(
                                el,
                                format!("multiplicity `{m}` has a fractional bound `{t}`"),
                            ));
                        } else if real < 0.0 {
                            out.push(RuleViolation::new(
                                el,
                                format!("multiplicity `{m}` has a negative bound `{t}`"),
                            ));
                        }
                        continue;
                    }
                    if t == "true" || t == "false" || t.starts_with('"') || t.starts_with('\'') {
                        out.push(RuleViolation::new(
                            el,
                            format!(
                                "multiplicity `{m}` has a bound `{t}`, which is not a whole number"
                            ),
                        ));
                        continue;
                    }
                    // A name. The rule applies to a bound that evaluates, so an answer this layer
                    // cannot compute means the bound is skipped rather than reported.
                    match value_of(el.id, t) {
                        Some(crate::Value::Int(v)) if v < 0 => out.push(RuleViolation::new(
                            el,
                            format!("multiplicity `{m}` has a negative bound `{t}` ({v})"),
                        )),
                        Some(crate::Value::Real(v)) if v < 0.0 => out.push(RuleViolation::new(
                            el,
                            format!("multiplicity `{m}` has a negative bound `{t}` ({v})"),
                        )),
                        // A real with no fractional part is a whole number that arrived through
                        // arithmetic (`2 - m1` where `m1` is -1 gives 3.0): legal, not fractional.
                        Some(crate::Value::Real(v)) if v.fract() != 0.0 => {
                            out.push(RuleViolation::new(
                                el,
                                format!("multiplicity `{m}` has a fractional bound `{t}` ({v})"),
                            ))
                        }
                        Some(crate::Value::Bool(_)) => out.push(RuleViolation::new(
                            el,
                            format!(
                                "multiplicity `{m}` has a bound `{t}`, which is a Boolean rather \
                                 than a whole number"
                            ),
                        )),
                        // An integer in range, or nothing this layer can settle. A bound whose
                        // value is out of reach may still have a known *type* — `b1 = I() > 3`
                        // compares, so it is a Boolean whether or not `I()` can be called.
                        _ => {
                            if is_boolean(el.id, t) {
                                out.push(RuleViolation::new(
                                    el,
                                    format!(
                                        "multiplicity `{m}` has a bound `{t}`, which is a Boolean \
                                         rather than a whole number"
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }
        for child in &el.children {
            walk(child, out, value_of, is_boolean);
        }
    }
    for root in roots {
        walk(root, &mut out, &value_of, &is_boolean);
    }
    out
}

/// `validateFeatureEndNoDirection` — an `end` feature carries no direction.
///
/// `end` means "this feature is an end of the association it belongs to"; a direction (`in`/`out`)
/// means "this is a parameter of a behaviour". Both at once is contradictory, and a model that
/// writes `end in x` gets no diagnostic from the resolver.
pub fn end_direction_issues(roots: &[Element]) -> Vec<RuleViolation> {
    let mut out = Vec::new();
    fn walk(el: &Element, out: &mut Vec<RuleViolation>) {
        if el.is_end && el.direction.is_some() {
            out.push(RuleViolation::new(
                el,
                "an `end` feature must not carry a direction".to_string(),
            ));
        }
        for child in &el.children {
            walk(child, out);
        }
    }
    for root in roots {
        walk(root, &mut out);
    }
    out
}
