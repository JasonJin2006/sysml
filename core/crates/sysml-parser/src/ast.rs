//! Syntax tree for SysML v2 and KerML textual notation.
//!
//! Shaped for lowering, not for mirroring the OMG metamodel. Every node keeps the authored text
//! it came from and the line it started on, because the two things every downstream consumer
//! needs are "what did the author write" and "where do I point when it is wrong".
//!
//! # One node per thing the language lets you write
//!
//! A definition and a usage of the same kind share a node, distinguished by [`Definition`]
//! versus [`Usage`] wrappers around a shared [`Head`]. This is the shape the grammar has —
//! `PartDefinition = 'part' 'def' DefinitionDeclaration DefinitionBody` and
//! `PartUsage = 'part' UsageDeclaration UsageBody` differ only in the `def` and which body
//! production they use — and keeping it makes the lowering layer one function per kind rather
//! than two.
//!
//! # What is *not* normalized
//!
//! A name keeps its quoting. `'vehicle-trailer system'` and `vehicleTrailerSystem` are different
//! authored spellings and a diagnostic has to quote the one that was written. The reference
//! text of a type is kept whole (`ISQ::TorqueValue`), because trimming to the last segment
//! destroys the only information a resolver needs to tell two same-named definitions apart.

use crate::lexer::Token;

/// A half-open byte range in the source buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    /// 1-based line of `start`.
    pub line: u32,
}

impl Span {
    pub fn new(tok: &Token) -> Self {
        Self {
            start: tok.start,
            end: tok.end,
            line: tok.line,
        }
    }

    /// A span covering both inputs, taking the start of `self` and the end of `other`.
    pub fn to(self, other: Span) -> Self {
        Self {
            start: self.start,
            end: other.end.max(self.end),
            line: self.line,
        }
    }
}

/// A declared name: the simple name plus the optional short name.
///
/// SysML lets one declaration carry both (`<B> byte : StorageCapacityUnit`), and a reference may
/// use either. Keeping both means a model that refers to `B` and one that refers to `byte` are
/// both resolvable, which a single-name field cannot do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Name {
    /// The authored name, unquoted. Empty for an anonymous declaration.
    pub text: String,
    /// `<B>` when written.
    pub short: Option<String>,
    pub span: Span,
}

impl Name {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.short.is_none()
    }

    /// The name a reference is most likely to use: the simple name, or the short name when
    /// that is all there is.
    pub fn display(&self) -> String {
        if !self.text.is_empty() {
            self.text.clone()
        } else {
            self.short.clone().unwrap_or_default()
        }
    }
}

/// A visibility prefix on a membership.
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

/// A documented relationship to one or more target elements.
///
/// `: T` (typing), `:> T` (specialization/subsetting), `:>> T` (redefinition), and `::> T`
/// (reference subsetting) are the four the language distinguishes. Keeping the operator means
/// the lowering layer can tell a type from a supertype, which is the difference between "what
/// this is an instance of" and "what this is a kind of".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationOp {
    /// `: T` / `typed by T` — the type of a feature.
    Typing,
    /// `:> T` / `specializes T` — a supertype.
    Specializes,
    /// `subsets T` / `subset T` — a subsetting clause.
    ///
    /// Kept apart from [`RelationOp::Specializes`] because the specification states constraints on
    /// `Subsetting` that do not apply to `Specialization` (`validateSubsettingConstantConformance`,
    /// `validateSubsettingUniquenessConformance`, `validateSubsettingFeaturingTypes`), and while
    /// the two shared one operator the element tree could not tell them apart.
    Subsets,
    /// `:>> T` / `redefines T`.
    Redefines,
    /// `::> T` / `references T`.
    References,
    /// `crosses T`.
    Crosses,
    /// `unions T` / `intersects T` / `differences T` / `disjoint from T` — the type-relationship
    /// parts that follow a declaration's specialization clause
    /// (`KerML.xtext:340-365`, `TypeRelationshipPart`).
    Unions,
    Intersects,
    Differences,
    Disjoints,
    /// `~T` / `conjugates T`.
    Conjugates,
    /// `chains a.b` — the features a feature chain passes through.
    Chains,
}

/// One relationship clause, with every target written for it.
///
/// A clause can name several targets (`:> A, B`), which is multiple inheritance or a subset of
/// several features. Keeping the vector rather than its first element is what makes `C :> A, B`
/// a subtype of both.
#[derive(Debug, Clone, PartialEq)]
pub struct Relation {
    pub op: RelationOp,
    pub targets: Vec<TypeRef>,
    pub span: Span,
}

/// A reference to a type or feature, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    /// The authored spelling: `ISQ::TorqueValue`, `'A B'::C`.
    pub text: String,
    pub span: Span,
}

/// A multiplicity bound: `[2]`, `[0..*]`, `[*]`, `[1..3]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Multiplicity {
    pub text: String,
    pub span: Span,
}

