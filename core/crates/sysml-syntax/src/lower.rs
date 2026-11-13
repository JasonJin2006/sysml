//! Lowering: syntax tree to the language-neutral element tree.
//!
//! This is the boundary the rest of the tool lives behind. Everything upstream is
//! `sysml-parser`'s tree; nothing downstream sees a `sysml_parser` type.
//!
//! # Why one function per kind is enough
//!
//! The parser keeps a definition and a usage of the same kind in one node shape, differing only
//! in `is_definition` and the body production. The lowering mirrors that: `part def X` and
//! `part x : T` walk the same path, and the differences are two flags. The previous lowering had
//! a separate arm per spelling, which is where the silent drops came from — a spelling nobody
//! wrote an arm for simply vanished.
//!
//! # The two channels
//!
//! A construct this layer does not model becomes [`Kind::Unsupported`] with the construct's name,
//! never a silent skip. A construct it does model but cannot interpret (an operator it does not
//! evaluate) becomes an opaque expression node, which keeps its children reachable by the name
//! checker. Both are visible in `check`; neither pretends to be understood.

use std::collections::HashMap;

use crate::library;
use sysml_model::{
    BinaryOp, Direction, Element, Expr, ImportShape, Kind, Membership, Modifiers, NameRef,
    TextRange, UnaryOp, Visibility,
};
use sysml_parser::ast::{
    Annotation, BodyMember, Declaration, Direction as AstDirection, Expr as AstExpr,
    ImportShape as AstImportShape, Kind as AstKind, RelationOp, Statement,
    Visibility as AstVisibility,
};

/// One loaded project.
pub struct Loaded {
    pub roots: Vec<Element>,
    /// Standard library elements, kept out of `roots` so no view ever draws them. Their ids
    /// continue after the project's, so one index can hold both.
    pub library: Vec<Element>,
    pub library_skipped: Vec<(std::path::PathBuf, String)>,
    /// Which standard library this load actually used.
    ///
    /// The bundled stub is a real fallback — 35 `attribute def`s and no units — and it used to
    /// be indistinguishable from the full library in every report. A model checked against the
    /// stub can pass while its unit chain is missing, so the load has to say which one it read.
    pub library_source: LibrarySource,
    pub element_count: usize,
}

/// Which standard library a load used.
///
/// The distinction matters because the two are not interchangeable: the real library carries the
/// quantity and unit chain (`MassValue :> ScalarQuantityValue`, `ISQ::kg`, conversion functions),
/// while the bundled stub carries only the names. A model can be clean against the stub and
/// inconclusive against the library, or the reverse, and nothing in the report used to say which
/// one had been read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibrarySource {
    /// Read from a directory of `.kerml` / `.sysml` files.
    Full {
        root: std::path::PathBuf,
        files: usize,
    },
    /// The bundled stub compiled into the binary.
    Bundled,
    /// No library at all — `SYSML_LIBRARY=off`, or a caller that asked for none.
    None,
}

impl LibrarySource {
    /// True when a model checked against this library may be missing the unit and quantity chain.
    pub fn is_stub(&self) -> bool {
        !matches!(self, LibrarySource::Full { .. })
    }

    /// One line for a report, naming the case rather than leaving it to be inferred.
    pub fn describe(&self) -> String {
        match self {
            LibrarySource::Full { root, files } => {
                format!("standard library: {} ({files} files)", root.display())
            }
            LibrarySource::Bundled => "standard library: the bundled stub, not the full library \
                 (no units, no quantity chain; set SYSML_LIBRARY to a library directory)"
                .to_string(),
            LibrarySource::None => "standard library: not loaded".to_string(),
        }
    }
}

// The buffer index the current lowering pass is reading from.
//
// A span in the syntax tree is an offset into one buffer, and the element tree records spans for
// editing. Threading a `file` parameter through every lowering call would touch every
// `Element::new` site for a value that is constant for the whole pass, so the index lives here.
thread_local! {
    static CURRENT_FILE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn current_file() -> usize {
    CURRENT_FILE.with(std::cell::Cell::get)
}

fn text_range(span: sysml_parser::ast::Span) -> TextRange {
    TextRange {
        file: current_file(),
        start: span.start,
        end: span.end,
    }
}

/// Lower one parsed file's members.
pub fn lower_document(doc: &sysml_parser::ParsedDocument) -> Vec<Element> {
    let mut out = Vec::new();
    for member in &doc.document.members {
        if let Some(el) = lower_member(member, doc) {
            out.push(el);
        }
    }
    out
}

/// Lower one body member, if it produces an element.
fn lower_member(member: &BodyMember, doc: &sysml_parser::ParsedDocument) -> Option<Element> {
    match member {
        BodyMember::Declaration(d) => Some(lower_declaration(d, doc)),
        BodyMember::Relationship(r) => lower_relationship(r, doc),
        BodyMember::Statement(s) => lower_statement(s, doc),
        BodyMember::Annotation(a) => lower_annotation(a, doc),
        BodyMember::Filter { expr, span } => {
            let mut el = Element::new(Kind::Filter, "filter", span.line);
            el.constraint = Some(render(expr));
            // The condition itself, not only its text: a rule asks whether its value is a Boolean,
            // and `ElementFilterMembership::condition` is the expression the clause writes.
            el.value_expr = Some(lower_expr(expr, doc));
            el.name_span = Some(text_range(*span));
            Some(el)
        }
        BodyMember::Error { message, span, .. } => {
            // A member the grammar rejected. It keeps its place in the tree so a reader sees
            // where the model stopped being readable, rather than a body that appears shorter.
            let mut el = Element::new(Kind::Unsupported, "syntax error", span.line);
            el.constraint = Some(message.clone());
            el.name_span = Some(text_range(*span));
            Some(el)
        }
    }
}

/// The name of the first feature among `members`, for an end a succession leaves unwritten.
///
/// Follows `UsageUtil.getPreviousFeature` in the reference implementation: a member that is not a
/// feature is passed over — a definition, an annotation, an import, a parameter, another
/// succession or transition, a connector or binding — and the first feature found decides. `first
/// start;` counts as `start`, the feature its membership names. A feature with no name a body could
/// refer to (`accept sig;`, an anonymous `action accept …`) ends the search with no answer rather
/// than letting an earlier feature stand in for it.
fn neighbour_feature<'a>(mut members: impl Iterator<Item = &'a BodyMember>) -> Option<String> {
    members.find_map(|member| -> Option<Option<String>> {
        match member {
            BodyMember::Declaration(d) => {
                if d.is_definition || d.head.prefixes.direction.is_some() {
                    return None;
                }
                Some(Some(d.head.name.display()).filter(|name| !name.is_empty()))
            }
            BodyMember::Statement(statement) => match statement {
                Statement::First { target, .. } => Some(target.as_ref().map(render)),
                Statement::Succession { .. }
                | Statement::Connect { .. }
                | Statement::Bind { .. }
                | Statement::Expression { .. }
                | Statement::Constraint { .. }
                | Statement::Inverting { .. }
                // Parameters: a result, and a subject (`SubjectMembership` is a parameter membership).
                | Statement::Return { .. }
                | Statement::ReturnDeclaration { .. }
                | Statement::Subject { .. } => None,
                // A flow is a connector unless it is a message, which has no ends.
                Statement::Flow { head, .. } if head.from.is_some() || head.to.is_some() => None,
                Statement::Flow { head, .. } => {
                    Some(Some(head.name.display()).filter(|name| !name.is_empty()))
                }
                Statement::ControlNode { name, .. } => {
                    Some(Some(name.display()).filter(|name| !name.is_empty()))
                }
                _ => Some(None),
            },
            _ => None,
        }
    })?
}

/// Lower every member of a body.
fn lower_body(body: &[BodyMember], doc: &sysml_parser::ParsedDocument) -> Vec<Element> {
    lower_body_of(body, doc, None)
}

/// Move each prefix annotation into the element it annotates.
///
/// `@Safety { level = 3 } part def Brake;` writes the annotation before the declaration it belongs
/// to — a `MemberPrefix` of that `FeatureElement` (`KerML.xtext:384`) — and lowering both as members
/// of the body put the annotation *beside* Brake: the tree carried a metadata usage in the package
/// and a part def with no annotation, so nothing downstream could ask what Brake's safety level was.
/// The annotated element owns its metadata usage, which is the shape `Annotation` gives it, so the
/// usage moves inside.
///
/// An annotation that ends at its own `;` is a different statement. `@B;` is a member of the body,
/// not a prefix of what follows, and SysML §7.27.2 (PDF p.192) says "if the metadata usage is an
/// owned member of a namespace, then the explicit identification of annotated elements can be
/// omitted, in which case the annotated element shall be implicitly the containing namespace". So it
/// stays where it is written and annotates the enclosing element. Moving it onto the next member
/// made `class C1 { @B; feature :>> x; }` claim that `x` was annotated — measured before this fix,
/// `outline` showed `feature x / metadata B : B` and the annotation of `C1` was nowhere.
///
/// A usage with an `about` clause names its targets and stays where it is written. A body that ends
/// with an annotation annotates nothing in that body; it stays a member rather than disappearing, so
/// a model that wrote it can still be read back.
fn attach_annotations(members: Vec<Element>) -> Vec<Element> {
    // Whether a run of leading annotations that starts at `from` ends at a declaration, which is
    // what decides if they are a `MemberPrefix` of it.
    let reaches_a_declaration: Vec<bool> = (0..members.len())
        .map(|from| {
            members[from..]
                .iter()
                .find(|m| !(m.kind == Kind::MetadataUsage && m.to.is_none()))
                .is_some_and(|m| m.kind != Kind::MetadataUsage)
        })
        .collect();
    let mut out: Vec<Element> = Vec::with_capacity(members.len());
    let mut pending: Vec<Element> = Vec::new();
    for (index, member) in members.into_iter().enumerate() {
        let is_annotation = member.kind == Kind::MetadataUsage && member.to.is_none();
        if is_annotation {
            // A leading annotation belongs to the declaration after it, so it waits — but only if a
            // declaration actually follows. `part y { @Classified { … } @Security; }` writes two
            // annotations and no declaration; under the old "attach to the next member" rule the
            // first was buried inside `metadata Security` (measured: `outline` showed `metadata
            // Security { metadata Classified { … } }`). Neither was a `MemberPrefix` there, so
            // §7.27.2 gives both to their containing element and neither to the other.
            if member.annotation_leads && reaches_a_declaration[index] {
                pending.push(member);
            } else {
                out.append(&mut pending);
                out.push(member);
            }
            continue;
        }
        let mut member = member;
        for annotation in pending.drain(..).rev() {
            member.children.insert(0, annotation);
        }
        out.push(member);
    }
    out.append(&mut pending);
    out
}

/// Lower every member of a body, given the kind of the element that owns it.
fn lower_body_of(
    body: &[BodyMember],
    doc: &sysml_parser::ParsedDocument,
    parent: Option<Kind>,
) -> Vec<Element> {
    let mut out: Vec<Element> = attach_annotations(
        body.iter()
            .enumerate()
            .filter_map(|(i, m)| {
                let mut el = match m {
                    BodyMember::Declaration(d) => {
                        Some(lower_declaration_in(d, doc, parent.clone()))
                    }
                    other => lower_member(other, doc),
                }?;
                // `then b;` and `then action b { … }` write no source, and the second no target
                // either: the source is the feature before the succession and the target the one
                // after it. Their neighbours in the body are the only place that says which.
                if let BodyMember::Statement(Statement::Succession {
                    implicit_source: true,
                    then,
                    ..
                }) = m
                {
                    if el.from.is_none() {
                        el.from = neighbour_feature(body[..i].iter().rev());
                    }
                    if then.is_none() && el.to.is_none() {
                        el.to = neighbour_feature(body[i + 1..].iter());
                    }
                }
                Some(el)
            })
            .collect(),
    );
    // A case's terminal expression *is* its result: `analysis def MassAnalysisCase { ...
    // vehicle.mass }` defines a `result` feature whose value is that expression, and
    // `MassAnalysisCase::result` is a legal reference elsewhere. Leaving it anonymous made every
    // such reference unresolved.
    if matches!(
        parent,
        Some(Kind::AnalysisCaseDef)
            | Some(Kind::CaseDef)
            | Some(Kind::UseCaseDef)
            | Some(Kind::CalcDef)
            | Some(Kind::CalcUsage)
    ) {
        for el in &mut out {
            if el.kind == Kind::DefaultReference && el.name == "<anonymous>" {
                el.kind = Kind::AttributeUsage;
                el.name = "result".to_string();
            }
        }
    }
    out
}

// -- declarations ------------------------------------------------------------

