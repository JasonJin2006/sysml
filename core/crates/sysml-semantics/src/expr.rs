//! An id space for expressions, kept out of the element tree.
//!
//! A rule that asks a question about *an expression* — "is its result a Boolean", "what does it
//! reference" — needs to name one. [`Expr`](sysml_model::Expr) is a recursive value with no
//! identity, and putting its nodes into [`Element::children`] would make every expression node a
//! *member* of the declaration that wrote it: every rule that counts members would start counting
//! arguments, which is how the projection acquired phantom members before.
//!
//! So expressions get their own id space. [`ExprId`] is a distinct type, ids are dense from zero
//! within one [`ExprTree`], and the element that wrote the expression is carried on the node as
//! `owner` rather than being implied. A caller that wants "the expressions of this declaration"
//! asks [`ExprTree::roots_of`]; a caller that wants to walk one asks [`ExprTree::preorder`].
//!
//! # What the tree is for
//!
//! [`result_of`] answers `Expression::result` for the shapes the notation writes: a reference
//! denotes the feature it names, and a call denotes the return parameter of what it calls (the same
//! `derived::result_parameter` a declaration's `return` clause fills). [`boolean_result`] answers
//! the narrower question the filter rules ask — whether an expression's value is a `Boolean` —
//! from the operator's signature, a literal, or the declared type of what is referenced.
//!
//! Both answer [`Option`]: `None` is "the tree cannot tell", never "false". A rule that treats an
//! unknown as a violation would report the whole corpus.

use std::collections::HashMap;

use sysml_model::{BinaryOp, Element, Expr, NameRef, TypeCheckKind, UnaryOp};
use sysml_resolve::{Index, Resolution};

/// A position in an [`ExprTree`]. Distinct from an element id on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExprId(pub usize);

/// Which clause of a declaration an expression was written in.
///
/// A rule asks about *the guard* or *the trigger*, not about "one of the expressions this element
/// wrote", so the field is recorded on every node and the accessors below select by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExprField {
    /// `= v` / `:= v`.
    Value,
    /// `if <condition>` — a requirement's or a transition's condition.
    Predicate,
    /// `accept …` on a transition.
    Trigger,
    /// `do …` on a transition.
    Effect,
}

/// One expression node, with its children in [`ExprNode::children`].
#[derive(Debug, Clone)]
pub struct ExprNode {
    /// What the node is, without its children.
    pub kind: ExprKind,
    /// Which clause wrote it; the same for every node of one expression.
    pub field: ExprField,
    /// The element whose declaration wrote the expression.
    pub owner: usize,
    /// Where in the expression this node sits; `None` for a declaration's top-level expression.
    pub parent: Option<ExprId>,
    /// The subexpressions, in authored order.
    pub children: Vec<ExprId>,
    /// The name each argument was written under, parallel to `children` after the callee:
    /// `f(mass = 1)` states that the argument redefines `mass`. Empty for a node that is not a
    /// call or a constructor.
    ///
    /// This is the fact the invocation rules are stated over
    /// (`inp.ownedRedefinition.redefinedFeature->intersection(parameters)`). It is **not** a
    /// member list: an argument is still an expression rather than an owned feature, which is why
    /// it stays here instead of in `Element::children`.
    pub arg_names: Vec<Option<String>>,
    /// The node's own source text, rendered back from the tree.
    pub text: String,
}

/// The part of an expression a rule asks about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprKind {
    /// A literal number, string or boolean.
    Literal,
    /// A name or dotted path: `d.endurance`, `SI::kg`.
    Ref(NameRef),
    Binary(BinaryOp),
    Unary(UnaryOp),
    /// `callee(args)` — `children[0]` is the callee, the rest are the arguments.
    Call,
    /// `new T(args)` — the type is kept, `children` are the arguments.
    Constructor {
        type_ref: NameRef,
    },
    /// `cond ? a : b` — `children` are test, then, else.
    Conditional,
    /// A comma sequence or a grouping.
    Sequence,
    /// `operand istype T` — `children[0]` is the operand.
    TypeCheck {
        kind: TypeCheckKind,
        type_name: NameRef,
    },
    /// A lambda, with the names it binds.
    Lambda {
        params: Vec<String>,
    },
    /// `base.member` where the base is not a name — `D().b`, `(x as T).b`.
    ///
    /// `children[0]` is the base. The member is *not* a child and not a `NameRef`: it names a
    /// feature of the base's type, so resolving it needs the base's type first. Keeping it here as
    /// text lets the member check ask the index once that type is known.
    Member {
        member: String,
    },
    /// `base->op(args)` — `children[0]` is the base, the rest are the arguments.
    Chain {
        op: String,
        params: Vec<String>,
    },
    /// A form this layer does not interpret; `children` are still walked.
    Opaque,
}
impl ExprKind {
    /// Whether the node is a call or a constructor — something that supplies arguments.
    pub fn is_invocation(&self) -> bool {
        matches!(self, ExprKind::Call | ExprKind::Constructor { .. })
    }

