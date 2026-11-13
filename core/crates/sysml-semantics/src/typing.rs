//! Value type conformance: the value a feature is initialised with must be of its type.
//!
//! This is the largest gap between "the model resolves" and "the model is well formed". Until now
//! `attribute b : Real = "text"` passed every gate: the resolver saw two resolvable names, and the
//! evaluator does not evaluate strings, so nothing ever compared them.
//!
//! What this decides, and what it deliberately does not:
//!
//!   * A literal against a scalar type from the Kernel Data Type Library (`Real`, `Integer`,
//!     `String`, `Boolean`, and their specializations) is decided from the literal's own kind.
//!   * A value whose type can be named against a declared scalar type, and the other way round:
//!     `attribute mass : Real = wheels`, where `wheels` is a part, and `part p : Wheel = 5` both
//!     put a value of one family where a type of the other belongs. These used to be silent, and
//!     they are the class of error a reader means by "the tool does not type-check".
//!   * Two structural types against each other: `part spare : Wheel = car` with `car : Car` is an
//!     error, and a value whose type specializes the declared one is not. See [`Family`] for the
//!     one direction that is left alone on purpose.
//!   * A bound written on the type (`Real[1]`) is the feature's multiplicity, not part of the type.
//!   * Anything else — a value whose type cannot be named, a declared type that does not resolve —
//!     is left undecided. Guessing here would reject models that are correct, which is worse than
//!     saying nothing: a wrong diagnostic costs more than a missing one.
//!
//! The scalar hierarchy is read from the model's own library rather than hard-coded, so a project
//! that imports a different scalar library is judged by its own types.

use sysml_model::{BinaryOp, Element, Expr, Kind, UnaryOp};

use sysml_resolve::Index;

use crate::expr::{self, ExprField, ExprId, ExprKind, ExprTree};

/// The literal kinds a value expression can have, as far as this layer can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    Integer,
    Real,
    String,
    Boolean,
}

impl LiteralKind {
    fn of(expr: &Expr) -> Option<Self> {
        match expr {
            Expr::Int(_) => Some(LiteralKind::Integer),
            Expr::Real(_) => Some(LiteralKind::Real),
            Expr::Str(_) => Some(LiteralKind::String),
            Expr::Bool(_) => Some(LiteralKind::Boolean),
            Expr::Unary { operand, .. } => LiteralKind::of(operand),
            _ => None,
        }
    }

    /// The kind a type's leaf name stands for, when it is one of the scalars this check knows.
    fn of_leaf(leaf: &str) -> Option<Self> {
        match leaf {
            "Integer" | "Natural" | "Positive" => Some(LiteralKind::Integer),
            "Real" | "Rational" => Some(LiteralKind::Real),
            "String" => Some(LiteralKind::String),
            "Boolean" => Some(LiteralKind::Boolean),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            LiteralKind::Integer => "an integer",
            LiteralKind::Real => "a real",
            LiteralKind::String => "a string",
            LiteralKind::Boolean => "a boolean",
        }
    }
}

/// The library type a scalar literal of this kind conforms to, by simple name.
///
/// `Integer` and `Natural` are themselves `Real`s, so an integer literal conforms to both; the
/// check accepts the literal when the declared type is at or above any of these in the hierarchy.
fn candidate_types(kind: LiteralKind) -> &'static [&'static str] {
    match kind {
        LiteralKind::Integer => &[
            "Integer",
            "Rational",
            "Real",
            "Complex",
            "Number",
            "NumericalValue",
            "ScalarValue",
        ],
        LiteralKind::Real => &["Real", "Complex", "Number", "NumericalValue", "ScalarValue"],
        LiteralKind::String => &["String", "ScalarValue"],
        LiteralKind::Boolean => &["Boolean", "ScalarValue"],
    }
}

/// Whether `declared` resolves to a scalar type that accepts a literal of `kind`.
fn accepts(index: &Index, declared: usize, kind: LiteralKind) -> bool {
    let name = &index.qualified[declared];
    let leaf = name.rsplit("::").next().unwrap_or(name);
    candidate_types(kind).contains(&leaf)
}