/// Map the parser's kind to the element kind.
///
/// A definition and a usage of the same language construct differ in one element kind pair
/// (`PartDef`/`PartUsage`), so the mapping needs to know which one this is.
fn element_kind(kind: &AstKind, is_definition: bool) -> Kind {
    use AstKind as K;
    match kind {
        K::Package | K::Namespace | K::LibraryPackage => Kind::Package,
        // `actor` and `stakeholder` name a membership, not a part: keeping them distinct is what
        // lets a rule ask whether the owner is a requirement or a case.
        K::Actor => Kind::Actor,
        K::Stakeholder => Kind::Stakeholder,
        K::Verification => Kind::Verification,
        K::Part => {
            if is_definition {
                Kind::PartDef
            } else {
                Kind::PartUsage
            }
        }
        K::Attribute => {
            if is_definition {
                Kind::AttributeDef
            } else {
                Kind::AttributeUsage
            }
        }
        K::Port => {
            if is_definition {
                Kind::PortDef
            } else {
                Kind::PortUsage
            }
        }
        K::Item => {
            if is_definition {
                Kind::ItemDef
            } else {
                Kind::ItemUsage
            }
        }
        K::Connection => {
            if is_definition {
                Kind::ConnectionDef
            } else {
                Kind::Connection
            }
        }
        K::Interface => {
            if is_definition {
                Kind::InterfaceDef
            } else {
                Kind::InterfaceUsage
            }
        }
        K::Flow => {
            if is_definition {
                Kind::Flow
            } else {
                Kind::FlowUsage
            }
        }
        K::Allocation => {
            if is_definition {
                Kind::AllocationDef
            } else {
                Kind::AllocationUsage
            }
        }
        K::Action => {
            if is_definition {
                Kind::ActionDef
            } else {
                Kind::ActionUsage
            }
        }
        K::State => {
            if is_definition {
                Kind::StateDef
            } else {
                Kind::StateUsage
            }
        }
        K::Calculation => {
            if is_definition {
                Kind::CalcDef
            } else {
                Kind::CalcUsage
            }
        }
        K::Constraint => {
            if is_definition {
                Kind::ConstraintDef
            } else {
                Kind::ConstraintUsage
            }
        }
        K::Requirement => {
            if is_definition {
                Kind::RequirementDef
            } else {
                Kind::RequirementUsage
            }
        }
        K::Concern => Kind::ConcernDef,
        K::Case => Kind::CaseDef,
        K::AnalysisCase => Kind::AnalysisCaseDef,
        K::VerificationCase => {
            if is_definition {
                Kind::VerificationDef
            } else {
                Kind::VerificationUsage
            }
        }
        K::UseCase => {
            if is_definition {
                Kind::UseCaseDef
            } else {
                Kind::UseCaseUsage
            }
        }
        K::View => {
            if is_definition {
                Kind::ViewDef
            } else {
                Kind::ViewUsage
            }
        }
        K::Viewpoint => {
            if is_definition {
                Kind::ViewpointDef
            } else {
                Kind::ViewpointUsage
            }
        }
        K::Rendering => {
            if is_definition {
                Kind::RenderingDef
            } else {
                Kind::RenderingUsage
            }
        }
        K::Metadata => {
            if is_definition {
                Kind::MetadataDef
            } else {
                Kind::MetadataUsage
            }
        }
        // A usage of `enum` is one value of an enumeration; the definition is the enumeration.
        K::Enumeration => {
            if is_definition {
                Kind::EnumDef
            } else {
                Kind::EnumValue
            }
        }
        // `occurrence def X` and `occurrence x : X` are two spellings, and the collapse used to
        // keep only the definition. Split them the way `item` and `use case` are split.
        K::Occurrence => {
            if is_definition {
                Kind::OccurrenceDef
            } else {
                Kind::OccurrenceUsage
            }
        }
        // `event e : E` is an `EventOccurrenceUsage` (`SysML.xtext:865-870`).
        K::Event => Kind::EventOccurrenceUsage,
        // `individual def X` and `individual x : T` are the same word twice: the first is a
        // definition of one instance, the second a usage naming one. Collapsing both into the
        // definition kind put the usage outside every rule that asks about occurrence usages,
        // including the one that requires it to be typed by an individual definition.
        K::Individual => {
            if is_definition {
                Kind::IndividualDef
            } else {
                Kind::IndividualUsage
            }
        }
        K::Dependency => Kind::Dependency,
        // KerML classifiers are their own kind: they are how the standard library defines every
        // scalar type and quantity.
        K::Kerml(k) => match k.as_str() {
            // A KerML step or feature is a feature of the behaviour that owns it.
            // `feature f : T` is a KerML Feature, not a SysML attribute. They shared a kind, so
            // every SysML rule about attributes judged KerML models: `feature f : T` with `T` a
            // `class` was rejected, and a feature with a composite member broke the rule that a
            // feature of an attribute is referential. The two languages name different things.
            "feature" | "end" => Kind::KermlFeature,
            // A KerML `step` is a `Step` — a `Feature` that is also a `Behavior` — not a SysML
            // attribute. Sharing the attribute kind made SysML's rules judge it: `composite step
            // b : B { in x1; }` was "an attribute usage is always referential, but it states
            // `composite`" and "an attribute typed by `B`, a behavior def", neither of which is
            // about a step (`Redefinition_DirectionConformance_invalid.kerml.xt:28`).
            "step" => Kind::KermlFeature,
            "bool" => Kind::AttributeUsage,
            // `connector c { end a; end b; }` is a KerML Connector, and the projection already
            // names both metaclasses on one kind (`metaclass.rs:139`: `ConnectionUsage` and
            // `Connector`). Folding it into `AttributeUsage` made the connector an ordinary
            // feature, so its ends were read as plain ports and the rule about what an end may
            // name never saw them.
            "connector" => Kind::Connection,
            // `metaclass B :> A` — a KerML Metaclass, which is what a metadata usage names.
            // Falling into `KermlType` made `@B` name a plain classifier, and the rule that says
            // a metadata usage must name a metadata definition rejected a conforming model
            // (`MetadataTests_SemanticMetadata_valid.kerml.xt`).
            "metaclass" => Kind::MetadataDef,
            // `variant part x` / `variant def V` — one alternative of a variability point.
            "variant" => Kind::Variant,
            // The three KerML classifiers the specification states constraints between. They
            // used to fall into `KermlType` together, which discarded the only thing that tells
            // them apart.
            "multiplicity" => Kind::MultiplicityDef,
            "class" => Kind::ClassDef,
            // `behavior B :> S;` — a KerML Behavior. Sharing `KermlType` with a struct lost the
            // distinction `validateBehaviorSpecialization` is about.
            "behavior" => Kind::BehaviorDef,
            "function" | "predicate" => Kind::FunctionDef,
            "expr" => Kind::ExpressionDef,
            "datatype" => Kind::DataTypeDef,
            "struct" => Kind::StructureDef,
            "assoc" | "association" => Kind::AssocDef,
            "assoc struct" => Kind::AssocStructureDef,
            _ => Kind::KermlType,
        },
    }
}

/// A shorthand connector owns end features; the endpoint features are their reference targets,
/// not the ends themselves. Preserve fresh identities and authored order so positional
/// redefinition can replace each inherited branch without coalescing unrelated feature names.
fn project_connector_ends(el: &mut Element) {
    if !matches!(
        el.kind,
        Kind::Connection | Kind::InterfaceDef | Kind::InterfaceUsage | Kind::AllocationUsage
    ) {
        return;
    }
    let kind = if matches!(el.kind, Kind::InterfaceDef | Kind::InterfaceUsage) {
        Kind::PortUsage
    } else {
        Kind::KermlFeature
    };
    for endpoint in [&el.from, &el.to]
        .into_iter()
        .flatten()
        .chain(el.extra_ends.iter())
    {
        let endpoint = endpoint.trim();
        if endpoint.is_empty() {
            continue;
        }
        // The parser keeps an end's own multiplicity and its `::>` clause inside the text
        // (`connector_end` above), which is the shape a rule that bounds the multiplicity reads.
        // The reference is the path the end points at and only that path: pushing `[0..1] a` made
        // the reference check ask whether a feature with that literal name exists and reported
        // `valid_Connector0.sysml.xt:48` as naming nothing.
        // The bracket a connector writes before its first end belongs to the connector itself —
        // `connect [0..1] a to [1] b` is a connector that happens at most once, with a `1..1` end —
        // so no bracket here becomes the end's multiplicity. Recording it made `SEM021` and
        // `SEM033` read the connector's bound as the end's and reject `valid_Connector0`.
        //
        // The reference is what the end points at: the `::>` target when one is written
        // (`[1] cd ::> b.c.d` refers to `b.c.d`), and the endpoint path otherwise (`[0..1] a`
        // refers to `a`).
        let rest = match endpoint.find(']') {
            Some(close) if endpoint.starts_with('[') => endpoint[close + 1..].trim(),
            _ => endpoint,
        };
        // `name ::> target` (also written `name references target`) declares the end's own name.
        let (declared, reference) = match rest.split_once("::>") {
            Some((declared, reference)) => (declared.trim(), reference.trim()),
            None => ("", rest),
        };
        let mut end = Element::new(
            kind.clone(),
            if declared.is_empty() {
                "<anonymous>"
            } else {
                declared
            },
            el.line,
        );
        end.is_end = true;
        end.membership = Membership::EndFeature;
        end.references.push(reference.to_string());
        // ReferenceSubsetting supplies derived typing; no FeatureTyping was authored here.
        el.children.push(end);
    }
}

fn lower_declaration(d: &Declaration, doc: &sysml_parser::ParsedDocument) -> Element {
    lower_declaration_in(d, doc, None)
}

/// Lower a declaration whose parent's kind decides how its features are interpreted.
/// Whether an element of this kind can own parameters — that is, whether it is a behaviour.
///
/// A direction on a member means two different things depending on this: inside a behaviour it
/// marks a parameter; inside anything else it marks the feature itself (a port payload pointing
/// in or out). Getting the two confused made the tree report parameters under part definitions.
fn can_own_parameters(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::CalcDef
            | Kind::CalcUsage
            | Kind::ActionDef
            | Kind::ActionUsage
            | Kind::StateDef
            | Kind::StateUsage
            | Kind::ConstraintDef
            | Kind::ConstraintUsage
            | Kind::Perform
            | Kind::ExhibitState
            | Kind::AnalysisCaseDef
            | Kind::VerificationDef
            | Kind::VerificationUsage
            // A loop binds its variable: `for x in items { ... }` gives `x` a parameter that the
            // body reads, and the control-flow node is the owner.
            | Kind::ControlFlow
    )
}