    /// The metaclasses this node projects to, by the specification's own names.
    ///
    /// The OCL asks its questions of metaclasses — `validateFeatureReferenceExpressionReferent`
    /// is stated of `FeatureReferenceExpression` — and before this mapping no expression node
    /// could answer to one, so every rule about an expression was undecidable no matter what its
    /// own condition was.
    ///
    /// A node answers to more than one name where the specification's hierarchy says so: every
    /// node is an `Expression`, a call is both an `InvocationExpression` and the
    /// `InstantiationExpression` it specializes, and a bare reference is a
    /// `FeatureReferenceExpression` because that is what a name evaluates to.
    ///
    /// `Opaque` and `Sequence` map to `Expression` alone: the layer does not interpret them, and
    /// claiming a narrower metaclass would let a rule decide something this tree cannot see.
    pub fn metaclasses(&self) -> &'static [&'static str] {
        const EXPRESSION: &[&str] = &["Expression"];
        const REFERENCE: &[&str] = &["FeatureReferenceExpression", "Expression"];
        const INVOCATION: &[&str] = &[
            "InvocationExpression",
            "InstantiationExpression",
            "Expression",
        ];
        const CONSTRUCTOR: &[&str] = &[
            "ConstructorExpression",
            "InstantiationExpression",
            "Expression",
        ];
        const OPERATOR: &[&str] = &["OperatorExpression", "Expression"];
        const CHAIN: &[&str] = &["FeatureChainExpression", "Expression"];
        const LITERAL: &[&str] = &["LiteralExpression", "Expression"];
        match self {
            ExprKind::Ref(_) => REFERENCE,
            ExprKind::Call => INVOCATION,
            ExprKind::Constructor { .. } => CONSTRUCTOR,
            ExprKind::Binary(_) | ExprKind::Unary(_) => OPERATOR,
            ExprKind::Chain { .. } => CHAIN,
            ExprKind::Literal => LITERAL,
            _ => EXPRESSION,
        }
    }

    /// Whether this node is a kind of `metaclass`, through the names above.
    pub fn is_kind_of(&self, metaclass: &str) -> bool {
        self.metaclasses().contains(&metaclass)
    }
}

/// Every expression in a model, flattened into one id space.
#[derive(Debug, Default)]
pub struct ExprTree {
    nodes: Vec<ExprNode>,
    by_owner: HashMap<usize, Vec<ExprId>>,
}

impl ExprTree {
    /// Flatten every element's value and predicate expressions, in tree order.
    ///
    /// A declaration's value expression comes before its predicate, and elements come in preorder,
    /// so two builds of one model assign the same ids to the same nodes.
    pub fn build(roots: &[Element]) -> Self {
        let mut tree = ExprTree::default();
        for root in roots {
            tree.collect(root);
        }
        tree
    }

    fn collect(&mut self, el: &Element) {
        for (expr, field) in [
            (&el.value_expr, ExprField::Value),
            (&el.predicate, ExprField::Predicate),
            (&el.trigger_expr, ExprField::Trigger),
            (&el.effect_expr, ExprField::Effect),
        ] {
            let Some(expr) = expr else { continue };
            let id = self.push(expr, el.id, None, field);
            self.by_owner.entry(el.id).or_default().push(id);
        }
        for child in &el.children {
            self.collect(child);
        }
    }