/// Every feature whose value contradicts its declared type.
///
/// Three questions are asked, in this order, and at most one of them reports on an element:
///
///   * **A scalar value against a scalar type.** The value's kind comes from the literal when it is
///     one, and from the expression's type otherwise: `attribute b : Boolean = 1 + 2` writes no
///     literal against a Boolean, and the sum is an `Integer` only because its operands are.
///     Reading the type is what lets the same check cover both, and it is why a value whose type
///     cannot be named is skipped rather than guessed.
///   * **A value from the other family.** `attribute mass : Real = wheels` initialises a number
///     with a part, and `part p : Wheel = 5` initialises a part with a number. Neither side is a
///     literal of the wrong kind — the first is a reference and the second is a literal against a
///     type this layer had no opinion about — so both used to pass silently. That silence is the
///     gap a reader means by "there is no type checking".
///   * **Two structural types against each other.** `part spare : Wheel = car` with `car : Car`
///     is an error. Narrowing stays legal, and one direction is left alone on purpose — see
///     [`Family`].
///
/// The family is decided by the loaded library and the compiled-in metaclass table: a type is
/// scalar when it specializes `ScalarValues::ScalarValue` (which covers the quantity definitions
/// the Systems Library writes as attribute definitions over `Real`), and structural when it is an
/// occurrence definition. Without a library the scalar side has no root to compare against, and
/// the check then says nothing rather than guessing.
pub fn scalar_value_issues(
    index: &Index,
    roots: &[Element],
    tree: &ExprTree,
) -> Vec<(String, u32, String)> {
    let mut out = Vec::new();
    // The root of the scalar family, looked up once. `None` means the project loaded no library
    // that declares one, and every family question below then has no answer.
    let scalar_root = index.library_element("ScalarValues::ScalarValue");
    fn walk(
        el: &Element,
        index: &Index,
        tree: &ExprTree,
        scalar_root: Option<usize>,
        out: &mut Vec<(String, u32, String)>,
    ) {
        if let (Some(expr), Some(type_text)) = (&el.value_expr, &el.type_ref) {
            let head = type_text.split(',').next().unwrap_or(type_text).trim();
            if let Some(declared) = index.resolve(el.id, head).found() {
                check_value(index, tree, el, expr, declared, scalar_root, out);
            }
        }
        for child in &el.children {
            walk(child, index, tree, scalar_root, out);
        }
    }
    for root in roots {
        walk(root, index, tree, scalar_root, &mut out);
    }
    out
}