fn lower_declaration_in(
    d: &Declaration,
    doc: &sysml_parser::ParsedDocument,
    parent_kind: Option<Kind>,
) -> Element {
    let mut kind = element_kind(&d.kind, d.is_definition);
    // A bare `end` is a Usage wherever SysML writes one, and only a KerML association's body
    // makes it a plain `Feature`. `DefaultInterfaceEnd returns SysML::PortUsage`
    // A bare `end` written in a type body is a Usage: `ConnectionDefinition::connectionEnd` is
    // declared `Usage`, and the vendored library relies on it — `Connections.sysml:43-44` writes
    // `end source: Anything` and every connection definition below inherits those ends, so reading
    // them as KerML features made `Allocations::Allocation` reject four definitions the corpus
    // states clean. The two owners whose ends are KerML features are the associations: `assoc` and
    // `assoc struct` state association ends, which is what `connection def B :> Links::BinaryLink`
    // inherits and what `SEM096` has to reject.
    //
    // A connection end is an `OccurrenceUsage` rather than a `PortUsage`. The metamodel declares
    // only `Usage`, and reading the end as a port made `validatePortUsageNestedUsagesNotComposite`
    // reject the cross-feature member it owns — six `SEM066` against `14c`. An interface end is the
    // one place the notation says `PortUsage` (`DefaultInterfaceEnd`, `SysML.xtext:1143-1145`).
    //
    // Explicit kind keywords stay untouched either way: `end part notPort : Fuel` must remain a
    // PartUsage so the interface-end property check can reject it, and `end feature f` remains a
    // KerML Feature.
    if d.kind == AstKind::Kerml("end".to_string()) {
        kind = match parent_kind {
            Some(Kind::AssocDef | Kind::AssocStructureDef) => Kind::KermlFeature,
            Some(
                Kind::ConnectionDef
                | Kind::Connection
                | Kind::AllocationDef
                | Kind::AllocationUsage,
            ) => Kind::OccurrenceUsage,
            _ => Kind::PortUsage,
        };
    }
    // the declaration is an occurrence usage, and reading it as an attribute lost both the fact that
    // it happens in time and the portion kind a rule reads.
    if matches!(
        d.head.prefixes.portion.as_deref(),
        Some("snapshot") | Some("timeslice")
    ) && d.kind == AstKind::Attribute
    {
        kind = Kind::OccurrenceUsage;
    }
    // A bare name inside an enum definition is one of its values. `enum def M { a; b; }` is as
    // legal as `enum def M { enum a; enum b; }`, and the bare spelling was reading as an
    // attribute -- so `ClassificationLevel::conf` could not resolve to anything.
    if matches!(parent_kind, Some(Kind::EnumDef))
        && kind == Kind::AttributeUsage
        && d.head.relations.is_empty()
        && d.head.value.is_none()
    {
        kind = Kind::EnumValue;
    }
    // A directed feature inside a behaviour (`in x : Real` in an action or calculation) is a
    // A directed feature is a *parameter* only inside a behaviour — a calculation, an action, a
    // state, a constraint. Anywhere else the direction is a marker on the feature itself: a port's
    // payload points in or out, and so does a directed item in a part.
    //
    // This used to turn *any* directed attribute into Kind::Parameter, whatever owned it. That made
    // `part def P { in y : Integer; }` report a parameter under a part definition and made
    // `validateParameterMembershipOwningType` fire 50 times across the official models, all of them
    // conforming. The distinction is the owner's kind, not the word.
    if d.head.prefixes.direction.is_some()
        && kind == Kind::AttributeUsage
        && parent_kind.as_ref().is_some_and(can_own_parameters)
    {
        kind = Kind::Parameter;
    }
    // A directed feature inside a `port def` stays an attribute usage on purpose. It reads like a
    // port, and drawing it as one would be closer to the notation — but `PortUsage` demands a
    // `PortDefinition` as its type, and `port def Out { out image : I; }` with `item def I` is a
    // model the reference accepts (`rules_usage_type_metaclass.rs`). A bare directed feature is a
    // reference usage in the specification, which any classifier may type, so changing the kind
    // here buys a better drawing and a false diagnostic.
    let name = d.head.name.display();
    let mut el = Element::new(
        kind.clone(),
        if name.is_empty() {
            "<anonymous>"
        } else {
            &name
        },
        d.head.span.line,
    );
    // A bare feature that stayed an attribute usage. The flag is kept to attribute usages: a bare
    // declaration that became a parameter, a part or a reference is judged by its own kind.
    el.is_bare = d.bare && kind == Kind::AttributeUsage;
    // `def` is in the kind as well (`InterfaceDef` against `InterfaceUsage`), and the flag is
    // still kept: `Element::is_definition` is what a rule reads without matching kinds, and the
    // reference judges the two forms differently ("An interface must be typed by interface
    // definitions", `InterfaceUsage_Invalid.sysml.xt:75`).
    el.is_definition = d.is_definition;
    // `variant part car` keeps its kind and records the membership here; a bare `variant v;`
    // has no inner kind and keeps `Kind::Variant`, so both forms answer the same question.
    el.is_variant = d.head.prefixes.is_variant || kind == Kind::Variant;
    // `standard library package` — the kind is the same as `library package`, and the reference
    // warns about the word (`LibraryPackage_invalid_notStandard.kerml.xt:15`).
    el.is_standard = d.head.prefixes.is_standard;
    el.visibility = d.head.prefixes.visibility.map(|v| match v {
        sysml_parser::ast::Visibility::Private => Visibility::Private,
        sysml_parser::ast::Visibility::Protected => Visibility::Protected,
        sysml_parser::ast::Visibility::Public => Visibility::Public,
    });
    // `entry` / `do` / `exit` inside a state are the state's own behaviours, and a bare `then`
    // with no name is a transition out of the enclosing state. Both were being read as ordinary
    // attributes and successions, which lost the state machine's edges.
    let in_state = matches!(kind, Kind::StateDef | Kind::StateUsage);
    el.short_name = d.head.name.short.clone();
    el.name_span = if d.head.name.is_empty() {
        None
    } else {
        Some(text_range(d.head.name.span))
    };
    if let Some(doc_text) = &d.doc {
        el.doc = Some(doc_text.clone());
    }

    // Typing versus specialization. A usage's `:` is its type; a definition's `:` clause is a
    // supertype. The parser already normalized that, so both come from the relations.
    let typing = d.head.target_of(RelationOp::Typing);
    // A multiplicity written on the type (`Wheel[2]`) is the feature's bound, not part of the
    // type's name. Keeping it in `type_ref` made every bounded feature's type unresolved.
    el.type_ref = typing.map(|t| strip_multiplicity(&t.text));
    // Keep the whole list: `part p : A, B` names two types and `target_of` returns one.
    el.type_refs = d
        .head
        .targets_of(RelationOp::Typing)
        .iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    // `end [0..1] part hub : Hub` states how many connections the end takes part in, not how many
    // values it holds: the specification fixes an end at 1..1 and puts this bound on the owned
    // cross feature. Kept in its own field so `validateFeatureEndMultiplicity` does not read it as
    // the end's own bound.
    el.cross_name = d.head.cross_name.clone();
    el.cross_multiplicity = d.head.cross_multiplicity.as_ref().map(|m| m.text.clone());
    // The bound itself is kept: `part wheels : Wheel[4]` and `part x[0..*]` both state one, and
    // dropping it left every multiplicity rule in the specification with nothing to read.
    el.multiplicity = d
        .head
        .multiplicity
        .as_ref()
        .map(|m| m.text.clone())
        .or_else(|| typing.and_then(|t| multiplicity_of(&t.text)))
        // `part redefines cyl[7]` writes the bound on the redefinition's *target*: the parser
        // keeps it there and every clause reader strips it, so a redefinition that stated a
        // bound arrived with none and the subsetting-multiplicity rule could not see it.
        .or_else(|| {
            d.head
                .targets_of(RelationOp::Redefines)
                .into_iter()
                .filter_map(|t| multiplicity_of(&t.text))
                .next()
        });
    el.is_end = d.head.prefixes.is_end;
    // `render r : R` states a view rendering membership; `rendering r : R` does not. The rule
    // that bounds a view to one rendering counts only the former (`SysML.xtext:2335-2343`).
    el.is_render = d.head.prefixes.is_render;
    el.is_initial = d.head.is_initial;
    // Whether the value was written as a *default*. `validateFeatureValueOverriding` is the one rule
    // that reads the difference, and without it `= 42` and `default = 42` are indistinguishable.
    el.is_default = d.head.is_default;
    // Every modifier the declaration wrote. `Prefixes` has always parsed these; until now the
    // lowering layer kept only `is_end` and `direction`, so a rule asking whether a feature is
    // abstract or a reference had nothing to read.
    let p = &d.head.prefixes;
    el.modifiers = Modifiers {
        is_abstract: p.is_abstract,
        is_variation: p.is_variation,
        // `individual` is a kind keyword as well as a prefix, and the metamodel derives
        // `isIndividual` from the class either way: an `IndividualDefinition` and an
        // `IndividualUsage` both have it true. Recording it only from the prefix left
        // `individual def X` with the flag false, and every rule that asks `isIndividual` had to
        // remember the kind as well.
        is_individual: p.is_individual || matches!(d.kind, AstKind::Individual),
        is_derived: p.is_derived,
        is_constant: p.is_constant,
        is_readonly: p.is_readonly,
        is_variable: p.is_variable,
        is_reference: p.is_ref,
        is_sufficient: p.is_sufficient,
        is_parallel: p.is_parallel,
        is_ordered: p.is_ordered,
        is_nonunique: p.is_nonunique,
        // The parser folds `composite` into the same slot as `portion`/`snapshot`/`timeslice`
        // (parser.rs:304), so the word itself says which of the two it is.
        is_composite: p.portion.as_deref() == Some("composite"),
        portion: p.portion.clone().filter(|w| w != "composite"),
    };
    // Every authored reference this declaration makes, with the range it occupies. A rename
    // rewrites exactly these ranges; without them the editor can only rewrite the declaration
    // and would leave the model referring to a name that no longer exists.
    for rel in &d.head.relations {
        for target in &rel.targets {
            el.ref_spans
                .push((target.text.clone(), text_range(target.span)));
        }
    }
    let supers: Vec<String> = d
        .head
        .targets_of(RelationOp::Specializes)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    // `subsets T` is a different relation from `:> T`: it states a subsetting, and the
    // specification constrains Subsetting itself (constant conformance, uniqueness conformance,
    // featuring types). Kept in its own field so those rules can see it, and appended to
    // `supertypes` below so a subsetting feature still inherits the members of what it subsets.
    let subsets: Vec<String> = d
        .head
        .targets_of(RelationOp::Subsets)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    el.subsets = subsets.clone();
    // The type-relationship parts. `datatype M unions A` and the three beside it are their own
    // relations, not supertypes; keeping them apart is what lets their rules see them.
    el.unions = d
        .head
        .targets_of(RelationOp::Unions)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    el.intersects = d
        .head
        .targets_of(RelationOp::Intersects)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    el.differences = d
        .head
        .targets_of(RelationOp::Differences)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    el.disjoints = d
        .head
        .targets_of(RelationOp::Disjoints)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    el.chains = d
        .head
        .targets_of(RelationOp::Chains)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    el.conjugates = d
        .head
        .targets_of(RelationOp::Conjugates)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    // `references T` and `crosses T` are subsettings of a specific kind. The specification bounds
    // each at one, so they are kept apart from `subsets` rather than folded in.
    el.references = d
        .head
        .targets_of(RelationOp::References)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    el.crosses = d
        .head
        .targets_of(RelationOp::Crosses)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    // `connector a : A from x to y` writes its ends on the declaration, and the rules that ask
    // what a connector or a flow connects read them from here.
    el.from = d.head.from.clone();
    el.to = d.head.to.clone();
    el.extra_ends = d.head.extra_ends.clone();
    // A usage whose type is written on the right of `:` has no supertype; a definition's
    // specialization clause is what the subtype graph needs.
    let supers = if d.is_definition || !supers.is_empty() {
        supers
    } else {
        Vec::new()
    };
    // A subsetting is also a kind of generalization for member lookup: `part g subsets A::f`
    // inherits what `A::f` provides. Appended here so the member walk keeps working; the
    // `subsets` field above is what the Subsetting rules read.
    // The written clause, before the other relations are folded in. A reader checking "does
    // this specialization name something" must use this: `supertypes` below also carries
    // subsettings and redefinitions, which resolve by different scope rules.
    el.specializes = supers.clone();
    let mut supers = supers;
    for target in &subsets {
        if !supers.contains(target) {
            supers.push(target.clone());
        }
    }
    el.supertype_ref = supers.first().cloned();
    el.supertypes = supers;

    // A redefinition names the feature being replaced. That is both a subtype relation (the
    // redefining feature is a kind of the original) and the reason its value wins. Keeping it in
    // `supertypes` is what lets a redefined part inherit the original's features.
    let redefs: Vec<String> = d
        .head
        .targets_of(RelationOp::Redefines)
        .into_iter()
        .map(|t| strip_multiplicity(&t.text))
        .collect();
    el.redefines = redefs.clone();
    if !redefs.is_empty() {
        for r in &redefs {
            if !el.supertypes.contains(r) {
                el.supertypes.push(r.clone());
            }
        }
        if el.supertype_ref.is_none() {
            el.supertype_ref = redefs.first().cloned();
        }
    }

    if let Some(v) = &d.head.value {
        el.value_expr = Some(lower_expr(v, doc));
        // The value's own text, without the `=` operator: an editor writes `= <this>`, and a
        // report quoting a value quotes the value, not the assignment.
        el.value_text = Some(render_model(&lower_expr(v, doc)));
        el.value_span = d.head.value_span.map(text_range);
    }
    el.direction = d.head.prefixes.direction.map(|dir| match dir {
        AstDirection::In => Direction::In,
        AstDirection::Out => Direction::Out,
        AstDirection::InOut => Direction::InOut,
    });

    if el.cross_name.is_some()
        || el.cross_multiplicity.is_some()
        || !d.head.cross_relations.is_empty()
    {
        let mut cross = Element::new(
            Kind::KermlFeature,
            el.cross_name.clone().unwrap_or_default(),
            el.line,
        );
        cross.membership = Membership::Owning;
        cross.multiplicity = el.cross_multiplicity.clone();
        // `end e subsets s feature f` — what the cross feature itself specializes.
        for rel in &d.head.cross_relations {
            for target in &rel.targets {
                let text = strip_multiplicity(&target.text);
                cross
                    .ref_spans
                    .push((target.text.clone(), text_range(target.span)));
                match rel.op {
                    RelationOp::Typing => cross.type_refs.push(text),
                    RelationOp::Subsets | RelationOp::Specializes => cross.subsets.push(text),
                    RelationOp::Redefines => cross.redefines.push(text),
                    RelationOp::References => cross.references.push(text),
                    RelationOp::Crosses => cross.crosses.push(text),
                    _ => {}
                }
            }
        }
        cross.type_ref = cross.type_refs.first().cloned();
        // `end cross : X[0..1] part hub` writes the cross bound on its type.
        if cross.multiplicity.is_none() {
            cross.multiplicity = d
                .head
                .cross_relations
                .iter()
                .flat_map(|rel| rel.targets.iter())
                .find_map(|target| multiplicity_of(&target.text));
        }
        // Its type is derived from the owning end, including inherited end typing. Keep that
        // relation in the semantic layer rather than inventing an authored typing clause here.
        el.children.push(cross);
    }
    project_connector_ends(&mut el);
    el.children
        .extend(lower_body_of(&d.body, doc, Some(kind.clone())));
    if d.head.prefixes.is_member {
        el.membership = Membership::Owning;
    }

    if in_state {
        // The names a transition inside this state may name as its own source. Collected before
        // the loop because the loop holds `el.children` mutably.
        let members: Vec<String> = el
            .children
            .iter()
            .filter(|c| {
                matches!(
                    c.kind,
                    Kind::StateUsage
                        | Kind::StateDef
                        | Kind::ActionUsage
                        | Kind::ActionDef
                        | Kind::PseudoState
                )
            })
            .map(|c| c.name.clone())
            .collect();
        for child in &mut el.children {
            if child.kind == Kind::AttributeUsage
                && matches!(child.name.as_str(), "entry" | "do" | "exit")
            {
                child.kind = Kind::StateAction;
            }
            // A transition's source is the element it is written in — except when the statement
            // names one: `transition off then on` carries its source in the element's name (the
            // parser reads the first word after `transition` as the name and leaves a `first`
            // clause to fill the same slot). Filling `from` with the owner there would record the
            // state machine as the source of a transition between two of its states, and every
            // consumer that draws or checks `from -> to` would be reading a different fact for
            // the two spellings of the same transition.
            if child.kind == Kind::Transition && child.from.is_none() {
                child.from = Some(if members.contains(&child.name) {
                    child.name.clone()
                } else {
                    el.name.clone()
                });
            }
        }
    }

    // A requirement's `require`/`assume` clauses become the element's predicate and assumptions,
    // because every checker downstream asks the requirement for exactly those two things.
    if matches!(
        kind,
        Kind::RequirementDef | Kind::RequirementUsage | Kind::ConstraintDef | Kind::ConstraintUsage
    ) {
        let (required, assumed) = split_constraints(&d.body, doc);
        if let Some(joined) = conjoin(required) {
            el.constraint = Some(join_text(&d.body, false));
            el.predicate = Some(joined);
        }
        if let Some(joined) = conjoin(assumed) {
            el.assumption_text = Some(join_text(&d.body, true));
            el.assumptions = Some(joined);
        }
    }

    // A calculation or constraint written with a bare trailing expression states its result or
    // its condition that way, with no `require` keyword.
    if matches!(kind, Kind::CalcDef | Kind::CalcUsage)
        || matches!(kind, Kind::ConstraintDef | Kind::ConstraintUsage)
    {
        if el.predicate.is_none() {
            let exprs: Vec<&AstExpr> = d
                .body
                .iter()
                .filter_map(|m| match m {
                    BodyMember::Statement(Statement::Expression { expr, .. }) => Some(expr),
                    BodyMember::Statement(Statement::Return { expr: Some(e), .. }) => Some(e),
                    BodyMember::Statement(Statement::ReturnDeclaration { head, .. }) => {
                        head.value.as_ref()
                    }
                    _ => None,
                })
                .collect();
            if let Some(joined) = conjoin(exprs.into_iter().map(|e| lower_expr(e, doc)).collect()) {
                el.constraint = Some(render_model(&joined));
                if matches!(kind, Kind::CalcDef | Kind::CalcUsage) {
                    el.value_expr = Some(joined.clone());
                    el.value_text = Some(render_model(&joined));
                } else {
                    el.predicate = Some(joined);
                }
            }
        }
    }

    // A metadata usage's `about` clause is the first member of its body (`parser.rs`, the
    // `metadata` declaration). Its targets are the element's `to`, the way a named comment keeps its
    // own, so the relation can be resolved and checked against the element that owns it.
    let mut about: Vec<sysml_parser::ast::TypeRef> = Vec::new();
    for member in &d.body {
        if let BodyMember::Annotation(Annotation::About { targets, .. }) = member {
            about.extend(targets.iter().cloned());
        }
    }
    if !about.is_empty() {
        el.to = Some(
            about
                .iter()
                .map(|t| t.text.clone())
                .collect::<Vec<_>>()
                .join(", "),
        );
        for target in &about {
            el.ref_spans
                .push((target.text.clone(), text_range(target.span)));
        }
    }

    el
}