    fn push(
        &mut self,
        expr: &Expr,
        owner: usize,
        parent: Option<ExprId>,
        field: ExprField,
    ) -> ExprId {
        let id = ExprId(self.nodes.len());
        // Reserve the slot before the children, so a node's id precedes its descendants'.
        self.nodes.push(ExprNode {
            kind: ExprKind::Opaque,
            field,
            owner,
            parent,
            children: Vec::new(),
            arg_names: Vec::new(),
            text: expr.render(),
        });
        let mut children = Vec::new();
        let mut names: Vec<Option<String>> = Vec::new();
        let kind = match expr {
            Expr::Int(_) | Expr::Real(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => {
                ExprKind::Literal
            }
            Expr::Ref(name) => ExprKind::Ref(name.clone()),
            Expr::Member { base, member } => {
                children.push(self.push(base, owner, Some(id), field));
                ExprKind::Member {
                    member: member.clone(),
                }
            }
            Expr::Binary { op, left, right } => {
                children.push(self.push(left, owner, Some(id), field));
                children.push(self.push(right, owner, Some(id), field));
                ExprKind::Binary(*op)
            }
            Expr::Unary { op, operand } => {
                children.push(self.push(operand, owner, Some(id), field));
                ExprKind::Unary(*op)
            }
            Expr::Call {
                callee,
                args,
                arg_names,
            } => {
                children.push(self.push(callee, owner, Some(id), field));
                for arg in args {
                    children.push(self.push(arg, owner, Some(id), field));
                }
                // `children[0]` is the callee; the names line up with `children[1..]`.
                names = arg_names.clone();
                ExprKind::Call
            }
            Expr::Constructor {
                type_ref,
                args,
                arg_names,
            } => {
                for arg in args {
                    children.push(self.push(arg, owner, Some(id), field));
                }
                names = arg_names.clone();
                ExprKind::Constructor {
                    type_ref: type_ref.clone(),
                }
            }
            Expr::Conditional {
                test,
                then_expr,
                else_expr,
            } => {
                children.push(self.push(test, owner, Some(id), field));
                children.push(self.push(then_expr, owner, Some(id), field));
                children.push(self.push(else_expr, owner, Some(id), field));
                ExprKind::Conditional
            }
            Expr::Sequence(items) => {
                for item in items {
                    children.push(self.push(item, owner, Some(id), field));
                }
                ExprKind::Sequence
            }
            Expr::TypeCheck {
                kind,
                operand,
                type_name,
            } => {
                children.push(self.push(operand, owner, Some(id), field));
                ExprKind::TypeCheck {
                    kind: *kind,
                    type_name: type_name.clone(),
                }
            }
            Expr::Lambda { params, body } => {
                for expr in body {
                    children.push(self.push(expr, owner, Some(id), field));
                }
                ExprKind::Lambda {
                    params: params.clone(),
                }
            }
            Expr::Chain {
                op,
                base,
                args,
                params,
            } => {
                children.push(self.push(base, owner, Some(id), field));
                for arg in args {
                    children.push(self.push(arg, owner, Some(id), field));
                }
                ExprKind::Chain {
                    op: op.clone(),
                    params: params.clone(),
                }
            }
            Expr::Opaque(items) => {
                for item in items {
                    children.push(self.push(item, owner, Some(id), field));
                }
                ExprKind::Opaque
            }
        };
        self.nodes[id.0] = ExprNode {
            kind,
            field,
            owner,
            parent,
            children,
            arg_names: names,
            text: expr.render(),
        };
        id
    }

    /// One node by id.
    pub fn node(&self, id: ExprId) -> &ExprNode {
        &self.nodes[id.0]
    }

    /// Every expression written by the declaration at `element`, value first.
    pub fn roots_of(&self, element: usize) -> &[ExprId] {
        self.by_owner
            .get(&element)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The root of one clause's expression, if the declaration wrote that clause.
    pub fn root_of(&self, element: usize, field: ExprField) -> Option<ExprId> {
        self.roots_of(element)
            .iter()
            .copied()
            .find(|id| self.node(*id).field == field)
    }

    /// The base node of the first `base.member` access under `root` that names `member`.
    ///
    /// `check_value` walks the raw `Expr` to find member accesses, but the type of a base is only
    /// answerable through the tree, which is what assigns node ids. This maps one to the other: the
    /// first `Member` node in preorder whose member matches. Searching by name rather than by index
    /// is enough because the caller only needs *a* base that types correctly, and two accesses to
    /// the same member of the same value are the same question.
    pub fn find_base(&self, root: ExprId, member: &str) -> Option<ExprId> {
        for id in self.preorder(root) {
            if let ExprKind::Member { member: found } = &self.node(id).kind {
                if found == member {
                    return self.node(id).children.first().copied();
                }
            }
        }
        None
    }

    /// Every node, so a caller can walk the whole space once.
    pub fn ids(&self) -> impl Iterator<Item = ExprId> + '_ {
        (0..self.nodes.len()).map(ExprId)
    }

    /// `id` and its descendants, parents before children.
    pub fn preorder(&self, id: ExprId) -> Vec<ExprId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            out.push(current);
            for child in self.node(current).children.iter().rev() {
                stack.push(*child);
            }
        }
        out
    }

    /// The number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// `Expression::result` for the shapes the notation writes, as the element it denotes.