fn check_value(
    index: &Index,
    tree: &ExprTree,
    el: &Element,
    expr: &Expr,
    declared: usize,
    scalar_root: Option<usize>,
    out: &mut Vec<(String, u32, String)>,
) {
    let declared_family = family(index, scalar_root, declared);
    if let Some(kind) = LiteralKind::of(expr).or_else(|| value_kind(index, tree, el)) {
        if is_scalar(index, declared) && !accepts(index, declared, kind) {
            out.push((
                el.name.clone(),
                el.line,
                format!(
                    "`{}` is declared as `{}` but initialised with {}",
                    el.name,
                    index.qualified[declared],
                    kind.as_str()
                ),
            ));
        } else if declared_family == Family::Structural {
            // `part p : Wheel = 5`. The literal names a scalar and the declared type is an
            // occurrence, so the value cannot be an instance of it.
            out.push((
                el.name.clone(),
                el.line,
                format!(
                    "`{}` is declared as `{}`, which is not a scalar, but initialised with {}",
                    el.name,
                    index.qualified[declared],
                    kind.as_str()
                ),
            ));
        }
        return;
    }
    // Not a scalar value: the expression names a reference, a call or a member read. Its type is
    // the only thing that can be compared.
    let Some(root) = tree.root_of(el.id, ExprField::Value) else {
        return;
    };
    let Some(value_type) = type_of(index, tree, root) else {
        return;
    };
    let value_family = family(index, scalar_root, value_type);
    match (declared_family, value_family) {
        (Family::Scalar, Family::Structural) => out.push((
            el.name.clone(),
            el.line,
            format!(
                "`{}` is declared as `{}`, a scalar type, but initialised with a `{}`, a {}",
                el.name,
                index.qualified[declared],
                index.qualified[value_type],
                index.kind[value_type].keyword()
            ),
        )),
        // An occurrence initialised with a scalar: `part p : Wheel = mass`.
        (Family::Structural, Family::Scalar) => out.push((
            el.name.clone(),
            el.line,
            format!(
                "`{}` is declared as `{}`, a {}, but initialised with a `{}`, a scalar type",
                el.name,
                index.qualified[declared],
                index.kind[declared].keyword(),
                index.qualified[value_type]
            ),
        )),
        // Two occurrences: `part spare : Wheel = car` with `car : Car`. Only a pair with no
        // relation either way is reported. The direction where the declared type specializes the
        // value's type is left alone on purpose: the official corpus writes it and states the
        // model clean — `out item assembledVehicle : AssembledVehicle = 'vehicle assy without
        // engine' { part engine : Engine = …; }`, where `AssembledVehicle :> VehicleAssembly`.
        // The `= ref { … }` form says "take this value and add these members", and the body is
        // what makes up the difference; reporting it would be a second answer to a question the
        // reference implementation does not ask.
        (Family::Structural, Family::Structural) => {
            let value_type = core_type(index, value_type);
            let declared = core_type(index, declared);
            if !conforms(index, value_type, declared) && !conforms(index, declared, value_type) {
                out.push((
                    el.name.clone(),
                    el.line,
                    format!(
                        "`{}` is declared as `{}` but initialised with a `{}`, which is neither a \
                         `{}` nor a subtype of it",
                        el.name,
                        index.qualified[declared],
                        index.qualified[value_type],
                        index.qualified[declared]
                    ),
                ));
            }
        }
        _ => {}
    }
}

/// Whether a type holds a number or an occurrence, as far as this layer can tell.
///
/// Three answers rather than two, because the third is the common case and guessing it would be
/// the wrong kind of confidence. A project's own `attribute def Real` is neither the library's
/// scalar nor something a part is made of, and a type above either family (`Base::DataValue` sits
/// above `ScalarValue`, `Base::Anything` above everything) says nothing about what conforms to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    /// The type specializes the library's `ScalarValue`.
    Scalar,
    /// The type is an occurrence definition — something a part, an item or a state is made of.
    Structural,
    /// Neither, or unknown. The check has no opinion.
    Unknown,
}

/// Whether the type at `id` is (or specializes) the library's `ScalarValue`.
///
/// `None` when its family cannot be told: no library is loaded, so the root is unknown, or `id`
/// sits above `ScalarValue` in the hierarchy (`Base::DataValue` is a supertype of it, and both a
/// scalar and a structure specialize it). A supertype of the scalar family is not a member of it,
/// and calling one a structure would reject `attribute x : DataValue = 1`.
fn scalar_family(index: &Index, scalar_root: Option<usize>, id: usize) -> Option<bool> {
    let root = scalar_root?;
    let id = core_type(index, id);
    // The root, or a type above it. `Base::DataValue` is one: both a scalar and a structure
    // specialize it, so a value of that type says nothing about which family it is in.
    if index.is_subtype(root, id) {
        return (id == root).then_some(true);
    }
    Some(index.is_subtype(id, root))
}

/// The family a type belongs to, by the library's scalar root and the metaclass table.
///
/// The structural side is read from the compiled-in metaclass hierarchy rather than from a library
/// package, so it answers the same on a checkout whose library is the bundled stub, which declares
/// no `Occurrences`. `attribute def` is deliberately *not* structural: it is the data layer, where
/// this layer has no hierarchy to compare against, and `attribute def RealDef; attribute b :
/// RealDef = "text"` is a model a project may write for reasons of its own.
fn family(index: &Index, scalar_root: Option<usize>, id: usize) -> Family {
    if scalar_family(index, scalar_root, id) == Some(true) {
        return Family::Scalar;
    }
    let id = core_type(index, id);
    if index.kind[id].is_metaclass("OccurrenceDefinition") {
        return Family::Structural;
    }
    Family::Unknown
}