/// A prefix on a feature: everything that can come before its kind keyword.
///
/// `in`/`out`/`inout` for direction, `abstract`/`variation` for a definition, `individual`,
/// `derived`, `constant`, `readonly`, `end`, and the `ref` that makes a feature point at
/// something rather than own it. Collected into one struct because the grammar treats them as
/// one optional run and the lowering layer reads them as one set of flags.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Prefixes {
    pub direction: Option<Direction>,
    pub is_abstract: bool,
    pub is_variation: bool,
    /// `variant part a` — one alternative of a variability point.
    ///
    /// A membership prefix, not a kind: `variant part a` is a part usage that is a variant, and
    /// `variant b1` states no kind at all. Keeping it here rather than overwriting the kind is
    /// what lets `outline` and the canvas show what the alternative is.
    pub is_variant: bool,
    pub is_individual: bool,
    pub is_derived: bool,
    pub is_constant: bool,
    pub is_readonly: bool,
    /// `var` — a variable feature.
    pub is_variable: bool,
    pub is_end: bool,
    pub is_ref: bool,
    /// `render r` — the member is a *view rendering*, not an ordinary member.
    ///
    /// `render` is a `MemberPrefix` that introduces a `ViewRenderingMembership`
    /// (`SysML.xtext:2335-2343`), while `rendering r` is a `RenderingUsage` declared like any
    /// other usage member. The two were folded into one keyword, so a view body holding one
    /// `render` and one `rendering` counted as two renderings when it states one.
    pub is_render: bool,
    /// `member feature` uses an OwningMembership rather than a FeatureMembership.
    pub is_member: bool,
    pub is_sufficient: bool,
    /// `standard library package` — a library package the user marked as standard.
    ///
    /// The reference warns about it: a user's own library is not the standard one, and marking it
    /// so makes `import` behave as if it were (`LibraryPackage_invalid_notStandard.kerml.xt:15`).
    pub is_standard: bool,
    /// The visibility keyword written before the member, when one was.
    ///
    /// It belongs to the membership, but nothing models memberships as elements here, so the
    /// declaration carries it: `private attribute b;` inside `item i` is not nameable from
    /// outside `i` (`AssignmentActionUsage_invalid.sysml.xt:41`).
    pub visibility: Option<Visibility>,
    /// `parallel` — a state's substates run concurrently.
    pub is_parallel: bool,
    /// `ordered` — the feature's values have an order.
    pub is_ordered: bool,
    /// `nonunique` — the feature's values may repeat.
    pub is_nonunique: bool,

    /// `composite` / `portion` / `snapshot` / `timeslice` — occurrence portion kinds.
    pub portion: Option<String>,
    /// `#Tag` extension keywords, in authored order, without the `#`.
    pub extensions: Vec<String>,
}

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

/// The declaration half of a definition or usage.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Head {
    pub name: Name,
    pub prefixes: Prefixes,
    pub multiplicity: Option<Multiplicity>,
    /// `end connectingHub [0..1] part hub` — the name written for the owned cross feature.
    ///
    /// Distinct from `name`, which is the end's: `end cross part hub : Hub` declares a cross
    /// feature called `cross` and an end called `hub`.
    pub cross_name: Option<String>,
    /// `end [0..1] part hub` — the bound written between the end marker and the kind keyword.
    ///
    /// It is not the end's multiplicity: the specification fixes an end's own multiplicity at 1..1
    /// and puts this bound on the owned cross feature. Kept separate so the two never get confused
    /// by a rule that asks how many values the end itself holds.
    pub cross_multiplicity: Option<Multiplicity>,
    /// `end e subsets s feature f` — the specializations of the owned cross feature, kept apart
    /// from `relations`, which are the end's own.
    pub cross_relations: Vec<Relation>,
    /// Ordered relationship clauses: typing, then subsetting/redefinition as written.
    pub relations: Vec<Relation>,
    /// `connector a : A from x to y` — the end the declaration starts from.
    ///
    /// A flow and a connector write their ends after the head rather than after a `connect`
    /// keyword. Left to the body they became four members — `from`, `x`, `to`, `y` — and the two
    /// ends then counted as declared twice in the enclosing namespace
    /// (`ConnectorTest_NestedConnectorConnectingEndsGoodCase.kerml.xt:34`).
    pub from: Option<String>,
    /// The end the declaration goes to. `flow a.b to c.d` writes only this one.
    pub to: Option<String>,
    /// The ends after the second of an n-ary connector, `connection c connect (a, b, c)`. Empty for
    /// every binary form. `from` and `to` hold the first two of the list.
    pub extra_ends: Vec<String>,
    /// The value clause (`= expr`), when one was written.
    pub value: Option<Expr>,
    /// Authored text of the value clause, operator included.
    pub value_text: Option<String>,
    /// The range covering `= <value>`, operator included.
    ///
    /// An editor replaces this range when a value changes, so it has to start at the `=` and end
    /// at the last character of the value. Handing back only the value's own range left the `=`
    /// behind and produced `= = 950`.
    pub value_span: Option<Span>,
    /// `x := 1` — the value is an initial value, not a default one. Only a variable may carry it.
    pub is_initial: bool,
    /// `default = 1` / `default 1` — a *default* `FeatureValue`.
    ///
    /// The distinction is not cosmetic: `validateFeatureValueOverriding` lets a redefinition rebind a
    /// redefined feature's value only while that value is a default one
    /// (`featureWithValue.redefinition.redefinedFeature->closure(...).valuation->forAll(isDefault)`).
    /// Before this field existed the parser read the word `default` and used it only to decide whether
    /// an expression followed, so `= 42` and `default = 42` lowered identically and the rule had nothing
    /// to test. `modifiers.is_constant`-style care applies here too: this records only what was written.
    pub is_default: bool,
    pub span: Span,
}