/// A type reference with any trailing multiplicity removed: `Wheel[2]` becomes `Wheel`.
fn strip_multiplicity(text: &str) -> String {
    match text.find('[') {
        Some(at) => text[..at].trim_end().to_string(),
        None => text.to_string(),
    }
}

/// The bracketed bound of a type reference, brackets included: `Wheel[2]` gives `[2]`.
///
/// A type reference may carry the feature's bound (`part wheels : Wheel[4]`), and that bracket is
/// the only place some models state it.
fn multiplicity_of(text: &str) -> Option<String> {
    let at = text.find('[')?;
    let close = text[at..].find(']')? + at;
    Some(text[at..=close].to_string())
}

/// The authored text of a requirement's `require` (or `assume`) clauses, joined.
fn join_text(body: &[BodyMember], assume: bool) -> String {
    let mut out: Vec<String> = Vec::new();
    for m in body {
        if let BodyMember::Statement(Statement::Constraint {
            is_assume, exprs, ..
        }) = m
        {
            if *is_assume != assume {
                continue;
            }
            for e in exprs {
                out.push(render(e));
            }
        }
        // `assert constraint` inside a requirement states the same kind of condition.
        if !assume {
            if let BodyMember::Statement(Statement::Assert { exprs, .. }) = m {
                for e in exprs {
                    out.push(render(e));
                }
            }
        }
    }
    // Each clause is parenthesised so a nested `and` inside one cannot swallow the join. The
    // text a report prints then matches the tree that was evaluated.
    match out.len() {
        0 => String::new(),
        1 => out.pop().unwrap_or_default(),
        _ => out
            .into_iter()
            .map(|c| format!("({c})"))
            .collect::<Vec<_>>()
            .join(" and "),
    }
}

/// A requirement's or constraint's condition expressions, split into required and assumed.
fn split_constraints(
    body: &[BodyMember],
    doc: &sysml_parser::ParsedDocument,
) -> (Vec<sysml_model::Expr>, Vec<sysml_model::Expr>) {
    let mut required = Vec::new();
    let mut assumed = Vec::new();
    for m in body {
        match m {
            BodyMember::Statement(Statement::Constraint {
                is_assume, exprs, ..
            }) => {
                let bucket = if *is_assume {
                    &mut assumed
                } else {
                    &mut required
                };
                for e in exprs {
                    bucket.push(lower_expr(e, doc));
                }
            }
            BodyMember::Statement(Statement::Assert { exprs, .. }) => {
                for e in exprs {
                    required.push(lower_expr(e, doc));
                }
            }
            _ => {}
        }
    }
    (required, assumed)
}

fn conjoin(parts: Vec<sysml_model::Expr>) -> Option<sysml_model::Expr> {
    let mut parts = parts;
    match parts.len() {
        0 => None,
        1 => parts.pop(),
        _ => {
            let mut iter = parts.into_iter();
            let mut acc = iter.next().expect("non-empty");
            for next in iter {
                acc = sysml_model::Expr::Binary {
                    op: BinaryOp::And,
                    left: Box::new(acc),
                    right: Box::new(next),
                };
            }
            Some(acc)
        }
    }
}

// -- relationships -----------------------------------------------------------