///
/// * A reference denotes what it names: `x` is the feature `x`.
/// * A call denotes the return parameter of what it calls — the same
///   [`derived::result_parameter`](crate::derived::result_parameter) a `return` clause fills.
/// * A constructor denotes an instance of its type, which is not an element the tree holds.
/// * An operator, a conditional or a literal computes a value; nothing in the tree names it.
///
/// `None` is "not named here", not "no result". The rules that ask `result.owningType = self` are
/// asking about the expression *element*, which the projection does not build, so they are decided
/// by the derivation rather than by this function.
/// One argument of a call or a constructor, as the rules about invocations need it.
pub struct Argument {
    /// The feature the argument was written under, confirmed to be one of `instantiated`'s.
    pub redefines: usize,
    /// The node holding the argument's value.
    pub value: ExprId,
}

/// The arguments of a call or a constructor that name a feature of what is being instantiated.
///
/// The invocation rules are stated over `inp.ownedRedefinition.redefinedFeature`: an argument
/// redefines one input parameter of the callee, and two arguments must not pick the same one. The
/// name is the whole of that relation — `F(x = 1)` redefines `x` — and this resolves it against the
/// instantiated type so a rule can compare ids rather than text.
///
/// An argument written without a name is skipped: it is positional, and the specification's
/// relation is stated over the redefinition, which a positional argument does not write.
pub fn arguments_of(index: &Index, tree: &ExprTree, id: ExprId) -> Vec<Argument> {
    let node = tree.node(id);
    let Some(instantiated) = instantiated_type_of(index, tree, id) else {
        return Vec::new();
    };
    // A call puts the callee first, so its arguments start at 1; a constructor has no callee.
    let first = match node.kind {
        ExprKind::Call => 1,
        ExprKind::Constructor { .. } => 0,
        _ => return Vec::new(),
    };
    node.children
        .iter()
        .enumerate()
        .skip(first)
        .filter_map(|(i, &child)| {
            let name = node.arg_names.get(i - first)?.as_deref()?;
            // The named argument fills a feature of what is instantiated, and that feature may be
            // inherited: `part def D :> Base; new D(b = 1)` names `Base`'s `b`. Asking only the
            // instantiated type's own children made every inherited name look unknown.
            let redefines = index.member_of_access(instantiated, name).found()?;
            Some(Argument {
                redefines,
                value: child,
            })
        })
        .collect()
}

/// The type a call or a constructor instantiates — `InstantiationExpression::instantiatedType`.
fn instantiated_type_of(index: &Index, tree: &ExprTree, id: ExprId) -> Option<usize> {
    let node = tree.node(id);
    match &node.kind {
        ExprKind::Call => {
            let callee = *node.children.first()?;
            match &tree.node(callee).kind {
                ExprKind::Ref(name) => resolve_from(index, node.owner, &name.text),
                _ => None,
            }
        }
        ExprKind::Constructor { type_ref } => resolve_from(index, node.owner, &type_ref.text),
        _ => None,
    }
}

pub fn result_of(index: &Index, tree: &ExprTree, id: ExprId) -> Option<usize> {
    let node = tree.node(id);
    match &node.kind {
        ExprKind::Ref(name) => resolve_from(index, node.owner, &name.text),
        ExprKind::Call => {
            let callee = *node.children.first()?;
            let target = match &tree.node(callee).kind {
                ExprKind::Ref(name) => resolve_from(index, node.owner, &name.text)?,
                _ => return None,
            };
            crate::derived::result_parameter(index, target).or(Some(target))
        }
        // `base->op` names a feature of what `base` evaluates to. `targetFeature` is "the Feature
        // that is accessed by this FeatureChainExpression", and it lives in the base's type — the
        // same walk `resolve_path` does for a dotted name, one step at a time.
        ExprKind::Chain { op, .. } => {
            let base = *node.children.first()?;
            let container = result_of(index, tree, base)?;
            step_into(index, container, op)
        }
        _ => None,
    }
}