impl Head {
    /// The first target of the first relation of the given operator.
    pub fn target_of(&self, op: RelationOp) -> Option<&TypeRef> {
        self.relations
            .iter()
            .find(|r| r.op == op)
            .and_then(|r| r.targets.first())
    }

    /// Every target of every clause of the given operator, in authored order.
    pub fn targets_of(&self, op: RelationOp) -> Vec<&TypeRef> {
        self.relations
            .iter()
            .filter(|r| r.op == op)
            .flat_map(|r| r.targets.iter())
            .collect()
    }
}

/// An expression, as written.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// An integer literal, spelling preserved (`1_000`).
    Int(String, Span),
    /// A real literal, spelling preserved (`3.6e6`).
    Real(String, Span),
    /// A string literal, quotes stripped.
    Str(String, Span),
    Bool(bool, Span),
    /// `null`.
    Null(Span),
    /// A name or dotted path: `d.endurance`, `ISQ::mass`.
    Ref(TypeRef),
    /// `op lhs rhs`.
    Binary {
        op: String,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    /// `op operand`.
    Unary {
        op: String,
        operand: Box<Expr>,
        span: Span,
    },
    /// `callee(args)`.
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        /// The name each argument was written under, parallel to `args`:
        /// `f(mass = 1)` states that the argument redefines `mass`.
        arg_names: Vec<Option<String>>,
        span: Span,
    },
    /// `if c ? a else b`, and the `if c then a else b` spelling.
    Conditional {
        test: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        span: Span,
    },
    /// `base [ unit ]` — a unit bracket on a literal or expression.
    Unit {
        value: Box<Expr>,
        unit: TypeRef,
        span: Span,
    },
    /// `base.member` — a member access that is not a plain path.
    Member {
        base: Box<Expr>,
        member: String,
        span: Span,
    },
    /// `a, b, c` — a comma sequence, used by tuple arguments and collection literals.
    Sequence {
        items: Vec<Expr>,
        span: Span,
    },
    /// `x istype T` / `x hastype T` / `x as T`.
    TypeTest {
        kind: TypeTestKind,
        operand: Box<Expr>,
        type_name: TypeRef,
        span: Span,
    },
    /// `base->op(args)` — a sequence/collection operation.
    ChainOp {
        base: Box<Expr>,
        op: String,
        args: Vec<Expr>,
        /// Names the braced form declares: `->forAll { in w; ... }` binds `w`.
        params: Vec<String>,
        span: Span,
    },
    /// `new T(a, b)` — an instance is created, not referenced. Type and arguments are both
    /// kept: reading the type only and dropping the arguments made every constructor
    /// indistinguishable from a bare type reference.
    Constructor {
        type_ref: TypeRef,
        args: Vec<Expr>,
        /// The name each argument was written under, parallel to `args`.
        arg_names: Vec<Option<String>>,
        span: Span,
    },
    /// `a ? b : c` in ternary position could not be told from the conditional; kept for
    /// completeness when the parser sees a bare `?` it cannot classify.
    Opaque {
        text: String,
        children: Vec<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, s)
            | Expr::Real(_, s)
            | Expr::Str(_, s)
            | Expr::Bool(_, s)
            | Expr::Null(s) => *s,
            Expr::Ref(r) => r.span,
            Expr::Binary { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Call { span, .. }
            | Expr::Conditional { span, .. }
            | Expr::Unit { span, .. }
            | Expr::Member { span, .. }
            | Expr::Sequence { span, .. }
            | Expr::TypeTest { span, .. }
            | Expr::ChainOp { span, .. }
            | Expr::Constructor { span, .. }
            | Expr::Opaque { span, .. } => *span,
        }
    }

    /// Replace the whole-expression range.
    ///
    /// Needed when a construct spans more than its parts: a parenthesised expression is the inner
    /// node inside its own parentheses, and a caller holding only the inner node reports a range
    /// that stops before the closing paren. A value clause spliced into that range leaves the
    /// paren behind.
    pub fn set_span(&mut self, span: Span) {
        match self {
            Expr::Int(_, s)
            | Expr::Real(_, s)
            | Expr::Str(_, s)
            | Expr::Bool(_, s)
            | Expr::Null(s) => *s = span,
            Expr::Ref(r) => r.span = span,
            Expr::Binary { span: s, .. }
            | Expr::Unary { span: s, .. }
            | Expr::Call { span: s, .. }
            | Expr::Conditional { span: s, .. }
            | Expr::Unit { span: s, .. }
            | Expr::Member { span: s, .. }
            | Expr::Sequence { span: s, .. }
            | Expr::TypeTest { span: s, .. }
            | Expr::ChainOp { span: s, .. }
            | Expr::Constructor { span: s, .. }
            | Expr::Opaque { span: s, .. } => *s = span,
        }
    }

    /// Every name reference in the tree, in source order.
    pub fn refs(&self) -> Vec<&TypeRef> {
        let mut out = Vec::new();
        self.collect_refs(&mut out);
        out
    }

    fn collect_refs<'a>(&'a self, out: &mut Vec<&'a TypeRef>) {
        match self {
            Expr::Ref(r) => out.push(r),
            Expr::Binary { lhs, rhs, .. } => {
                lhs.collect_refs(out);
                rhs.collect_refs(out);
            }
            Expr::Unary { operand, .. } => operand.collect_refs(out),
            Expr::Call { callee, args, .. } => {
                callee.collect_refs(out);
                for a in args {
                    a.collect_refs(out);
                }
            }
            Expr::Constructor { type_ref, args, .. } => {
                out.push(type_ref);
                for a in args {
                    a.collect_refs(out);
                }
            }
            Expr::Conditional {
                test,
                then_expr,
                else_expr,
                ..
            } => {
                test.collect_refs(out);
                then_expr.collect_refs(out);
                else_expr.collect_refs(out);
            }
            // A unit name is not a model element to be resolved; whether the unit is known is
            // the units layer's question.
            Expr::Unit { value, .. } => value.collect_refs(out),
            Expr::Member { base, .. } => base.collect_refs(out),
            Expr::Sequence { items, .. } => {
                for i in items {
                    i.collect_refs(out);
                }
            }
            Expr::TypeTest {
                operand, type_name, ..
            } => {
                operand.collect_refs(out);
                out.push(type_name);
            }
            Expr::ChainOp { base, args, .. } => {
                base.collect_refs(out);
                for a in args {
                    a.collect_refs(out);
                }
            }
            Expr::Opaque { children, .. } => {
                for c in children {
                    c.collect_refs(out);
                }
            }
            Expr::Int(..) | Expr::Real(..) | Expr::Str(..) | Expr::Bool(..) | Expr::Null(..) => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeTestKind {
    Istype,
    Hastype,
    As,
    /// `x meta T` — the metadata of `x`, read as metaclass `T`.
    Meta,
    /// `x @@ T` — whether the metadata of `x` is classified by `T`.
    MetaTest,
}

/// A documentation or annotation member.
#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    /// `doc /* ... */` — prose attached to the owning element.
    Doc { text: String, span: Span },
    /// `comment` / `rep` — prose. A named comment with an `about` clause is an element of its own
    /// (see `lower.rs`); the rest is not part of the element's meaning.
    Prose {
        keyword: String,
        /// The name the comment was given, if any.
        name: Option<Name>,
        /// The `about` targets, in authored order.
        about: Vec<TypeRef>,
        text: String,
        span: Span,
    },
    /// `about a, b` on a metadata usage written with the `metadata` keyword (`SysML.xtext:141-148`).
    /// It is the first member of that usage's body, and lowering reads its targets.
    About { targets: Vec<TypeRef>, span: Span },
    /// `@Safety { level = 3 }` / `#Safety` / `metadata Safety { ... }` — structured annotation.
    Metadata {
        type_ref: TypeRef,
        /// `about a, b` targets.
        about: Vec<TypeRef>,
        /// The body: each member is a redefinition of one of the type's features.
        args: Vec<BodyMember>,
        /// Whether the annotation ends at its own `;` instead of preceding a declaration.
        ///
        /// `@A;` and `@Safety { level = 3; } part def Brake;` are different statements. The first is
        /// a member of the body on its own, and SysML §7.27.2 (PDF p.192, "the annotated element
        /// shall be implicitly the containing namespace") makes the enclosing element the annotated
        /// one. The second is a `MemberPrefix` of the declaration that follows
        /// (`KerML.xtext:384`, `OwnedFeatureMember : MemberPrefix FeatureElement`), so it annotates
        /// that declaration. Position alone cannot tell them apart — both are an annotation member
        /// followed by a declaration — so the terminator is recorded here.
        self_terminated: bool,
        span: Span,
    },
}