fn lower_relationship(
    r: &sysml_parser::ast::Relationship,
    _doc: &sysml_parser::ParsedDocument,
) -> Option<Element> {
    use sysml_parser::ast::Relationship as R;
    match r {
        R::Import {
            visibility,
            target,
            shape,
            span,
        } => {
            let shape = match shape {
                AstImportShape::Membership { recursive } => ImportShape::Membership {
                    recursive: *recursive,
                },
                AstImportShape::Namespace { recursive } => ImportShape::Namespace {
                    recursive: *recursive,
                },
                AstImportShape::Filter => ImportShape::Filter,
            };
            let vis = visibility.map(|v| match v {
                AstVisibility::Private => Visibility::Private,
                AstVisibility::Protected => Visibility::Protected,
                AstVisibility::Public => Visibility::Public,
            });
            Some(
                Element::new(Kind::Import, target.clone(), span.line)
                    .with_type(Some(target.clone()))
                    .with_import(shape, vis),
            )
        }
        R::Alias { name, target, span } => {
            let mut el = Element::new(Kind::Alias, name.display(), span.line);
            el.name_span = Some(text_range(name.span));
            el.to = Some(target.text.clone());
            el.type_ref = Some(target.text.clone());
            Some(el)
        }
        R::Expose { target, span, .. } => {
            // `expose` is a keyword, not the element's name. Using it as the name put an element
            // called `expose` in every view's namespace and printed the statement as
            // `expose expose : Demo::power`. The target is the whole content of the statement, so
            // it lives in `to` and nowhere else: a `type_ref` would make the outline print it as a
            // type the exposure is annotated with.
            let mut el = Element::new(Kind::Expose, "<anonymous>", span.line);
            el.to = Some(target.clone());
            Some(el)
        }
        R::Satisfy {
            requirement,
            by,
            span,
        } => {
            let mut el = Element::new(Kind::Satisfy, requirement.text.clone(), span.line);
            el.to = Some(requirement.text.clone());
            el.from = by.as_ref().map(|t| t.text.clone());
            el.ref_spans
                .push((requirement.text.clone(), text_range(requirement.span)));
            if let Some(by) = by {
                el.ref_spans.push((by.text.clone(), text_range(by.span)));
            }
            Some(el)
        }
        R::Dependency {
            name,
            clients,
            suppliers,
            span,
        } => {
            let mut el = Element::new(Kind::Dependency, name.display(), span.line);
            el.name_span = if name.is_empty() {
                None
            } else {
                Some(text_range(name.span))
            };
            el.from = Some(
                clients
                    .iter()
                    .map(|t| t.text.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            el.to = Some(
                suppliers
                    .iter()
                    .map(|t| t.text.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            Some(el)
        }
        R::Specialization {
            specific,
            general,
            span,
        } => {
            // A relationship, not a declaration: both ends name types that already exist, so the
            // element has no name of its own. `from`/`to` carry the two ends the way a dependency
            // does, and the reference spans are what the name checks read.
            let mut el = Element::new(Kind::Specialization, String::new(), span.line);
            el.from = Some(specific.text.clone());
            el.to = Some(general.text.clone());
            el.ref_spans
                .push((specific.text.clone(), text_range(specific.span)));
            el.ref_spans
                .push((general.text.clone(), text_range(general.span)));
            Some(el)
        }
        R::Disjoining {
            disjoined,
            disjoining,
            span,
        } => {
            let mut el = Element::new(Kind::Disjoining, String::new(), span.line);
            el.from = Some(disjoined.text.clone());
            el.to = Some(disjoining.text.clone());
            el.ref_spans
                .push((disjoined.text.clone(), text_range(disjoined.span)));
            el.ref_spans
                .push((disjoining.text.clone(), text_range(disjoining.span)));
            Some(el)
        }
        R::Featuring {
            name,
            featured,
            featuring,
            span,
        } => {
            let label = name.as_ref().map(|n| n.display()).unwrap_or_default();
            let mut el = Element::new(Kind::TypeFeaturing, label, span.line);
            if let Some(name) = name {
                el.name_span = Some(text_range(name.span));
            }
            el.from = Some(featuring.text.clone());
            el.to = Some(featured.text.clone());
            el.ref_spans
                .push((featured.text.clone(), text_range(featured.span)));
            el.ref_spans
                .push((featuring.text.clone(), text_range(featuring.span)));
            Some(el)
        }
    }
}

// -- statements --------------------------------------------------------------

fn lower_statement(s: &Statement, doc: &sysml_parser::ParsedDocument) -> Option<Element> {
    let line = s.span().line;
    match s {
        // `require constraint c1;` / `assume constraint c2 { ... }` — a named constraint is a
        // member in its own right. The requirement's `constraint` and `assumptions` fields keep
        // the joined expression, but a name means the model can refer to this one constraint, and
        // leaving it out made the name unreachable. An unnamed clause produces nothing here.
        Statement::Constraint {
            is_assume,
            name,
            type_ref,
            exprs,
            ..
        } if !name.is_empty() => {
            let mut el = Element::new(Kind::ConstraintUsage, name.display(), line);
            el.type_ref = type_ref.as_ref().map(|t| t.text.clone());
            el.clause = Some(if *is_assume { "assume" } else { "require" }.to_string());
            el.constraint = exprs.first().map(render);
            el.predicate = exprs.first().map(|e| lower_expr(e, doc));
            el.assumptions = if *is_assume {
                el.predicate.clone()
            } else {
                None
            };
            Some(el)
        }
        Statement::Connect {
            name,
            type_ref,
            from,
            to,
            extra,
            ..
        } => {
            let mut el = Element::new(
                Kind::Connection,
                if name.is_empty() {
                    "connect".to_string()
                } else {
                    name.display()
                },
                line,
            );
            // A named connection usage is an ordinary feature of its owner: `connection
            // trailerHitch : TrailerHitch connect ...` can be read by name from anywhere its
            // owner is in scope. Only the anonymous `connect a to b;` form is a bare statement
            // with no name of its own. `name_span` records which of the two this is.
            el.name_span = if name.is_empty() {
                None
            } else {
                Some(text_range(name.span))
            };
            el.from = Some(from.clone());
            el.to = Some(to.clone());
            // The ends after the second of an n-ary connector. They are projected with the others.
            el.extra_ends = extra.clone();
            el.type_ref = type_ref.as_ref().map(|t| t.text.clone());
            // The same type in `type_refs`, which every rule that counts types reads: a
            // statement lowered here never passed through `lower_declaration_in`, where the list
            // is filled, so a connection written as a statement carried one type and no list.
            el.type_refs = el.type_ref.iter().cloned().collect();
            if let Some(t) = type_ref {
                el.ref_spans.push((t.text.clone(), text_range(t.span)));
            }
            project_connector_ends(&mut el);
            Some(el)
        }
        Statement::Flow {
            head,
            payload,
            body,
            ..
        } => {
            // A flow and its payload share the declaration-head grammar with other features.
            // Reuse that lowering so every value flag, relation and source range stays visible.
            let mut el = lower_declaration(
                &Declaration {
                    kind: AstKind::Flow,
                    is_definition: false,
                    bare: false,
                    head: head.clone(),
                    body: Vec::new(),
                    doc: None,
                },
                doc,
            );
            if head.name.is_empty() {
                el.name = "flow".into();
            }
            if let Some(payload) = payload {
                let mut feature = lower_declaration(
                    &Declaration {
                        kind: AstKind::Kerml("feature".into()),
                        is_definition: false,
                        bare: false,
                        head: payload.clone(),
                        body: Vec::new(),
                        doc: None,
                    },
                    doc,
                );
                feature.kind = Kind::PayloadFeature;
                if payload.name.is_empty() {
                    feature.name.clear();
                }
                el.children.push(feature);
            }
            el.children
                .extend(lower_body_of(body, doc, Some(Kind::FlowUsage)));
            Some(el)
        }
        Statement::Bind {
            lhs, rhs, extra, ..
        } => {
            let mut el = Element::new(Kind::Bind, "bind", line);
            el.from = Some(render(lhs));
            el.to = Some(render(rhs));
            el.extra_ends = extra.iter().map(render).collect();
            Some(el)
        }
        Statement::Assign { lhs, rhs, .. } => {
            // `assign x := v` is an `AssignmentActionUsage`, not a branch of an action: the
            // projection kept the word as a `ControlFlow` name, so a rule about an assignment had
            // nothing of its own to find.
            let mut el = Element::new(Kind::AssignmentActionUsage, "assign", line);
            el.from = Some(render(lhs));
            el.to = Some(render(rhs));
            el.constraint = Some(format!("{} := {}", render(lhs), render(rhs)));
            // The left-hand side is a reference to a feature that already exists, and the
            // reference pass only sees what is registered here. `assign i.b := null` writes a
            // dotted path, which is what `validateAssignmentActionUsageReferent` has to follow:
            // without the span, `resolve` never saw the referent and no rule could ask what it
            // names.
            if let AstExpr::Ref(name) = lhs {
                el.ref_spans
                    .push((name.text.clone(), text_range(name.span)));
            }
            Some(el)
        }
        Statement::Succession {
            name,
            first,
            then,
            trigger,
            trigger_kind,
            trigger_type,
            trigger_via,
            guard,
            effect,
            ..
        } => {
            let mut el = Element::new(
                Kind::Transition,
                if name.is_empty() {
                    "succession".to_string()
                } else {
                    name.display()
                },
                line,
            );
            el.from = first.as_ref().map(render);
            el.to = then.as_ref().map(render);
            el.trigger_text = trigger.as_ref().map(render);
            el.effect_text = effect.as_ref().map(render);
            // The expressions themselves, not only their text: a rule asks what a trigger's
            // argument *is* (a Boolean reference, a time instant) and what a `do` names.
            el.trigger_expr = trigger.as_ref().map(|e| lower_expr(e, doc));
            el.effect_expr = effect.as_ref().map(|e| lower_expr(e, doc));
            el.trigger_kind = trigger_kind.clone();
            // `accept p : T via port` — what the trigger accepts and where it arrives. Both were
            // read and dropped; the receiver goes where a message keeps its addressee.
            if let Some(t) = trigger_type {
                el.trigger_type = Some(strip_multiplicity(&t.text));
                el.ref_spans.push((t.text.clone(), text_range(t.span)));
            }
            el.addressed = trigger_via.as_ref().map(render);
            if let Some(g) = guard {
                let lowered = lower_expr(g, doc);
                el.constraint = Some(render_model(&lowered));
                el.predicate = Some(lowered);
                // A transition's guard may sit several lines below the `transition` keyword —
                // `transition` / `first S2_1` / `if "test"` / `then S2_2` writes each clause on its
                // own line. `el.line` is the keyword's, so the guard keeps its own:
                // `validateTransitionFeatureMembershipGuardExpression` has to point at the guard.
                el.predicate_line = Some(g.span().line);
            }
            Some(el)
        }
        Statement::First { target, .. } => {
            let mut el = Element::new(Kind::PseudoState, "first", line);
            el.to = target.as_ref().map(render);
            Some(el)
        }
        Statement::ControlFlow {
            keyword,
            condition,
            branches,
            ..
        } => {
            let mut el = Element::new(Kind::ControlFlow, keyword, line);
            if let Some(c) = condition {
                let lowered = lower_expr(c, doc);
                el.constraint = Some(render_model(&lowered));
                el.predicate = Some(lowered);
            }
            for (label, body) in branches {
                let mut branch = Element::new(Kind::ControlFlow, label, line);
                branch.children.extend(lower_body(body, doc));
                el.children.push(branch);
            }
            Some(el)
        }
        Statement::ControlNode {
            keyword,
            name,
            body,
            ..
        } => {
            // The declared name is the node's name: `merge continue` declares a node called
            // `continue`, and a succession `first start then continue` names it. Keeping the
            // keyword as the element name and the declared name in `to` made `continue`
            // unresolvable — the node existed under a name no other statement uses, so every
            // succession to it read as a dangling end.
            let declared = name.display();
            let mut el = Element::new(
                Kind::ControlNode,
                if declared.is_empty() {
                    keyword.clone()
                } else {
                    declared.clone()
                },
                line,
            );
            if !declared.is_empty() {
                el.to = Some(declared);
            }
            el.control_kind = Some(keyword.clone());
            el.children.extend(lower_body(body, doc));
            Some(el)
        }
        Statement::For {
            variable,
            source,
            body,
            ..
        } => {
            let mut el = Element::new(Kind::ControlFlow, "for", line);
            let mut var = Element::new(Kind::Parameter, variable.display(), line);
            var.direction = Some(Direction::In);
            // The iterated expression is the loop's range, not the variable's type. Storing it
            // as a type made every `for` loop report its range as an unresolved type name.
            var.value_text = Some(render(source));
            el.children.push(var);
            el.children.extend(lower_body(body, doc));
            Some(el)
        }
        Statement::Until { condition, .. } => {
            let mut el = Element::new(Kind::ControlFlow, "until", line);
            el.constraint = Some(render(condition));
            Some(el)
        }
        Statement::Terminate { target, .. } => {
            let mut el = Element::new(Kind::ControlFlow, "terminate", line);
            el.to = target.as_ref().map(render);
            Some(el)
        }
        // An inverting relates two features that exist elsewhere and declares nothing, so it is a
        // relationship element with both ends and their source ranges, the way a disjoining is.
        Statement::Inverting {
            inverted,
            inverting,
            ..
        } => {
            let mut el = Element::new(Kind::Inverting, String::new(), line);
            el.from = inverted.as_ref().map(|t| t.text.clone());
            el.to = inverting.as_ref().map(|t| t.text.clone());
            for end in [inverted, inverting].into_iter().flatten() {
                el.ref_spans.push((end.text.clone(), text_range(end.span)));
            }
            Some(el)
        }
        Statement::ReturnDeclaration { head, .. } => {
            // `return attribute accelerationProfile :> ... := ()` names the result slot. The name
            // was dropped in favour of the fixed `return`, so the slot was unreachable and an
            // `:=` on it read as an initial value on a non-variable.
            let slot = head.name.display();
            let mut el = Element::new(
                Kind::Parameter,
                if slot.is_empty() {
                    "return".to_string()
                } else {
                    slot
                },
                line,
            );
            // `out` unless the model wrote another direction, which is then what the parameter
            // direction rule has to see.
            el.direction = Some(match head.prefixes.direction {
                Some(AstDirection::In) => Direction::In,
                Some(AstDirection::InOut) => Direction::InOut,
                _ => Direction::Out,
            });
            el.is_initial = head.is_initial;
            el.is_default = head.is_default;
            el.clause = Some("return".to_string());
            // `return : Boolean = true` states the result's type, and dropping it left every
            // result parameter untyped. `Function::result` then had no type, so a call's value
            // could not be judged at all: `filter f((as A).y)` — and every other condition
            // written as a call — answered "cannot tell" no matter what the callee returned.
            if let Some(typing) = head.target_of(RelationOp::Typing) {
                el.type_ref = Some(strip_multiplicity(&typing.text));
            }
            el.type_refs = head
                .targets_of(RelationOp::Typing)
                .iter()
                .map(|t| strip_multiplicity(&t.text))
                .collect();
            // A result slot is assigned by the function, so it is a variable by construction.
            el.modifiers.is_variable = true;
            el.modifiers.is_ordered = head.prefixes.is_ordered;
            el.modifiers.is_nonunique = head.prefixes.is_nonunique;
            el.multiplicity = head
                .multiplicity
                .as_ref()
                .map(|m| m.text.clone())
                .or_else(|| {
                    head.target_of(RelationOp::Typing)
                        .and_then(|t| multiplicity_of(&t.text))
                });
            el.redefines = head
                .targets_of(RelationOp::Redefines)
                .iter()
                .map(|t| strip_multiplicity(&t.text))
                .collect();
            el.subsets = head
                .targets_of(RelationOp::Subsets)
                .iter()
                .map(|t| strip_multiplicity(&t.text))
                .collect();
            for rel in &head.relations {
                for target in &rel.targets {
                    el.ref_spans
                        .push((target.text.clone(), text_range(target.span)));
                }
            }
            if let Some(v) = &head.value {
                let lowered = lower_expr(v, doc);
                el.value_expr = Some(lowered.clone());
                el.value_text = Some(render_model(&lowered));
            }
            Some(el)
        }
        Statement::Return { expr, .. } => {
            let mut el = Element::new(Kind::Parameter, "return", line);
            el.direction = Some(Direction::Out);
            if let Some(e) = expr {
                let lowered = lower_expr(e, doc);
                el.value_expr = Some(lowered.clone());
                el.value_text = Some(render_model(&lowered));
            }
            Some(el)
        }
        Statement::Message {
            keyword,
            name,
            type_ref,
            addressed,
            has_payload,
            via,
            to,
            body,
            ..
        } => {
            // `accept`/`send` are `ActionUsage` metaclasses of their own; keeping them as a
            // `ControlFlow` named after the keyword is what left
            // `validateAcceptActionUsageParameters` with nothing to count. `transition` still
            // lowers here for its `then` clause and stays a control flow.
            let kind = match keyword.as_str() {
                "accept" => Kind::AcceptActionUsage,
                "send" => Kind::SendActionUsage,
                _ => Kind::ControlFlow,
            };
            let mut el = Element::new(kind, keyword, line);
            el.to = Some(name.display());
            el.type_ref = type_ref.as_ref().map(|t| t.text.clone());
            // Where the message goes. The payload is the element's own name; this is the other
            // end, and it is what a view needs to place the message on a lifeline.
            el.addressed = addressed.as_ref().map(render);
            // The ends apart, under the names the metamodel gives the arguments: a send's `via` is
            // its `senderArgument` and its `to` its `receiverArgument`; an accept's `via` is its
            // `receiverArgument`.
            if el.kind == Kind::SendActionUsage {
                el.sender = via.as_ref().map(render);
                el.receiver = to.as_ref().map(render);
            } else if el.kind == Kind::AcceptActionUsage {
                el.receiver = via.as_ref().map(render);
            }
            el.has_payload = *has_payload;
            // A send's own body: its members, including any further input parameter it declares.
            let owner = el.kind.clone();
            el.children.extend(lower_body_of(body, doc, Some(owner)));
            Some(el)
        }
        Statement::Behavior {
            keyword,
            declares,
            name,
            path,
            type_ref,
            value,
            value_span,
            body,
            ..
        } => {
            let kind = match keyword.as_str() {
                "exhibit" => Kind::ExhibitState,
                _ => Kind::Perform,
            };
            // A performance without a kind keyword references the feature it performs
            // (`PerformActionUsageDeclaration`, `SysML.xtext:1411-1425`) — that reference is what
            // the whole dotted path names, and it is what a rule asking "what does this perform"
            // has to resolve. With the keyword (`perform action x : T`) the declaration states no
            // reference at all, and the feature is the one it declares.
            //
            // The reference form declares no name either, so the element is anonymous: `perform b;`
            // beside `ref b : B` is one member called `b`, and naming the performance `b` as well
            // made every lookup of `b` from inside it ambiguous.
            let mut el = Element::new(
                kind.clone(),
                if *declares {
                    name.display()
                } else {
                    "<anonymous>".to_string()
                },
                line,
            );
            el.type_ref = type_ref.as_ref().map(|t| t.text.clone());
            if let Some(t) = type_ref {
                el.ref_spans.push((t.text.clone(), text_range(t.span)));
            }
            if !declares && !name.is_empty() {
                let mut reference = name.display();
                for segment in path.iter() {
                    reference.push('.');
                    reference.push_str(segment);
                }
                el.performs = Some(reference);
            }
            if let Some(value) = value {
                let value = lower_expr(value, doc);
                el.value_text = Some(render_model(&value));
                el.value_expr = Some(value);
                el.value_span = value_span.map(text_range);
            }
            if path.is_empty() {
                el.children.extend(lower_body(body, doc));
                return Some(el);
            }
            // `perform vehicleMassTest.collectData { ... }` refines a feature of the existing
            // performance. The body belongs under a placeholder that redefines that feature, so
            // names written inside it resolve in the refined feature's scope — `testVehicle` is
            // declared on `vehicleMassTest`, and the body says `:>> testVehicle`.
            let mut host = Element::new(kind, path[0].clone(), line);
            // What this refines is the whole dotted path (`vehicleMassTest.collectData`), not
            // just its last segment: `collectData` alone is not reachable from `scale`.
            let refined = format!("{}.{}", name.display(), path.join("."));
            host.redefines = vec![refined.clone()];
            host.supertypes = vec![refined.clone()];
            host.performs = Some(refined);
            host.children.extend(lower_body(body, doc));
            el.children.push(host);
            Some(el)
        }
        // A `require`/`assume` clause inside a requirement is folded into the requirement's own
        // predicate by `lower_declaration`; it is not a separate element.
        Statement::Constraint { .. } => None,
        // `event a;` / `event a.areal;` — an event occurrence that denotes a feature rather than
        // declaring one. It has no authored name, so it is `<anonymous>` and takes no name in its
        // namespace: `event a;` beside `ref a : A` is one member called `a`, not two. The feature
        // it denotes is kept in `performs`, which is the same slot a `perform` uses for the same
        // reason — what the element stands for is an existing feature, not a type of its own.
        Statement::Event {
            reference,
            multiplicity,
            type_ref,
            body,
            ..
        } => {
            let mut el = Element::new(Kind::EventOccurrenceUsage, "<anonymous>".to_string(), line);
            el.performs = Some(reference.text.clone());
            el.ref_spans
                .push((reference.text.clone(), text_range(reference.span)));
            el.type_ref = type_ref.as_ref().map(|t| t.text.clone());
            if let Some(t) = type_ref {
                el.type_refs = vec![t.text.clone()];
                el.ref_spans.push((t.text.clone(), text_range(t.span)));
            }
            el.multiplicity = multiplicity.as_ref().map(|m| m.text.clone());
            el.children.extend(lower_body(body, doc));
            Some(el)
        }
        Statement::Assert {
            is_negated,
            name,
            type_ref,
            type_refs,
            exprs,
            target,
            ..
        } => {
            let mut el = Element::new(
                Kind::AssertConstraint,
                if name.is_empty() {
                    "assert".to_string()
                } else {
                    name.display()
                },
                line,
            );
            el.type_ref = type_ref.as_ref().map(|t| t.text.clone());
            // Every type the clause names, not only the first, and in a list because this
            // statement never passes through `lower_declaration_in`, where `type_refs` is
            // filled: `assert constraint two_types : AConstraint, ABlock;` states two and the
            // rule that rejects it counts them.
            el.type_refs = type_refs.iter().map(|t| t.text.clone()).collect();
            if el.type_refs.is_empty() {
                el.type_refs = el.type_ref.iter().cloned().collect();
            }
            el.to = target.as_ref().map(|t| t.text.clone());
            // The assertion writes its name, so a rule that skips an unnamed member has to be
            // able to see that this one has one: `assert constraint aConstraint0 : ABlock;`
            // carries the pilot's "Duplicate of inherited member name" warning, and that
            // warning is produced by the walk over named members.
            el.name_span = if name.is_empty() {
                None
            } else {
                Some(text_range(name.span))
            };
            if let Some(joined) = conjoin(exprs.iter().map(|e| lower_expr(e, doc)).collect()) {
                el.constraint = Some(render_model(&joined));
                el.predicate = Some(joined);
            }
            if *is_negated {
                el.value_text = Some("not".to_string());
            }
            Some(el)
        }
        Statement::Objective {
            name,
            type_ref,
            body,
            ..
        } => {
            // `objective o : R { ... }` is an ObjectiveMembership, not a nested requirement. It
            // used to lower to `Kind::RequirementUsage`, which made the case rules asking "does
            // this case have more than one objective" impossible: a legal requirement nested in a
            // case body was indistinguishable from an objective.
            let mut el = Element::new(
                Kind::Objective,
                if name.is_empty() {
                    "objective".to_string()
                } else {
                    name.display()
                },
                line,
            );
            el.type_ref = type_ref.as_ref().map(|t| t.text.clone());
            el.children.extend(lower_body(body, doc));
            Some(el)
        }
        Statement::Subject {
            name,
            type_ref,
            value,
            value_span,
            is_initial,
            is_default,
            ..
        } => {
            let mut el = Element::new(Kind::Subject, name.display(), line);
            // The subject's bound (`subject : Engine[1..*]`) is the feature's multiplicity, not
            // part of the type's name. Keeping it made the type reference unresolved.
            el.type_ref = type_ref.as_ref().map(|t| strip_multiplicity(&t.text));
            if let Some(t) = type_ref {
                el.ref_spans.push((t.text.clone(), text_range(t.span)));
            }
            if let Some(value) = value {
                let value = lower_expr(value, doc);
                el.value_text = Some(render_model(&value));
                el.value_expr = Some(value);
                el.value_span = value_span.map(text_range);
            }
            el.is_initial = *is_initial;
            el.is_default = *is_default;
            Some(el)
        }
        Statement::Frame { name, type_ref, .. } => {
            // A `FramedConcernMembership`, not a `SubjectMembership`
            // (`research/upstream/mod/sysmlv2-model-0.1.2/src/generated.rs:1678`). Keeping its
            // own kind is what lets the subject-count rules exclude it.
            let mut el = Element::new(Kind::Frame, name.display(), line);
            el.type_ref = type_ref.as_ref().map(|t| strip_multiplicity(&t.text));
            if let Some(t) = type_ref {
                el.ref_spans.push((t.text.clone(), text_range(t.span)));
            }
            Some(el)
        }
        Statement::Allocate { from, to, .. } => {
            let mut el = Element::new(Kind::AllocationDef, "allocate", line);
            el.from = Some(render(from));
            el.to = Some(render(to));
            Some(el)
        }
        Statement::Include {
            name, target, body, ..
        } => {
            // `include use case i : T;` makes a use case usage `i` typed by `T`. It refers to no case,
            // so the include rule has nothing to check, and only the typing is stated. `include T`
            // names the case it includes, which is the reference the rule checks.
            let named = !name.is_empty();
            let typed = strip_multiplicity(&target.text);
            let mut el = Element::new(
                if named {
                    Kind::UseCaseUsage
                } else {
                    Kind::UseCaseDef
                },
                if named {
                    name.display()
                } else {
                    "include".to_string()
                },
                line,
            );
            // `include` borrows a use-case kind because it is a use-case member, but it is not a
            // use case: `clause` says which statement introduced it, so a rule about the included
            // target does not mistake the include itself for one.
            el.clause = Some("include".to_string());
            if named {
                el.name_span = Some(text_range(name.span));
                el.type_ref = Some(typed.clone());
                el.type_refs = vec![typed];
            } else {
                // A multiplicity on the included case (`include 'add fuel'[0..*]`) bounds the
                // include, not the case's name.
                el.to = Some(typed.clone());
                el.type_ref = Some(typed);
            }
            el.ref_spans
                .push((target.text.clone(), text_range(target.span)));
            el.children.extend(lower_body(body, doc));
            Some(el)
        }
        Statement::Expression { expr, .. } => {
            // A bare expression is a body's terminal value. It is kept as an unnamed result so
            // the checker can still see every name in it, but it is not a feature with a
            // declared value: reporting it as one made every analysis model claim a value it
            // never wrote.
            let mut el = Element::new(Kind::DefaultReference, "<anonymous>", line);
            let lowered = lower_expr(expr, doc);
            el.constraint = Some(render_model(&lowered));
            Some(el)
        }
        Statement::StateAction {
            keyword,
            declares,
            name,
            type_ref,
            body,
            ..
        } => {
            // The behaviour's own name is the element's name; the keyword is its role.
            //
            // `entry doSomethingUndefined;` names an action, and that name is what a reader looks
            // for and what a reference resolves against. Keeping the keyword as the element name
            // and the declared name in `to` meant the reference was never in the tree at all:
            // `entry nosuchaction;` was reported clean because nothing recorded the name.
            let declared = name.display();
            let mut el = Element::new(
                Kind::StateAction,
                if declared.is_empty() {
                    keyword.clone()
                } else {
                    declared.clone()
                },
                line,
            );
            if !declared.is_empty() {
                el.to = Some(declared);
            }
            // The role (`entry`, `do`, `exit`) is not the element's name once the behaviour has
            // one, and the rules about duplicate memberships ask which role a StateAction plays.
            el.clause = Some(keyword.clone());
            // A declared behaviour is not a reference to one: `entry action boot;` introduces
            // the name, and checking it against the scope would report every inline behaviour.
            if *declares {
                el.to = None;
            }
            el.type_ref = type_ref.as_ref().map(|t| t.text.clone());
            if let Some(t) = type_ref {
                el.ref_spans.push((t.text.clone(), text_range(t.span)));
            }
            el.children.extend(lower_body(body, doc));
            Some(el)
        }
        Statement::Invariant { name, exprs, .. } => {
            let mut el = Element::new(Kind::ConstraintDef, name.display(), line);
            if let Some(joined) = conjoin(exprs.iter().map(|e| lower_expr(e, doc)).collect()) {
                el.constraint = Some(render_model(&joined));
                el.predicate = Some(joined);
            }
            Some(el)
        }
    }
}

// -- annotations -------------------------------------------------------------

fn lower_annotation(a: &Annotation, _doc: &sysml_parser::ParsedDocument) -> Option<Element> {
    match a {
        Annotation::Doc { .. } => None,
        // The `about` clause of a metadata usage is read by its owner (`lower_declaration_in`).
        Annotation::About { .. } => None,
        // A named comment that says what it is about is an element of its own, so the annotations
        // it states can be resolved and checked against the element that owns it. Its targets are
        // kept as `to`, the way a metadata annotation keeps its `about` list.
        Annotation::Prose {
            keyword,
            name,
            about,
            span,
            ..
        } if keyword.as_str() == "comment" && !about.is_empty() => {
            let label = name
                .as_ref()
                .map(|n| n.display())
                .unwrap_or_else(|| "<anonymous>".to_string());
            let mut el = Element::new(Kind::Comment, label, span.line);
            if let Some(name) = name {
                el.name_span = Some(text_range(name.span));
            }
            el.to = Some(
                about
                    .iter()
                    .map(|t| t.text.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            for target in about {
                el.ref_spans
                    .push((target.text.clone(), text_range(target.span)));
            }
            Some(el)
        }
        // Prose attaches to the owning element through `Element::doc`; it is not a member of its
        // own, and reporting it would make every documented model a coverage report.
        Annotation::Prose { .. } => None,
        Annotation::Metadata {
            type_ref,
            about,
            args,
            self_terminated,
            span,
        } => {
            // `@Safety { … }` declares an anonymous metadata usage typed by `Safety`; it does not
            // declare a feature *called* Safety. Naming it after its type made it a symbol that
            // shadowed the type it names: `@Bogus { level = 3 }` resolved its own `type_ref` to
            // itself and reported nothing, so an annotation naming a metadata definition that does
            // not exist passed `check`. Anonymous keeps the name out of the scope and leaves the
            // type reference to be resolved — and reported — like any other.
            let mut el = Element::new(Kind::MetadataUsage, "<anonymous>", span.line);
            // An annotation written with its own `;` is a member of the body; one written as a
            // prefix leads the declaration after it. `attach_annotations` needs the difference.
            el.annotation_leads = !*self_terminated;
            el.type_ref = Some(type_ref.text.clone());
            if !about.is_empty() {
                el.to = Some(
                    about
                        .iter()
                        .map(|t| t.text.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            el.children.extend(lower_body(args, _doc));
            Some(el)
        }
    }
}

// -- expressions -------------------------------------------------------------

/// Lower a syntax expression to the checker-facing tree.
///
/// Operators the checker does not evaluate keep their children inside [`Expr::Opaque`] so every
/// name stays reachable. That distinction matters: a checker that treated `x->collect(f)` as an
/// unknown *name* would report faults it cannot justify, while one that dropped the subtree would
/// miss `x`.
pub fn lower_expr(e: &AstExpr, _doc: &sysml_parser::ParsedDocument) -> Expr {
    match e {
        AstExpr::Int(t, _) => t
            .parse::<i64>()
            .map(Expr::Int)
            .unwrap_or_else(|_| Expr::Real(t.replace('_', ""))),
        AstExpr::Real(t, _) => Expr::Real(t.replace('_', "")),
        AstExpr::Str(s, _) => Expr::Str(s.clone()),
        AstExpr::Bool(b, _) => Expr::Bool(*b),
        AstExpr::Null(_) => Expr::Null,
        AstExpr::Ref(r) => Expr::Ref(NameRef::new(r.text.clone(), r.span.line)),
        AstExpr::Binary { op, lhs, rhs, .. } => Expr::Binary {
            op: binary_op(op),
            left: Box::new(lower_expr(lhs, _doc)),
            right: Box::new(lower_expr(rhs, _doc)),
        },
        AstExpr::Unary { op, operand, .. } => Expr::Unary {
            op: match op.as_str() {
                "+" => UnaryOp::Plus,
                "-" => UnaryOp::Minus,
                "not" => UnaryOp::Not,
                "~" => UnaryOp::Conjugate,
                "all" => UnaryOp::All,
                _ => UnaryOp::Other,
            },
            operand: Box::new(lower_expr(operand, _doc)),
        },
        AstExpr::Call {
            callee,
            args,
            arg_names,
            ..
        } => Expr::Call {
            callee: Box::new(lower_expr(callee, _doc)),
            args: args.iter().map(|a| lower_expr(a, _doc)).collect(),
            arg_names: arg_names.clone(),
        },
        AstExpr::Conditional {
            test,
            then_expr,
            else_expr,
            ..
        } => Expr::Conditional {
            test: Box::new(lower_expr(test, _doc)),
            then_expr: Box::new(lower_expr(then_expr, _doc)),
            else_expr: Box::new(lower_expr(else_expr, _doc)),
        },
        AstExpr::Unit { value, unit, .. } => Expr::Binary {
            op: BinaryOp::Unit,
            left: Box::new(lower_expr(value, _doc)),
            right: Box::new(Expr::Ref(NameRef::new(unit.text.clone(), unit.span.line))),
        },
        AstExpr::Member { base, member, .. } => {
            // A member access is a *pair*, and this layer keeps the pair as a dotted name only
            // when the base is itself a name. Two bases break that, in opposite directions:
            //
            //   * `(x as T).y` — the cast names no operand, so `format!("{}.{}", ..)` produced the
            //     text ` as T.y`, which resolved from the enclosing scope and reported "names
            //     nothing". The previous fix dropped the member instead, which made every name
            //     after a cast legal: `(instance1 as D).zzzNoSuchMember` was accepted. A false
            //     negative is worse than the false positive it replaced, and the standard library
            //     writes this form 22 times (`Occurrences.kerml`, `Items.sysml`, `SpatialItems.sysml`
            //     …), so it cannot be left unchecked.
            //   * `D().y` — a constructor call is not a name, so `D().y` never resolves and the
            //     whole expression was reported as naming nothing, even though `D` declares `y`.
            //
            // Neither can be answered here: this layer has no type information. What it can do is
            // keep the base as an expression and hand the member along, so the reference pass —
            // which does have the type — can check the member against the base's type instead of
            // against a name built by string formatting.
            //
            // A base that is itself a name — or a chain of names — stays a dotted `Ref`, and this is
            // deliberate. `v1.S` names `S` in `v1`'s scope even when `S` is not a feature of `v1`'s
            // type, and the pilot reports exactly that ("Must be a valid feature",
            // `FeatureChain_invalid.kerml.xt:61`). Turning it into a `Member` made the member
            // resolvable only through `v1`'s type, so a scope-sibling like `S` stopped being
            // reported at all — a diagnostic lost.
            //
            // `c.w.radius` is the same shape two levels deep: its base is `c.w`, which is itself a
            // name chain, so the whole path stays one `Ref`. Only a base that is *not* a name — a
            // call, a constructor, a cast — needs the pair.
            if is_name_chain(base) {
                let text = format!("{}.{}", render(base), member);
                return Expr::Ref(NameRef::new(text, e.span().line));
            }
            Expr::Member {
                base: Box::new(lower_expr(base, _doc)),
                member: member.clone(),
            }
        }
        AstExpr::Sequence { items, .. } => {
            Expr::Sequence(items.iter().map(|i| lower_expr(i, _doc)).collect())
        }
        AstExpr::TypeTest {
            kind,
            operand,
            type_name,
            span,
        } => Expr::TypeCheck {
            kind: match kind {
                sysml_parser::ast::TypeTestKind::Istype => sysml_model::TypeCheckKind::IsType,
                sysml_parser::ast::TypeTestKind::Hastype => sysml_model::TypeCheckKind::HasType,
                // `as` is a cast, not a test: it yields the operand read as the named type. It used
                // to be folded into `IsType`, which typed `(x as T).member`'s base as `Boolean`, so
                // the member was looked for in `Boolean` and never found.
                sysml_parser::ast::TypeTestKind::As => sysml_model::TypeCheckKind::As,
                sysml_parser::ast::TypeTestKind::Meta => sysml_model::TypeCheckKind::Meta,
                sysml_parser::ast::TypeTestKind::MetaTest => sysml_model::TypeCheckKind::MetaTest,
            },
            operand: Box::new(lower_expr(operand, _doc)),
            type_name: NameRef::new(type_name.text.clone(), span.line),
        },
        // A chain operation (`x->collect(f)`) has no evaluator. Its base is still checked as a
        // name; its arguments are kept, and a lambda's parameter names are recorded so the
        // checker does not report them as unresolved model references.
        AstExpr::ChainOp {
            base,
            op,
            args,
            params,
            ..
        } => Expr::Chain {
            op: op.clone(),
            base: Box::new(lower_expr(base, _doc)),
            args: args.iter().map(|a| lower_expr(a, _doc)).collect(),
            params: params.clone(),
        },
        AstExpr::Constructor {
            type_ref,
            args,
            arg_names,
            ..
        } => Expr::Constructor {
            type_ref: NameRef::new(type_ref.text.clone(), type_ref.span.line),
            args: args.iter().map(|a| lower_expr(a, _doc)).collect(),
            arg_names: arg_names.clone(),
        },
        AstExpr::Opaque { children, .. } => {
            Expr::Opaque(children.iter().map(|c| lower_expr(c, _doc)).collect())
        }
    }
}

fn binary_op(op: &str) -> BinaryOp {
    match op {
        "==" | "===" => BinaryOp::Eq,
        "!=" | "!==" => BinaryOp::NotEq,
        "<" => BinaryOp::Less,
        "<=" => BinaryOp::LessEq,
        ">" => BinaryOp::Greater,
        ">=" => BinaryOp::GreaterEq,
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        "*" => BinaryOp::Mul,
        "/" => BinaryOp::Div,
        "%" => BinaryOp::Rem,
        "**" | "^" => BinaryOp::Pow,
        "and" | "&" => BinaryOp::And,
        "or" | "|" => BinaryOp::Or,
        "xor" => BinaryOp::Xor,
        "implies" => BinaryOp::Implies,
        "??" => BinaryOp::NullCoalesce,
        ".." => BinaryOp::Other,
        _ => BinaryOp::Other,
    }
}

/// Render an expression to the canonical single-line form reports use.
/// Whether an expression is nothing but a dotted name — `c`, `c.w`, `a.b.c`.
///
/// Used to decide whether a member access can stay a dotted [`Expr::Ref`] rather than becoming an
/// [`Expr::Member`]. A chain of names denotes a path in the scope graph, which the path walk resolves;
/// anything else (a call, a constructor, a cast) denotes a *value*, whose type is what decides what
/// its members are.
fn is_name_chain(e: &AstExpr) -> bool {
    match e {
        AstExpr::Ref(_) => true,
        AstExpr::Member { base, .. } => is_name_chain(base),
        _ => false,
    }
}

pub fn render(e: &AstExpr) -> String {
    sysml_parser::parser::render_expr(e)
}

/// Render a lowered expression.
pub fn render_model(e: &sysml_model::Expr) -> String {
    e.render()
}

// -- project assembly --------------------------------------------------------

/// Parse and lower one source buffer.
///
/// # Errors
///
/// Returns the parser's message when the text is not valid SysML. The library is not loaded, so a
/// model that imports one of its names does not fail here; it resolves to nothing later.
pub fn load(src: &str) -> Result<Loaded, String> {
    load_project(&[src], false)
}

/// Parse and lower several buffers as one project.
///
/// # Errors
///
/// Returns the parser's message when a buffer is not valid SysML. A library that cannot be
/// discovered falls back to the bundled copy rather than failing, so a missing library is not an
/// error here; pin one with `load_project_at` when that matters.
pub fn load_project(sources: &[&str], with_stdlib: bool) -> Result<Loaded, String> {
    load_project_at(sources, with_stdlib, None)
}

/// Load a project against a named library, when the caller has one to name.
///
/// A project manifest can pin the library its model was written against. Pinning is worth doing
/// because the discovered library is whatever sits beside the binary: the same model checked on
/// another machine, or after the library directory moves, can be checked against a different one
/// and report a different set of problems.
pub fn load_project_at(
    sources: &[&str],
    with_stdlib: bool,
    library_dir: Option<&std::path::Path>,
) -> Result<Loaded, String> {
    if let Some(dir) = library_dir {
        // An explicit directory wins over `SYSML_LIBRARY=off`: a manifest that names a library is
        // stating which one its model was written against, and an environment variable must not
        // quietly answer a different question.
        let lib =
            library::LibrarySources::read(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        return load_with_library(sources, &lib);
    }
    if with_stdlib {
        // `SYSML_LIBRARY=off` forces the bundled stub, so a conformance run can isolate the model
        // from the library.
        if std::env::var("SYSML_LIBRARY").ok().as_deref() == Some("off") {
            return load_with_bundled(sources, Some(STDLIB));
        }
        if let Some(dir) = library::discover(None) {
            if let Ok(lib) = library::LibrarySources::read(&dir) {
                return load_with_library(sources, &lib);
            }
        }
    }
    load_with_bundled(sources, with_stdlib.then_some(STDLIB))
}

/// Load a project, attaching extra source files as library rather than as roots.
///
/// The official validation corpus is one project split across directories: `04-Functional
/// Allocation` imports `3a-Function-based Behavior-1`, which lives two directories away. Loading
/// a single file in isolation makes those imports unresolvable, and the model is not wrong — the
/// load was. This is the entry that lets a caller say which other files belong to the same
/// project without also making them roots the report audits.
///
/// # Errors
///
/// Returns the parser's message when a root or an extra buffer is not valid SysML.
pub fn load_project_with_extra(sources: &[&str], extra: &[&str]) -> Result<Loaded, String> {
    load_project_with_extra_at(sources, extra, None, false)
}

/// Load a project and its extra files against a named library.
///
/// `bundled` forces the stub compiled into the binary, which is what a manifest asks for when it
/// says `"stdlib": "bundled"` — a model written against the stub should be checked against it, not
/// against whichever library happens to be discovered.
pub fn load_project_with_extra_at(
    sources: &[&str],
    extra: &[&str],
    library_dir: Option<&std::path::Path>,
    bundled: bool,
) -> Result<Loaded, String> {
    let mut loaded = if bundled {
        load_with_bundled(sources, Some(STDLIB))?
    } else {
        load_project_at(sources, true, library_dir)?
    };
    for src in extra {
        // Extra files are lowered as library: reachable by name, not audited as part of the
        // model under review. Their ids continue after the current total so one index holds all.
        let mut next = loaded.roots.iter().map(Element::size).sum::<usize>()
            + loaded.library.iter().map(Element::size).sum::<usize>();
        let mut extra_elements = parse_and_lower(src, usize::MAX)?;
        fill_feature_defaults(&mut extra_elements);
        for el in &mut extra_elements {
            assign_ids(el, &mut next);
        }
        loaded.library.extend(extra_elements);
    }
    Ok(loaded)
}

fn parse_and_lower(src: &str, file: usize) -> Result<Vec<Element>, String> {
    CURRENT_FILE.with(|c| c.set(file));
    let doc = sysml_parser::parse(src).map_err(|e| e.to_string())?;
    Ok(lower_document(&doc))
}

fn load_with_bundled(sources: &[&str], bundled: Option<&str>) -> Result<Loaded, String> {
    let mut roots = Vec::new();
    for (file, src) in sources.iter().enumerate() {
        roots.extend(parse_and_lower(src, file)?);
    }
    // Before `inherit_ports`, so a copied port carries the value the definition's port computed.
    fill_feature_defaults(&mut roots);
    inherit_ports(&mut roots);
    let mut library = Vec::new();
    if let Some(text) = bundled {
        // The bundled library is not one of the caller's buffers, so its spans get an index no
        // caller can resolve. Nothing should edit the stdlib through a project load.
        library = parse_and_lower(text, sources.len()).unwrap_or_default();
        fill_feature_defaults(&mut library);
    }
    CURRENT_FILE.with(|c| c.set(0));
    let source = if bundled.is_some() {
        LibrarySource::Bundled
    } else {
        LibrarySource::None
    };
    finish(roots, library, Vec::new(), source)
}

fn load_with_library(sources: &[&str], lib: &library::LibrarySources) -> Result<Loaded, String> {
    let mut roots = Vec::new();
    for (file, src) in sources.iter().enumerate() {
        roots.extend(parse_and_lower(src, file)?);
    }
    fill_feature_defaults(&mut roots);
    inherit_ports(&mut roots);
    let mut library = Vec::new();
    let mut skipped = Vec::new();
    for (path, text) in &lib.files {
        let mut els = match parse_and_lower(text, usize::MAX) {
            Ok(els) => els,
            Err(e) => {
                skipped.push((path.clone(), e));
                continue;
            }
        };
        fill_feature_defaults(&mut els);
        library.extend(els);
    }
    CURRENT_FILE.with(|c| c.set(0));
    finish(
        roots,
        library,
        skipped,
        LibrarySource::Full {
            root: lib.root.clone(),
            files: lib.files.len(),
        },
    )
}

fn finish(
    mut roots: Vec<Element>,
    mut library: Vec<Element>,
    library_skipped: Vec<(std::path::PathBuf, String)>,
    library_source: LibrarySource,
) -> Result<Loaded, String> {
    let mut next = 0usize;
    for root in &mut roots {
        assign_ids(root, &mut next);
    }
    for element in &mut library {
        assign_ids(element, &mut next);
    }
    let element_count = roots.iter().map(Element::size).sum();
    Ok(Loaded {
        roots,
        library,
        library_skipped,
        library_source,
        element_count,
    })
}

fn assign_ids(el: &mut Element, next: &mut usize) {
    el.id = *next;
    *next += 1;
    for child in &mut el.children {
        assign_ids(child, next);
    }
}

/// Fill the effective `Element::is_composite` of every feature.
///
/// The specification derives `isReference = not isComposite` for a `Usage`
/// (`deriveUsageIsReference`, `generated.rs:8957-8961`) and writes six well-formedness constraints
/// against that value. `Modifiers::is_composite` records only that the word `composite` appeared,
/// and no SysML usage can write it — the usage prefix is `direction? 'derived'? ('abstract' |
/// 'variation')? 'constant'? 'ref'?` (`SysML.xtext:556-566`). A tree that stores only the written
/// word therefore answers "not composite" for `part wheel : Wheel;`, and every rule that reads
/// `isReference` passes a model it should judge.
///
/// The pilot models state the value the rules need. `PortUsage_Invalid.sysml.xt:35` expects
/// "Owned usages of a port definition (other than ports) must be referential" at `part b2 : B;`
/// inside `port def pd1` — the violation exists only because a part owned by a port definition is
/// composite, so `validatePortDefinitionOwnedUsagesNotComposite` has nothing to reject in a tree
/// where it is not.
///
/// Each arm below rests on a rule or derivation the specification states, in the order that decides
/// a declaration writing more than one word:
///
/// 1. `ref` — `isReference ?= 'ref'` (`SysML.xtext:565`). The notation's own word for "the owner
///    only refers to this", and it wins over a co-written `composite` because the reference
///    implementation tests it first (`build.rs:2243-2248`).
/// 2. `composite` — written outright, `isComposite ?= 'composite'` (`KerML.xtext:518`). A model
///    that states the value is not overruled by a default; the constraints that forbid it
///    (`validatePortUsageIsReference`, `validateAttributeUsageIsReference`,
///    `validateUsageIsReferential`) are the ones that report it.
/// 3. A `ReferenceUsage` — `validateReferenceUsageIsReference`, and the metaclass redefines the
///    property with `default: true` (`generated.rs:2491-2493`).
/// 4. An end — `validateFeatureEndNotDerivedAbstractCompositeOrPortion` states
///    `isEnd implies not (... isComposite ...)`, so a composite end is already invalid.
/// 5. A directed feature — a direction makes it a parameter (`ParameterMembership specializes
///    FeatureMembership`, `generated.rs:1723`), and a parameter's value is supplied by its caller
///    rather than owned here. Without this arm every `in x : Real` in a behaviour would come out
///    composite and `validateUsageIsReferential` would report the whole corpus; those models are
///    legal, so the arm is the reading their verdict requires.
/// 6. An `AttributeUsage` — `validateAttributeUsageIsReference: isReference`, "an attribute usage
///    is always referential".
/// 7. A `PortUsage` — `validatePortUsageIsReference` requires a port to be referential unless its
///    owning type is a port definition or a port usage, so a port is composite exactly when it is
///    nested in a port.
/// 8. A variant member — owned through a `VariantMembership` (`generated.rs:1772`), not a
///    `FeatureMembership`, so the owner is not made of it.
/// 9. Anything else: composite when a type owns it as a feature, and not otherwise. A package is a
///    `Namespace` and not a `Type`, so a usage declared at package level has no owning type.
fn fill_feature_defaults(elements: &mut [Element]) {
    fn go(el: &mut Element) {
        derive_modifiers(el);
        derive_membership(el);
        // The effective `isReference`. Two sources, one flag: the authored `ref` word (which
        // `modifiers.is_reference` also keeps, for a rule that must know what was written) and the
        // kind's own constant. `effective_reference` documents each arm.
        el.is_reference = effective_reference(el);
        let owner = el.kind.clone();
        for child in &mut el.children {
            child.is_composite = effective_composite(child, Some(&owner));
            derive_membership(child);
            go(child);
        }
    }
    for el in elements.iter_mut() {
        el.is_composite = effective_composite(el, None);
        derive_modifiers(el);
        derive_membership(el);
        el.is_reference = effective_reference(el);
        go(el);
    }
}

/// Which membership introduced this element, from what the declaration wrote.
///
/// The projection already knew this in three unrelated places — the kind ([`Kind::Parameter`],
/// [`Kind::Objective`], [`Kind::Subject`], …), [`Element::clause`] (`require`, `assume`, `return`)
/// and [`Element::is_end`]. A rule that asks "how was this member introduced" had to know which of
/// the three to consult, and a rule that counts memberships had nothing to count. This puts the
/// answer in one field without changing what the kind means.
///
/// The order matters where a declaration states two things: an `end` is an `EndFeatureMemberhip`'s
/// member before anything else (`end in x : T`), and a `return` clause is a
/// `ReturnParameterMembership` rather than the plain `ParameterMembership` the parameter kind would
/// otherwise suggest.
fn derive_membership(el: &mut Element) {
    if el.membership == Membership::Owning {
        return;
    }
    el.membership = if el.is_end {
        Membership::EndFeature
    } else if el.clause.as_deref() == Some("return") {
        Membership::ReturnParameter
    } else if matches!(el.clause.as_deref(), Some("require") | Some("assume")) {
        Membership::RequirementConstraint
    } else {
        match el.kind {
            // A directed feature is a *parameter* by its direction — `Step::parameter redefines
            // directedFeature` — and is owned through an ordinary `FeatureMembership`. The
            // `ParameterMembership`s the notation writes are the message payloads and call
            // arguments, which this projection does not build yet; mapping a directed feature to
            // one here would let a rule judge a membership the model never wrote.
            Kind::Objective => Membership::Objective,
            Kind::Frame => Membership::FramedConcern,
            Kind::StateAction => Membership::StateSubaction,
            Kind::Subject => Membership::Subject,
            Kind::Actor => Membership::Actor,
            Kind::Stakeholder => Membership::Stakeholder,
            Kind::Variant => Membership::Variant,
            _ => Membership::Feature,
        }
    };
}

/// The modifier values the notation implies, filled before any rule reads them.
///
/// `variation` and `abstract` are alternatives of **one** prefix slot: `DefinitionPrefix` is
/// `isAbstract ?= 'abstract' | isVariation ?= 'variation'` (`SysML.xtext:491`) and `RefPrefix` is
/// the same at `:559`. A declaration therefore writes one word or the other and never both, so
/// `variation x` states the abstract variant of the declaration — which is what the
/// specification's own `isVariation implies isAbstract` holds of, and what the official models
/// show: they write `variation part v;` with no `abstract` and are legal.
///
/// Without this fill the flag said `isVariation = true, isAbstract = false`, and the two rules
/// above reported every variation point in the corpus. The value is filled rather than the rules
/// withdrawn, because the rules are right about the metamodel and the tree was wrong about the
/// notation.
///
/// # One default the metamodel states and the notation never writes
///
/// `ConnectionDefinition::isSufficient` redefines the flag to a constant `true` in the
/// specification's own metamodel, so a `connection def` that writes no `all` word still has it
/// set, and `validateConnectionDefinitionIsSufficient` can never fail against a tree that leaves it
/// `false`. Filling it here is what makes that rule decidable; filing it "not applicable" instead
/// says the specification states something it does not.
///
/// This is a default of the *metaclass*, not of the notation, which is why it lives here beside the
/// notation fill rather than in a rule: every reader of the flag should see the same value.
///
/// # The enumeration default, and why it is not here
///
/// `EnumerationDefinition::isVariation` is redefined to `true` the same way, and filling it looked
/// like the identical one-line change. It is not: `variation_members` reads `is_variation` to mean
/// "every member must be a `Variant`", so setting the flag on every enumeration definition made
/// every ordinary enumeration literal a violation. Measured: pilot `spurious` went 0 → 7 and the
/// models the pilot calls clean fell 41/41 → 40/41, all of them
/// `EnumerationUsage_invalid::E::a`/`::b` "is an ordinary member of the variation". The metamodel's
/// redefinition is about the *definition* being a variation point; the rule in this kernel reads the
/// flag as a constraint on members, which is a different proposition. Filling the flag without
/// reconciling that reading trades one wrong answer for seven.
///
/// So `validateEnumerationDefinitionIsVariation` stays filed as not applicable, with that reason,
/// rather than being "fixed" by a default that breaks seven models.
///
/// # An event occurrence, which is the case the same argument supports
///
/// `EventOccurrenceUsage::isReference` is redefined to `true` with `derived: true`
/// (`generated.rs:2212`), and — unlike the enumeration flag — the notation says the same thing
/// independently: the production writes `ownedRelationship += OwnedReferenceSubsetting`
/// (`SysML.xtext:867`), so an `event a;` is a reference to an event rather than a part of its owner.
/// The tree already acted on that reading in one place, hard-coding `effective_composite` to `false`
/// for the kind (`lower.rs:2296`, naming this very rule) without ever setting the flag itself, so
/// `validateEventOccurrenceUsageIsReference` — whose OCL is the single word `isReference` — had
/// nothing to read. Filling it here retires that workaround and makes the rule decidable from the
/// flag rather than from a special case in the composite question.
///
/// The two redefinitions differ in exactly the way that matters: `isVariation` on an enumeration
/// definition is a claim about the definition being a variation *point*, which this kernel reads as a
/// constraint on its members and would therefore mis-apply; `isReference` on an event occurrence is a
/// claim about the occurrence itself, and the production says the same thing in the same direction.
fn derive_modifiers(el: &mut Element) {
    if el.modifiers.is_variation {
        el.modifiers.is_abstract = true;
    }
    // `ConnectionDefinition::isSufficient` is `true` by definition: a connection carries all the
    // values its ends do, which is what sufficiency states.
    if el.kind == Kind::ConnectionDef {
        el.modifiers.is_sufficient = true;
    }
}

/// One feature's effective `isReference`.
///
/// The flag is `true` from two independent sources, and the difference between them is the whole
/// reason [`Element::is_reference`] exists apart from [`Modifiers::is_reference`]:
///
///   * the declaration wrote `ref` — a word about this declaration, which is all
///     `modifiers.is_reference` records;
///   * the element's metaclass redefines the property to a constant `true`. Three do:
///     `AttributeUsage` (`generated.rs:2033`), `EventOccurrenceUsage` (`:2212`) and
///     `ReferenceUsage` (`:2492`), each with `derived: true`.
///
/// Only the second is filled here, because the first already arrives on the modifier and is copied
/// into the field for the reader's convenience. An `event` usage is skipped by `derive_modifiers`'
/// own arm and would otherwise need setting twice; reading it here keeps one place that answers the
/// question.
///
/// `ReferenceUsage` has no arm: this kernel has no such kind, so no element can reach it. That is
/// the entry `validateReferenceUsageIsReference` describes, and it stays a representation gap rather
/// than a default.
fn effective_reference(el: &Element) -> bool {
    if el.modifiers.is_reference {
        return true;
    }
    matches!(el.kind, Kind::AttributeUsage | Kind::EventOccurrenceUsage)
}

/// One feature's effective `isComposite`, given the kind of what owns it. The arms and the rule
/// behind each one are listed on [`fill_feature_defaults`].
fn effective_composite(el: &Element, owner: Option<&Kind>) -> bool {
    if el.modifiers.is_reference || el.kind == Kind::Ref {
        return false;
    }
    if el.modifiers.is_composite {
        return true;
    }
    if el.is_end || el.direction.is_some() {
        return false;
    }
    if el.kind == Kind::AttributeUsage {
        return false;
    }
    // `validateEventOccurrenceUsageIsReference`: an `EventOccurrenceUsage` references the event it
    // denotes — the production writes `ownedRelationship += OwnedReferenceSubsetting`
    // (`SysML.xtext:867`) — so it is not part of what its owner is made of.
    if el.kind == Kind::EventOccurrenceUsage {
        return false;
    }
    if el.kind == Kind::PortUsage {
        return matches!(owner, Some(Kind::PortDef) | Some(Kind::PortUsage));
    }
    // `VariantMembership specializes OwningMembership` (`generated.rs:1772`), not
    // `FeatureMembership`, so a variant alternative is outside its owner's `feature`. Both
    // spellings: a bare `variant v` keeps `Kind::Variant`, and `variant part car` keeps
    // `Kind::PartUsage` with the flag set.
    if el.is_variant {
        return false;
    }
    // An annotation describes the element it is attached to; it is not part of what that element is
    // made of. KerML owns it through `annotatingElement`, not through an owned feature, so applying
    // `@Safety{level = 3}` to an attribute must not make the attribute composite.
    // `14c-Language-Extensions.sysml` annotates `req3::sil` and
    // `validateAttributeUsageOwnedFeaturesNotComposite` rejected the model when the annotation
    // started arriving as a child.
    if el.kind == Kind::MetadataUsage {
        return false;
    }
    // A feature that states a condition or a relationship rather than a value the owner is made
    // of. `assert constraint` is a condition on the owner (`15_01-Constants.sysml:24` writes
    // `attribute e: Real { assert constraint { ... } }`, and `validateAttributeDefinitionFeatures`
    // would reject that legal model if the assertion were composite); `perform` and `exhibit` name
    // a behaviour or a state the owner takes part in (`6-Individual and Snapshots.sysml:113` writes
    // `exhibit vehicleStates.on` inside a snapshot); `bind` equates two of the owner's features.
    // None of them adds a part.
    if matches!(
        el.kind,
        Kind::AssertConstraint | Kind::Perform | Kind::ExhibitState | Kind::Bind
    ) {
        return false;
    }
    match owner {
        Some(owner) if owner.is_type() && el.kind.is_feature() => true,
        _ => false,
    }
}

/// Copy a definition's ports onto the usages typed by it.
///
/// A `part d : Drone` has no children of its own, and a view that draws `d` needs the ports
/// `Drone` declares. The copy is marked `inherited` so a reader can tell an authored port from an
/// inherited one.
fn inherit_ports(roots: &mut [Element]) {
    fn collect(el: &Element, defs: &mut HashMap<String, Vec<Element>>) {
        if el.kind.is_definition() {
            let ports: Vec<Element> = el
                .children
                .iter()
                .filter(|c| c.kind == Kind::PortUsage)
                .cloned()
                .collect();
            if !ports.is_empty() {
                defs.entry(el.name.clone()).or_insert(ports);
            }
        }
        for child in &el.children {
            collect(child, defs);
        }
    }

    fn apply(el: &mut Element, defs: &HashMap<String, Vec<Element>>) {
        if el.kind == Kind::PartUsage {
            if let Some(type_name) = el.type_ref.clone() {
                let leaf = type_name
                    .rsplit("::")
                    .next()
                    .unwrap_or(&type_name)
                    .to_string();
                if let Some(ports) = defs.get(&type_name).or_else(|| defs.get(&leaf)) {
                    let existing: Vec<String> = el
                        .children
                        .iter()
                        .filter(|c| c.kind == Kind::PortUsage)
                        .map(|c| c.name.clone())
                        .collect();
                    for port in ports {
                        if !existing.contains(&port.name) {
                            let mut inherited = port.clone();
                            inherited.inherited = true;
                            el.children.push(inherited);
                        }
                    }
                }
            }
        }
        for child in &mut el.children {
            apply(child, defs);
        }
    }

    let mut defs = HashMap::new();
    for root in roots.iter() {
        collect(root, &mut defs);
    }
    for root in roots.iter_mut() {
        apply(root, &mut defs);
    }
}

const STDLIB: &str = include_str!("../stdlib.sysml");