/// The type behind a name: an alias is followed to its target, and a feature to the one type it
/// declares.
///
/// `attribute x : Real` names `Real` through `x`, and a value that reads `x` has the type `x`
/// declares. Without this step `part p : Wheel = w2` compared the definition `Wheel` against the
/// *usage* `w2` and answered that neither is a scalar, which happens to be right; but
/// `attribute x : y = 5` with `attribute y : Real` compared against the usage `y` and would have
/// called the declared type structural.
fn core_type(index: &Index, mut id: usize) -> usize {
    for _ in 0..8 {
        if index.kind[id] == Kind::Alias {
            let Some(text) = index.type_of(id) else {
                return id;
            };
            match index.resolve(id, text.split(',').next().unwrap_or(text).trim()) {
                sysml_resolve::Resolution::Found(next) if next != id => id = next,
                _ => return id,
            }
        } else if index.kind[id].is_feature() {
            match declared_type(index, id) {
                Some(next) if next != id => id = next,
                _ => return id,
            }
        } else {
            return id;
        }
    }
    id
}

/// The scalar kind of a value expression that is not a literal, read from its type.
fn value_kind(index: &Index, tree: &ExprTree, el: &Element) -> Option<LiteralKind> {
    let root = tree.root_of(el.id, ExprField::Value)?;
    let type_id = type_of(index, tree, root)?;
    LiteralKind::of_leaf(leaf_of(index, type_id))
}

/// Whether a resolved type is one of the library's scalar types, i.e. a type this check has an
/// opinion about.
fn is_scalar(index: &Index, id: usize) -> bool {
    let name = &index.qualified[id];
    // A user definition that happens to be called `Real` is not the library's `Real`.
    if !name.starts_with("ScalarValues::") {
        return false;
    }
    let leaf = name.rsplit("::").next().unwrap_or(name);
    matches!(
        leaf,
        "ScalarValue"
            | "Boolean"
            | "String"
            | "NumericalValue"
            | "Number"
            | "Complex"
            | "Real"
            | "Rational"
            | "Integer"
            | "Natural"
            | "Positive"
    )
}

/// Kinds that a value expression could take, for callers that want to explain a mismatch.
pub fn declared_is_scalar(index: &Index, el: &Element) -> bool {
    el.type_ref
        .as_deref()
        .and_then(|t| {
            index
                .resolve(el.id, t.split(',').next().unwrap_or(t).trim())
                .found()
        })
        .map(|id| is_scalar(index, id))
        .unwrap_or(false)
}

/// `Kind` of an element that can carry a value, for callers that filter.
pub fn carries_value(el: &Element) -> bool {
    el.value_expr.is_some()
        && matches!(
            el.kind,
            Kind::AttributeUsage | Kind::AttributeDef | Kind::EnumValue
        )
}