/// An element in a body.
#[derive(Debug, Clone, PartialEq)]
pub enum BodyMember {
    /// A documented relationship: `import`, `alias`, `expose`, `dependency`, `satisfy`.
    Relationship(Relationship),
    /// A definition or usage.
    Declaration(Box<Declaration>),
    /// A statement that is not a declaration: `connect`, `bind`, `assign`, `succession`, control
    /// flow, `return`, a bare expression.
    Statement(Statement),
    /// `doc` / `comment` / `@Tag`.
    Annotation(Annotation),
    /// `filter expr;`.
    Filter { expr: Expr, span: Span },
    /// Text the grammar rejected. Carried in the tree so a report can point at it rather than
    /// pretend the body ended here.
    Error {
        message: String,
        text: String,
        span: Span,
    },
}

impl BodyMember {
    pub fn span(&self) -> Span {
        match self {
            BodyMember::Relationship(r) => r.span(),
            BodyMember::Declaration(d) => d.span(),
            BodyMember::Statement(s) => s.span(),
            BodyMember::Annotation(a) => match a {
                Annotation::Doc { span, .. }
                | Annotation::Prose { span, .. }
                | Annotation::About { span, .. }
                | Annotation::Metadata { span, .. } => *span,
            },
            BodyMember::Filter { span, .. } | BodyMember::Error { span, .. } => *span,
        }
    }
}