/// One access step: the member called `name` of what `container` is.
///
/// The container is asked for its own members first — `part i { attribute a; }` has `a` on itself —
/// and then, one level down, for its type's. A chain's operator is stored as text rather than as a
/// resolved reference, so this is the only place the step is taken.
fn step_into(index: &Index, container: usize, name: &str) -> Option<usize> {
    let name = name.split(OP_PAREN).next().unwrap_or(name).trim();
    if name.is_empty() {
        return None;
    }
    // The container's own members, then its type's, then that type's specialization closure — and
    // what the scope imports. Reading only the container and one level of its type reported
    // `d->b` as missing when `b` is inherited from what `d` is: `part def Base { attribute b; }
    // part def D :> Base; part d : D; d->b` is a legal access and came back as SEM095.
    index.member_of_access(container, name).found()
}

/// A chain operator may carry an argument list; the name ends where it starts.
const OP_PAREN: char = '\u{28}';
/// Whether the expression's value is a `Boolean`, as far as the tree can tell.
///
/// The type settles it whenever the tree can name one: [`crate::typing::type_of`] answers with the
/// type element, and a `Boolean` — or a definition specializing one — is a Boolean while any other
/// named type is not. That is what lets a call and a constructor be judged at all:
/// `filter new A(null, 1, "", false);` constructs an `A`, so its value is not a predicate, and the
/// reference implementation rejects it while this checker used to pass it.
///
/// When the type cannot be named the shape still decides what the notation fixes: a comparison, a
/// logical operator, a `not` or a type test yields a Boolean; a literal boolean is one; a number or
/// a string literal is definitively not one. Everything else answers `None`, which callers read as
/// "cannot tell".
pub fn boolean_result(index: &Index, tree: &ExprTree, id: ExprId) -> Option<bool> {
    if let Some(type_id) = crate::typing::type_of(index, tree, id) {
        return Some(crate::typing::is_boolean(index, type_id));
    }
    match &tree.node(id).kind {
        ExprKind::Literal => match tree.node(id).text.trim() {
            "true" | "false" => Some(true),
            // A number or a string literal is definitively not a truth value. Returning `None`
            // here meant `accept when 5` was left alone: the caller treats `None` as "cannot
            // tell", which is right for an expression whose type is unknown and wrong for one
            // whose type is the literal in front of it.
            text if text.parse::<f64>().is_ok() => Some(false),
            text if text.starts_with('\'') || text.starts_with('"') => Some(false),
            _ => None,
        },
        ExprKind::Binary(op) => match op {
            BinaryOp::Less
            | BinaryOp::LessEq
            | BinaryOp::Greater
            | BinaryOp::GreaterEq
            | BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::Xor
            | BinaryOp::Implies => Some(true),
            _ => None,
        },
        // A type *test* yields a truth value; a cast (`as`, `meta`) yields its operand.
        ExprKind::Unary(UnaryOp::Not)
        | ExprKind::TypeCheck {
            kind:
                sysml_model::TypeCheckKind::IsType
                | sysml_model::TypeCheckKind::HasType
                | sysml_model::TypeCheckKind::MetaTest,
            ..
        } => Some(true),
        _ => None,
    }
}