/// The type an expression evaluates to, as the id of a type element.
///
/// `boolean_result` used to answer from the shape of a node alone: a comparison is a Boolean, and
/// a call was not decidable. That left every call, every constructor and every sequence operation
/// answering "cannot tell", which is why the checks that require a Boolean skipped them —
/// `filter new A(null, 1, "", false);` is rejected by the reference implementation and passed here.
///
/// What decides each node:
///
///   * a literal is the scalar its own text names, resolved in the scope that wrote it;
///   * a reference is the declared type of what it names;
///   * a comparison, a logical join, `not` and a type test are Booleans whatever their operands;
///   * arithmetic widens: a `Real` on either side makes the result a `Real`, two `Integer`s stay
///     one, and anything else is the type both sides agree on;
///   * a call is the type of its result parameter, a constructor the type it constructs, and a
///     sequence operation the type of the feature it names;
///   * `if c then a else b` has a type only when both branches have the same one.
///
/// What it deliberately does not do: it does not reach for a scalar the model's own scope cannot
/// name. A literal in a model that imports nothing has no `Integer` to point at, and answering
/// `None` keeps the caller on its shape fallback rather than inventing a type the model never
/// mentioned. A common supertype of two different branches is the same kind of guess and is also
/// left unanswered.
pub fn type_of(index: &Index, tree: &ExprTree, id: ExprId) -> Option<usize> {
    let node = tree.node(id);
    match &node.kind {
        ExprKind::Literal => literal_type(index, node.owner, node.text.trim()),
        ExprKind::Ref(name) => {
            let target = expr::resolve_from(index, node.owner, &name.text)?;
            declared_type(index, target)
        }
        // `not x` is a Boolean whatever `x` is; `-x` has the type of `x`.
        ExprKind::Unary(UnaryOp::Not) => boolean_type(index, node.owner),
        ExprKind::Unary(_) => type_of(index, tree, *node.children.first()?),
        ExprKind::Binary(op) => binary_type(index, tree, id, *op),
        // `x istype T` is a question with a truth value, whatever the operand is. `x as T` is not a
        // question: it is the operand read as `T`, so its type is `T` — which is what lets
        // `(x as T).member` look the member up in `T`.
        ExprKind::TypeCheck { kind, type_name } => match kind {
            // `x meta T` is x's metaobject read as metaclass `T`, so it has the type `T` too.
            sysml_model::TypeCheckKind::As | sysml_model::TypeCheckKind::Meta => {
                expr::resolve_from(index, node.owner, &type_name.text)
            }
            _ => boolean_type(index, node.owner),
        },
        // A call is the type of its result parameter; a sequence operation is the type of the
        // feature it names. `result_of` already answers which feature that is.
        ExprKind::Call | ExprKind::Chain { .. } => {
            let target = expr::result_of(index, tree, id)?;
            declared_type(index, target)
        }
        ExprKind::Constructor { type_ref } => expr::resolve_from(index, node.owner, &type_ref.text),
        // `base.member` has the type of that member, which means finding it in the base's type.
        // The base's type is what `type_of` answers one level down, so `D().b` reads `D` from the
        // constructor and then `b` from `D` — the step that flattening the two halves into the text
        // `D().b` made impossible.
        ExprKind::Member { member } => {
            let base = *node.children.first()?;
            member_type(index, tree, base, member)
        }
        // `children` are test, then, else; the test decides which branch runs, not what it is.
        ExprKind::Conditional => {
            let mut branches = node.children.iter().skip(1);
            let then_type = type_of(index, tree, *branches.next()?)?;
            let else_type = type_of(index, tree, *branches.next()?)?;
            (then_type == else_type).then_some(then_type)
        }
        ExprKind::Sequence | ExprKind::Lambda { .. } | ExprKind::Opaque => None,
    }
}

/// The type of `member` as read from whatever `base` denotes.
///
/// Two base forms matter and they reach the answer differently:
///
///   * a constructor or a cast — the base's *type* is the constructor's type or the cast's type, so
///     the member is looked for there. `type_of` answers that directly.
///   * a declared feature — `instance1.b` reads `b` from `instance1`'s type, which is also what
///     `type_of` answers, so the same walk serves both.
///
/// `None` when the base has no type this layer can settle, or when the type does not declare the
/// member. The caller then falls back to its shape rule rather than inventing a type, which is the
/// same contract [`type_of`] keeps for every other variant.
fn member_type(index: &Index, tree: &ExprTree, base: ExprId, member: &str) -> Option<usize> {
    let base_type = type_of(index, tree, base)?;
    let target = expr::resolve_member(index, base_type, member)?;
    declared_type(index, target)
}