/// A documented relationship statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Relationship {
    Import {
        visibility: Option<Visibility>,
        /// The target path without its suffix: `ISQ`, `ScalarValues::Real`.
        target: String,
        /// `::*`, `::**`, `::*::**`, `::[filter]`, or empty for a membership import.
        shape: ImportShape,
        span: Span,
    },
    /// `alias Hub for Wheel;`
    Alias {
        name: Name,
        target: TypeRef,
        span: Span,
    },
    /// `expose X::*;` inside a view.
    Expose {
        target: String,
        recursive: bool,
        span: Span,
    },
    /// `satisfy R by x;`
    Satisfy {
        /// The requirement the statement points at.
        requirement: TypeRef,
        /// The element claimed to satisfy it, when a `by` clause was written.
        by: Option<TypeRef>,
        span: Span,
    },
    /// `dependency a from b;`
    Dependency {
        name: Name,
        clients: Vec<TypeRef>,
        suppliers: Vec<TypeRef>,
        span: Span,
    },
    /// `subtype C specializes B;` — a `Specialization` stated on its own rather than as the `:>`
    /// clause of a declaration (`KerML.xtext:388-398`).
    ///
    /// Both sides name types that already exist, so nothing is declared here.
    Specialization {
        /// The `specific` end: the type that specializes.
        specific: TypeRef,
        /// The `general` end: the type being specialized.
        general: TypeRef,
        span: Span,
    },
    /// `disjoint a.b from c.d;` — a `Disjoining` stated on its own (KerML 1.0 §8.2.3.2).
    Disjoining {
        disjoined: TypeRef,
        disjoining: TypeRef,
        span: Span,
    },
    /// `featuring of M by X;` — a `TypeFeaturing` stated on its own (`KerML.xtext:651-655`). The
    /// feature the statement is about is `featured` (`of M`), and the feature that features it is
    /// `featuring` (`by X`).
    Featuring {
        name: Option<Name>,
        featured: TypeRef,
        featuring: TypeRef,
        span: Span,
    },
}

impl Relationship {
    pub fn span(&self) -> Span {
        match self {
            Relationship::Import { span, .. }
            | Relationship::Alias { span, .. }
            | Relationship::Expose { span, .. }
            | Relationship::Satisfy { span, .. }
            | Relationship::Dependency { span, .. }
            | Relationship::Specialization { span, .. }
            | Relationship::Disjoining { span, .. }
            | Relationship::Featuring { span, .. } => *span,
        }
    }
}

/// Which form of import was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportShape {
    /// `import A::B` — one membership.
    Membership { recursive: bool },
    /// `import A::*` — every membership of a namespace.
    Namespace { recursive: bool },
    /// `import A::[filter]` — a filtered subset.
    Filter,
}