/// Whether an expression can be evaluated at model level, three-state.
///
/// `deriveExpressionIsModelLevelEvaluable` states the property of an `Expression`
/// (`generated.rs:7919-7922`) and defers to the `modelLevelEvaluable()` operation, whose bodies the
/// metamodel gives per metaclass (`generated.rs:9312-9387`). Read together:
///
///   * a `LiteralExpression`, a `NullExpression` and a `MetadataAccessExpression` are evaluable;
///   * a `ConstructorExpression` is evaluable when every argument is;
///   * an `InvocationExpression` is evaluable when every argument is *and* its function is — and
///     nothing derives `Function::isModelLevelEvaluable`: the property is declared `derived` with
///     default `false` and no rule computes it (`generated.rs:2224`, `:2314`);
///   * a `FeatureReferenceExpression` is evaluable under conditions on the referent's featuring
///     type and value.
///
/// The last two decide what this function may answer. A call is judged by whether the kernel
/// itself evaluates that function — `sum`, `size` and the rest of the builtin table are values it
/// computes, so answering "not evaluable" for them would contradict the evaluator one layer down.
/// A bare reference answers `None`: the OCL asks for `featuringType`, `owningType` and `valuation`,
/// and the tree keeps none of the three, so a report on it would be a guess.
pub fn is_model_level_evaluable(tree: &ExprTree, id: ExprId) -> Option<bool> {
    let node = tree.node(id);
    let children = || node.children.iter().copied();
    match &node.kind {
        // A literal is a value already. `null` arrives as an empty node rather than as a literal
        // of its own, so it is recognised by its text.
        ExprKind::Literal => Some(true),
        ExprKind::Opaque => {
            let text = node.text.trim();
            if text.starts_with('@') || text == "null" {
                Some(true)
            } else if node.children.is_empty() {
                // An operator or a cast whose operand was not written, an expression form this
                // layer does not interpret. Nothing in the node says what its value is.
                None
            } else {
                evaluable_all(tree, children())
            }
        }
        ExprKind::Binary(_) => evaluable_all(tree, children()),
        // `~x` written where a value belongs is a conjugation (`KerML.xtext:336`, `:412`): no
        // function of the operand's value is applied, so there is nothing to evaluate. The pilot
        // states exactly that for `= ~3` and `filter ~(as A).z`.
        ExprKind::Unary(UnaryOp::Conjugate) => Some(false),
        ExprKind::Unary(_) => evaluable_all(tree, children()),
        ExprKind::Conditional | ExprKind::Sequence => evaluable_all(tree, children()),
        ExprKind::Constructor { .. } => evaluable_all(tree, children()),
        // Whether `D().b` has a value depends on the value `D()` produces, which this layer does
        // not build. The base is still asked about, so a genuinely unevaluable base is reported.
        ExprKind::Member { .. } => evaluable_all(tree, children()),
        ExprKind::TypeCheck { .. } => evaluable_all(tree, children()),
        ExprKind::Call => {
            let mut rest = children();
            let Some(callee) = rest.next() else {
                return None;
            };
            let ExprKind::Ref(name) = &tree.node(callee).kind else {
                return Some(false);
            };
            if !crate::is_builtin(&name.text) {
                // A function the model declares: whether it evaluates at model level is a question
                // about its body, which this layer does not read.
                return Some(false);
            }
            evaluable_all(tree, rest)
        }
        ExprKind::Chain { op, params } => {
            // A braced body declares parameters: `y->collect { in x; x }` applies a lambda, and a
            // lambda is not a value the layer evaluates.
            if !params.is_empty() {
                return Some(false);
            }
            if !crate::is_builtin(op) {
                return Some(false);
            }
            // `children[0]` is the base, the rest are the arguments.
            evaluable_all(tree, children())
        }
        ExprKind::Lambda { .. } => Some(false),
        // A bare reference. For the reason in the comment above: the tree cannot see what the OCL
        // asks about it.
        ExprKind::Ref(_) => None,
    }
}

/// `Some(true)` when every node is evaluable, `Some(false)` as soon as one is not, `None` when the
/// answer depends on a node this layer cannot decide.
fn evaluable_all(tree: &ExprTree, ids: impl Iterator<Item = ExprId>) -> Option<bool> {
    let mut undecided = false;
    for id in ids {
        match is_model_level_evaluable(tree, id) {
            Some(false) => return Some(false),
            Some(true) => {}
            None => undecided = true,
        }
    }
    if undecided {
        None
    } else {
        Some(true)
    }
}

/// Resolve a reference written inside an expression owned by `owner`.
pub(crate) fn resolve_from(index: &Index, owner: usize, text: &str) -> Option<usize> {
    let head = text.split(',').next().unwrap_or(text).trim();
    if head.is_empty() {
        return None;
    }
    // `v1.S` is one reference with a `.` inside it, and `Index::resolve` walks `::` only: asking it
    // about a dotted name looks for a member literally called `v1.S` and answers Unresolved. Every
    // dotted reference in a value went unresolved because of this, which silently removed them
    // from every rule that goes through `result_of` — `v1.S` naming a struct in value position was
    // never reported.
    let name = sysml_model::NameRef::new(head, 0);
    match index.resolve_path(owner, &name) {
        Resolution::Found(found) => Some(found),
        _ => None,
    }
}

/// Resolve `member` in the scope of the type `base_type`, for a `base.member` access.
///
/// The member is looked for in the type, and through it in the type's specialization closure, because
/// `D().b` may name a feature inherited from a supertype of `D`. `Index::member_of_access` is the walk
/// that already answers "the member of what this denotes" for a dotted `Ref`, so both spellings of a
/// member access agree.
///
/// `None` when the type does not declare the member. That is the case that used to be silently
/// accepted after a cast: `(x as T).zzzNoSuchMember` passed because the member was dropped from the
/// expression entirely.
pub(crate) fn resolve_member(index: &Index, base_type: usize, member: &str) -> Option<usize> {
    if member.is_empty() {
        return None;
    }
    match index.member_of_access(base_type, member) {
        Resolution::Found(found) => Some(found),
        _ => None,
    }
}