/// Whether `actual` is `expected` or specializes it.
///
/// The question a type system asks of a redefinition, an argument or a bound: narrowing is legal,
/// widening is not. `Index::is_subtype` walks the specialization closure with a visited set, which
/// is what the library's own cycles need.
pub fn conforms(index: &Index, actual: usize, expected: usize) -> bool {
    index.is_subtype(actual, expected)
}

/// Whether a resolved type is, or specializes, the library's `Boolean`.
///
/// The library spells it `ScalarValues::Boolean`; a model may declare `attribute def Boolean` of
/// its own, or a definition specializing one. The closure is walked because a rule that asks for a
/// Boolean accepts a subtype of one.
pub fn is_boolean(index: &Index, type_id: usize) -> bool {
    let mut frontier = vec![type_id];
    let mut seen = std::collections::HashSet::from([type_id]);
    while let Some(current) = frontier.pop() {
        if leaf_of(index, current) == "Boolean" {
            return true;
        }
        for next in index.supertypes_of_public(current) {
            if seen.insert(next) {
                frontier.push(next);
            }
        }
    }
    false
}

/// The type a feature declares, resolved in the feature's own scope.
fn declared_type(index: &Index, id: usize) -> Option<usize> {
    let text = index.type_of(id)?;
    let head = text.split(',').next()?.trim();
    if head.is_empty() {
        return None;
    }
    index.resolve(id, head).found()
}

fn binary_type(index: &Index, tree: &ExprTree, id: ExprId, op: BinaryOp) -> Option<usize> {
    let node = tree.node(id);
    match op {
        BinaryOp::Less
        | BinaryOp::LessEq
        | BinaryOp::Greater
        | BinaryOp::GreaterEq
        | BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::And
        | BinaryOp::Or
        | BinaryOp::Xor
        | BinaryOp::Implies => boolean_type(index, node.owner),
        // `20 [min]` is the value 20 with a unit; the unit does not change its type.
        BinaryOp::Unit => type_of(index, tree, *node.children.first()?),
        BinaryOp::Add
        | BinaryOp::Sub
        | BinaryOp::Mul
        | BinaryOp::Div
        | BinaryOp::Rem
        | BinaryOp::Pow => {
            let left = type_of(index, tree, *node.children.first()?)?;
            let right = type_of(index, tree, *node.children.get(1)?)?;
            let (left_leaf, right_leaf) = (leaf_of(index, left), leaf_of(index, right));
            // A `Real` on either side makes the result one, and `Real + Integer` is not the
            // `Integer` the left operand alone would suggest.
            if left_leaf == "Real" {
                return Some(left);
            }
            if right_leaf == "Real" {
                return Some(right);
            }
            if left_leaf == "Integer" && right_leaf == "Integer" {
                return Some(left);
            }
            (left == right).then_some(left)
        }
        // `a ?? b` is `a` or `b`; the two sides agreeing is the only case the type is settled.
        BinaryOp::NullCoalesce => {
            let left = type_of(index, tree, *node.children.first()?)?;
            let right = type_of(index, tree, *node.children.get(1)?)?;
            (left == right).then_some(left)
        }
        BinaryOp::Other => None,
    }
}

fn literal_type(index: &Index, scope: usize, text: &str) -> Option<usize> {
    if text == "true" || text == "false" {
        return boolean_type(index, scope);
    }
    if text.starts_with('\'') || text.starts_with('"') {
        return named_type(index, scope, "String");
    }
    if text.parse::<i64>().is_ok() {
        return named_type(index, scope, "Integer");
    }
    if text.parse::<f64>().is_ok() {
        return named_type(index, scope, "Real");
    }
    None
}

fn boolean_type(index: &Index, scope: usize) -> Option<usize> {
    named_type(index, scope, "Boolean")
}

fn named_type(index: &Index, scope: usize, name: &str) -> Option<usize> {
    index.resolve(scope, name).found()
}

fn leaf_of(index: &Index, id: usize) -> &str {
    let name = &index.qualified[id];
    name.rsplit("::").next().unwrap_or(name)
}