impl ImportShape {
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

/// A statement inside a body.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// `connect a.p to b.q;`
    Connect {
        name: Name,
        /// The connector's own type, when written (`connection c : T connect ...`).
        type_ref: Option<TypeRef>,
        from: String,
        to: String,
        /// The ends after the second of an n-ary connector, `connect (a, b, c)`. Empty for the
        /// binary form.
        extra: Vec<String>,
        span: Span,
    },
    /// `bind a = b;`
    Bind {
        lhs: Expr,
        rhs: Expr,
        /// Ends written after the second (`bind a = b = c`). A binding is binary, so these are
        /// reported rather than read as more ends.
        extra: Vec<Expr>,
        span: Span,
    },
    /// `assign lhs := rhs;`
    Assign { lhs: Expr, rhs: Expr, span: Span },
    /// `succession first a then b;` / `a then b;`
    Succession {
        name: Name,
        first: Option<Expr>,
        then: Option<Expr>,
        /// `accept <payload>` / `accept at <time>` — what starts the transition.
        trigger: Option<Expr>,
        /// Which word introduced the trigger: `accept`, `at`, `when` or `after`. The keyword
        /// decides what the argument has to satisfy, so it is kept rather than dropped.
        trigger_kind: Option<String>,
        /// `accept p : T` — the type of the accepted payload.
        trigger_type: Option<TypeRef>,
        /// `accept p via port` — the port the payload arrives through.
        trigger_via: Option<Expr>,
        /// `if <condition>`.
        guard: Option<Expr>,
        /// `do <action>` — what the transition does.
        effect: Option<Expr>,
        /// The source is not written: `then b;` and the `then` before a declaration start at the
        /// feature that precedes them in the body (`UsageUtil.getPreviousFeature` in the reference
        /// implementation). With `then` also `None`, the target is the feature that follows.
        implicit_source: bool,
        span: Span,
    },
    /// `first a;`
    First { target: Option<Expr>, span: Span },
    /// A control-flow construct with a condition and one or more bodies.
    ControlFlow {
        keyword: String,
        condition: Option<Expr>,
        /// `then`/`else`/loop bodies, in authored order, each labelled by its keyword.
        branches: Vec<(String, Vec<BodyMember>)>,
        span: Span,
    },
    /// `merge m { ... }` and the other control nodes.
    ControlNode {
        keyword: String,
        name: Name,
        body: Vec<BodyMember>,
        span: Span,
    },
    /// `for x in y { ... }`
    For {
        variable: Name,
        source: Expr,
        body: Vec<BodyMember>,
        span: Span,
    },
    /// `until cond;` / `until condition;` on a loop.
    Until { condition: Expr, span: Span },
    /// `terminate;` / `terminate x;`
    Terminate { target: Option<Expr>, span: Span },
    /// `inverse A::f of B::g;` / `inverting X inverse A::f of B::g;` — a feature inverting stated
    /// on its own (`KerML.xtext:632-641`).
    Inverting {
        inverted: Option<TypeRef>,
        inverting: Option<TypeRef>,
        span: Span,
    },
    /// `return expr;`
    Return { expr: Option<Expr>, span: Span },
    /// `return : T = expr;` — a declaration of the behaviour's result parameter.
    ReturnDeclaration { head: Head, span: Span },
    /// `accept x : T;` / `send x : T;` — message handlers.
    Message {
        keyword: String,
        name: Name,
        type_ref: Option<TypeRef>,
        /// The feature the message is addressed to (`send x to ground`, `accept y via radio`).
        ///
        /// Consumed and dropped before: a send that does not say where it goes cannot be drawn on
        /// a lifeline, and no rule could ask whether the receiver exists.
        addressed: Option<Expr>,
        /// Whether a payload was written. `send via p;`, `send to q;` and `send;` write none; the
        /// grammar supplies an empty parameter for it when `via` or `to` follows
        /// (`SysML.xtext:1499-1506`).
        has_payload: bool,
        /// The `via` end as written: a send's sender, an accept's receiver.
        via: Option<Expr>,
        /// The `to` end as written: a send's receiver.
        to: Option<Expr>,
        /// A send's own body (`send x via p { in extra; }`). Its directed features are input
        /// parameters of the send.
        body: Vec<BodyMember>,
        span: Span,
    },
    /// `perform x : T;` / `exhibit s : T;`
    Behavior {
        keyword: String,
        /// Whether the statement writes the kind after the keyword: `perform action x : T` declares
        /// a name, `perform b;` references `b`.
        ///
        /// `PerformActionUsageDeclaration` and `ExhibitStateUsage` are
        /// `OwnedReferenceSubsetting FeatureSpecializationPart? | <keyword> UsageDeclaration?`
        /// (`SysML.xtext:1411-1425,1840-1848`), so the first alternative declares nothing.
        declares: bool,
        name: Name,
        /// The dotted continuation of the name, when the statement refines a feature of an
        /// existing behaviour: `perform vehicleMassTest.collectData { ... }` names the
        /// performance `vehicleMassTest` and the feature `collectData` inside it.
        path: Vec<String>,
        type_ref: Option<TypeRef>,
        /// `perform action :>> doXorY = doX;` binds the performance to a value.
        value: Option<Expr>,
        value_span: Option<Span>,
        body: Vec<BodyMember>,
        span: Span,
    },
    /// `require constraint { ... }` / `assume constraint { ... }` inside a requirement.
    Constraint {
        /// True for `assume`, false for `require`.
        is_assume: bool,
        /// The declared name, when the `constraint` keyword form names one.
        name: Name,
        /// The type written after `:` — `require constraint c1 : C;`.
        ///
        /// The type is a `ConstraintDefinition` the constraint is an instance of. Dropping it
        /// left the element untyped, so the reference's "must be typed by a constraint
        /// definition" check had nothing to judge and the outline showed `constraint c1`.
        type_ref: Option<TypeRef>,
        /// The constraint's expression(s). Several conjoin.
        exprs: Vec<Expr>,
        span: Span,
    },
    /// `assert (not)? constraint { ... }`
    Assert {
        is_negated: bool,
        name: Name,
        type_ref: Option<TypeRef>,
        /// Every type the clause names: `assert constraint x : A, B;` states two, and a rule
        /// asking whether an assertion names exactly one constraint definition needs both.
        type_refs: Vec<TypeRef>,
        exprs: Vec<Expr>,
        /// `assert path;` — a reference to an existing constraint rather than a body.
        target: Option<TypeRef>,
        span: Span,
    },
    /// `objective { ... }` / `objective name : T { ... }` — a requirement's objective.
    Objective {
        name: Name,
        type_ref: Option<TypeRef>,
        body: Vec<BodyMember>,
        span: Span,
    },
    /// `subject x : T;`
    Subject {
        name: Name,
        type_ref: Option<TypeRef>,
        value: Option<Expr>,
        value_text: Option<String>,
        value_span: Option<Span>,
        is_initial: bool,
        is_default: bool,
        span: Span,
    },
    /// `frame x;` — a requirement's frame.
    Frame {
        name: Name,
        type_ref: Option<TypeRef>,
        span: Span,
    },
    /// `allocate a to b;`
    Allocate { from: Expr, to: Expr, span: Span },
    /// `include useCase;` / `include useCase { ... }`
    Include {
        /// The usage's own name, for `include use case i : T;`. Empty for `include T`.
        name: Name,
        target: TypeRef,
        body: Vec<BodyMember>,
        span: Span,
    },
    /// `flow [of payload] [from a.b to c.d];` or the bare `flow a.b to c.d;` form.
    ///
    /// Modelled as a statement rather than a declaration because neither end is a name the
    /// element owns: `flow 'a'.x to 'b'.y` produces an anonymous flow, and reading the first
    /// path segment as its name made the flow collide with the action it starts from.
    Flow {
        head: Head,
        /// The payload is a feature declaration of its own, not the flow's identification.
        payload: Option<Head>,
        body: Vec<BodyMember>,
        span: Span,
    },
    /// A bare expression used as a body member: a terminal expression in a calculation, or a
    /// default reference's value.
    Expression { expr: Expr, span: Span },
    /// `entry` / `do` / `exit` inside a state, with the behaviour they name.
    StateAction {
        keyword: String,
        /// Whether the statement writes the `action` word: `entry action boot;` declares the
        /// behaviour, `entry boot;` names one that exists elsewhere. A rule about references
        /// must not judge the first.
        declares: bool,
        name: Name,
        type_ref: Option<TypeRef>,
        body: Vec<BodyMember>,
        span: Span,
    },
    /// `inv ... { ... }` — a KerML invariant.
    Invariant {
        name: Name,
        exprs: Vec<Expr>,
        span: Span,
    },
    /// `event a;` / `event a.areal;` — the occurrence an event denotes, named rather than declared.
    ///
    /// `EventOccurrenceUsage` is `OccurrenceUsagePrefix 'event' ( OwnedReferenceSubsetting
    /// FeatureSpecializationPart? | OccurrenceUsageKeyword UsageDeclaration? ) UsageCompletion`
    /// (`SysML.xtext:865-870`). The first alternative writes **no declared name at all**: what
    /// follows `event` is a reference to a feature that already exists. Reading it as a
    /// declaration made the name a member of the enclosing namespace — `event a;` inside a
    /// package next to `ref a : A` declared `a` twice — and a dotted one left the rest of the
    /// path behind as a member of its own: `event publish_message.sourceEvent;` put an
    /// `attribute sourceEvent` in `17b-Sequence-Modeling.sysml`'s `producer`.
    Event {
        /// The feature the event denotes, as written (`a`, `a.areal`).
        reference: TypeRef,
        multiplicity: Option<Multiplicity>,
        /// A `:` clause after the reference (`event a : Arrival`), which types the event itself.
        type_ref: Option<TypeRef>,
        body: Vec<BodyMember>,
        span: Span,
    },
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Statement::Connect { span, .. }
            | Statement::Bind { span, .. }
            | Statement::Assign { span, .. }
            | Statement::Succession { span, .. }
            | Statement::First { span, .. }
            | Statement::ControlFlow { span, .. }
            | Statement::ControlNode { span, .. }
            | Statement::For { span, .. }
            | Statement::Until { span, .. }
            | Statement::Terminate { span, .. }
            | Statement::Inverting { span, .. }
            | Statement::Return { span, .. }
            | Statement::ReturnDeclaration { span, .. }
            | Statement::Message { span, .. }
            | Statement::Behavior { span, .. }
            | Statement::Constraint { span, .. }
            | Statement::Assert { span, .. }
            | Statement::Objective { span, .. }
            | Statement::Subject { span, .. }
            | Statement::Frame { span, .. }
            | Statement::Allocate { span, .. }
            | Statement::Include { span, .. }
            | Statement::Expression { span, .. }
            | Statement::Invariant { span, .. }
            | Statement::Flow { span, .. }
            | Statement::Event { span, .. }
            | Statement::StateAction { span, .. } => *span,
        }
    }
}

/// The kind keyword of a definition or usage.
///
/// Derives `Ord` so a report can group by keyword, and carries the authoured spelling for the
/// synonyms the two languages allow (`assoc`/`association`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Package,
    LibraryPackage,
    Namespace,
    Part,
    /// `actor driver;` — a participant of a use case or requirement.
    Actor,
    /// `stakeholder owner;` — someone with an interest in a requirement.
    Stakeholder,
    /// `verify r : R;` — a requirement verification, not a requirement declaration.
    Verification,
    Attribute,
    Port,
    Item,
    Connection,
    Interface,
    Flow,
    Allocation,
    Action,
    State,
    Calculation,
    Constraint,
    Requirement,
    Concern,
    Case,
    AnalysisCase,
    VerificationCase,
    UseCase,
    View,
    Viewpoint,
    Rendering,
    Metadata,
    Enumeration,
    Occurrence,
    /// `event arrival : Arrival;` — an `EventOccurrenceUsage`.
    Event,
    Individual,
    Dependency,
    /// A KerML classifier keyword: `class`, `datatype`, `struct`, `assoc`, `metaclass`,
    /// `behavior`, `function`, `predicate`, `interaction`, `multiplicity`, `classifier`.
    Kerml(String),
}

impl Kind {
    /// The keyword a report should print.
    pub fn keyword(&self) -> &str {
        match self {
            Kind::Package => "package",
            Kind::LibraryPackage => "library package",
            Kind::Namespace => "namespace",
            Kind::Part => "part",
            Kind::Actor => "actor",
            Kind::Stakeholder => "stakeholder",
            Kind::Verification => "verify",
            Kind::Attribute => "attribute",
            Kind::Port => "port",
            Kind::Item => "item",
            Kind::Connection => "connection",
            Kind::Interface => "interface",
            Kind::Flow => "flow",
            Kind::Allocation => "allocation",
            Kind::Action => "action",
            Kind::State => "state",
            Kind::Calculation => "calc",
            Kind::Constraint => "constraint",
            Kind::Requirement => "requirement",
            Kind::Concern => "concern",
            Kind::Case => "case",
            Kind::AnalysisCase => "analysis case",
            Kind::VerificationCase => "verification",
            Kind::UseCase => "use case",
            Kind::View => "view",
            Kind::Viewpoint => "viewpoint",
            Kind::Rendering => "rendering",
            Kind::Metadata => "metadata",
            Kind::Enumeration => "enum",
            Kind::Occurrence => "occurrence",
            Kind::Event => "event",
            Kind::Individual => "individual",
            Kind::Dependency => "dependency",
            Kind::Kerml(k) => k.as_str(),
        }
    }
}

/// A definition or usage, with its body.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub kind: Kind,
    /// True for `X def Name` and false for `X name`.
    pub is_definition: bool,
    /// True when no kind keyword was written and the declaration is a feature by default:
    /// `x : T;`, `in x : T;`. `kind` is then `Kind::Attribute`, which the lowering shares with the
    /// explicit `attribute` keyword, so this is the only place the difference survives.
    pub bare: bool,
    pub head: Head,
    pub body: Vec<BodyMember>,
    /// The element's documentation, when a `doc` member was written.
    pub doc: Option<String>,
}

impl Declaration {
    pub fn span(&self) -> Span {
        self.head.span
    }
}

/// A whole parsed file.
#[derive(Debug, Clone)]
pub struct Document {
    pub members: Vec<BodyMember>,
}

impl Document {
    /// Every error member in the tree, in source order.
    ///
    /// A parse can succeed while carrying errors: a body member the grammar rejected becomes an
    /// `Error` member rather than aborting the file, so one bad statement does not hide the other
    /// nine hundred. The caller decides whether that is fatal; the library loader treats it as
    /// "this file is unreadable", a model check reports it and carries on.
    pub fn errors(&self) -> Vec<(Span, String)> {
        let mut out = Vec::new();
        collect_errors(&self.members, &mut out);
        out.sort_by_key(|(span, _)| span.start);
        out
    }

    pub fn has_errors(&self) -> bool {
        !self.errors().is_empty()
    }
}

fn collect_errors(members: &[BodyMember], out: &mut Vec<(Span, String)>) {
    for m in members {
        match m {
            BodyMember::Error { message, span, .. } => out.push((*span, message.clone())),
            BodyMember::Declaration(d) => collect_errors(&d.body, out),
            BodyMember::Statement(s) => match s {
                Statement::ControlFlow { branches, .. } => {
                    for (_, b) in branches {
                        collect_errors(b, out);
                    }
                }
                Statement::ControlNode { body, .. } | Statement::Objective { body, .. } => {
                    collect_errors(body, out)
                }
                Statement::For { body, .. } | Statement::Event { body, .. } => {
                    collect_errors(body, out)
                }
                _ => {}
            },
            BodyMember::Annotation(Annotation::Metadata { args, .. }) => collect_errors(args, out),
            BodyMember::Relationship(_) | BodyMember::Annotation(_) | BodyMember::Filter { .. } => {
            }
        }
    }
}
