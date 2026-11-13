//! Semantic checks a name resolver cannot do.
//!
//! The resolver answers "does this name exist". It cannot answer "does this constraint hold",
//! "do these two ports actually exist", or "is this value computable" — so a model containing
//! `require constraint { 1 > 0 }`, `attribute x = undefinedName / 2` and a connection between
//! two ports that were never declared passed every gate this project had.
//!
//! What lives here is evaluation, not type theory: values are reals, integers and booleans,
//! units are carried but not dimension-checked, and a reference with no computable value makes
//! the answer `Inconclusive` rather than an error. That three-way answer is the point. A
//! requirement nobody can evaluate, and a requirement that is false, are different problems.

use std::collections::HashMap;

pub mod conformance;
pub mod derived;
pub mod expr;
pub mod rules;
pub mod typing;

use sysml_model::{BinaryOp, Element, Expr, Kind, UnaryOp};
use sysml_resolve::{Index, Resolution};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Real(f64),
    Int(i64),
    Bool(bool),
}

impl Value {
    pub fn as_real(&self) -> Option<f64> {
        match self {
            Value::Real(v) => Some(*v),
            Value::Int(v) => Some(*v as f64),
            Value::Bool(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn render(&self) -> String {
        match self {
            Value::Real(v) => {
                if v.fract() == 0.0 && v.abs() < 1e15 {
                    format!("{}", *v as i64)
                } else {
                    format!("{v}")
                }
            }
            Value::Int(v) => v.to_string(),
            Value::Bool(v) => v.to_string(),
        }
    }
}

/// What a constraint came out as.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Pass(Value),
    Fail(Value),
    /// The expression is well formed but something it reads has no value, so nothing can be
    /// said about whether the requirement holds.
    Inconclusive(String),
    /// The expression itself is broken: unknown name, wrong operand kinds.
    Error(String),
}

impl Verdict {
    pub fn keyword(&self) -> &'static str {
        match self {
            Verdict::Pass(_) => "pass",
            Verdict::Fail(_) => "fail",
            Verdict::Inconclusive(_) => "inconclusive",
            Verdict::Error(_) => "error",
        }
    }
}

/// Names the language provides as functions rather than as model elements. A predicate that
/// calls `sum(...)` is not referring to something the model forgot to declare.
const BUILTIN_FUNCTIONS: &[&str] = &[
    "sum", "size", "abs", "min", "max", "sqrt", "floor", "ceil", "round", "product", "isEmpty",
    "notEmpty", "includes", "excludes", "count", "select", "collect", "reject", "forAll", "exists",
    "reduce", "as", "istype", "hastype",
];

/// Whether a name is one the language provides rather than one the model declares.
///
/// Public because the reference report in the CLI walks the resolver's own reference list, which
/// does not know about this table: without the filter, `sum((1, 2, 3))` in a predicate was
/// reported as `predicate unresolved: sum` while the same call evaluated to 6.
pub fn is_builtin(name: &str) -> bool {
    let leaf = name.rsplit("::").next().unwrap_or(name);
    let leaf = leaf.rsplit('.').next().unwrap_or(leaf);
    BUILTIN_FUNCTIONS.contains(&leaf)
}

/// Strip a multiplicity bracket from a path segment: `[1] subsystemB.part5` names
/// `subsystemB.part5` with a bound, not a feature called `[1] subsystemB`.
fn strip_multiplicity(segment: &str) -> &str {
    let trimmed = segment.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some(close) = rest.find(']') {
            return rest[close + 1..].trim();
        }
    }
    trimmed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub code: &'static str,
    pub severity: Severity,
    /// Qualified name of the element the issue is about.
    pub at: String,
    pub line: u32,
    pub message: String,
}

impl Issue {
    pub fn render(&self) -> String {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        format!(
            "{}:{}: {}[{}]: {}",
            self.at, self.line, level, self.code, self.message
        )
    }
}

/// One requirement's evaluation, reported separately from the issues so a ledger can be built
/// from it.
#[derive(Debug, Clone)]
pub struct RequirementVerdict {
    pub qualified: String,
    pub line: u32,
    pub verdict: Verdict,
    /// How many model elements (not literals) the predicate reads.
    pub model_refs: usize,
}

pub struct Checker<'a> {
    roots: &'a [Element],
    index: Index,
    flat: Vec<Option<&'a Element>>,
    /// What a calculation's parameters are bound to while its body is evaluated.
    ///
    /// A cell rather than a parameter through every `eval` call: the evaluator threads `from`,
    /// `expr` and `depth` through forty recursive calls, and the fourth argument would be `&HashMap`
    /// in every one of them to serve two. It is scoped — pushed before the body, restored after —
    /// and nothing outside a call reads it.
    locals: std::cell::RefCell<std::collections::HashMap<String, (Value, Option<String>)>>,
    /// The expression trees, built once.
    ///
    /// `check_value` already walked each value's expression through `name_statuses`, which reads the
    /// `Expr` directly; the member check needs the *tree*, because answering "what type is this
    /// base" requires the node ids `ExprTree` assigns. Building a tree per element would rebuild the
    /// whole model's expressions once per element, so it is built here and reused.
    exprs: crate::expr::ExprTree,
}

impl<'a> Checker<'a> {
    pub fn new(roots: &'a [Element], library: &'a [Element]) -> Self {
        let index = Index::build(roots).with_library(library);
        let mut flat: Vec<Option<&'a Element>> = Vec::new();
        fn walk<'a>(el: &'a Element, flat: &mut Vec<Option<&'a Element>>) {
            if el.id >= flat.len() {
                flat.resize(el.id + 1, None);
            }
            flat[el.id] = Some(el);
            for child in &el.children {
                walk(child, flat);
            }
        }
        for root in roots {
            walk(root, &mut flat);
        }
        for root in library {
            walk(root, &mut flat);
        }
        flat.resize(index.len(), None);
        // Both the project and the library: a value may read a member of a library type —
        // `(that as Occurrence).localClock` is the library's own spelling — so the tree has to hold
        // the library's expressions too or those members would be looked for in an empty type.
        let mut all: Vec<Element> = Vec::new();
        all.extend_from_slice(roots);
        all.extend_from_slice(library);
        let exprs = crate::expr::ExprTree::build(&all);
        // The index is complete at this point, so the supertype cache can be switched on and left
        // to fill on demand. It must be switched on *here* rather than inside `supertypes_of`: the
        // index resolves as it builds, and recording an answer during that scored 342/387 with 48
        // spurious diagnostics, because it recorded "no supertype" for clauses that had not been
        // resolved yet. What it must not do is compute every id up front — that is O(index) for a
        // question the rules ask about a handful of ids, and the index carries the whole standard
        // library: measured, `check` on a five-line model took 3,441 ms that way.
        index.enable_supertype_cache();
        Self {
            roots,
            index,
            flat,
            locals: std::cell::RefCell::new(std::collections::HashMap::new()),
            exprs,
        }
    }

    pub fn issues(&self) -> Vec<Issue> {
        let mut out = Vec::new();
        self.check_syntax(&mut out);
        self.check_connections(&mut out);
        self.check_values(&mut out);
        check_behaviour_ends(self, &mut out);
        self.check_predicates(&mut out);
        self.check_satisfactions(&mut out);
        self.check_spec_rules(&mut out);
        out
    }

    /// The places the parser could not read.
    ///
    /// A member the grammar rejected lowers to a [`Kind::Unsupported`] element named
    /// `syntax error`, which the CLI also lists under `notModelled` — but a file that does not
    /// parse was not "modelled with an unsupported construct", it failed. The reference states
    /// these as errors and the pilot's suite asserts them as errors
    /// (`Import_Visibility_Invalid.sysml.xt:23`), while this kernel reported `ok: false` with an
    /// empty `issues` list and a note beside it: a caller reading `issues` saw a clean model.
    ///
    /// The library is not walked. Its KerML is written in constructs this layer does not model, and
    /// none of them is a parse failure: the loader parses every library file without an error.
    fn check_syntax(&self, out: &mut Vec<Issue>) {
        fn walk(el: &Element, out: &mut Vec<Issue>) {
            if el.kind == Kind::Unsupported && el.name == "syntax error" {
                out.push(Issue {
                    code: "SYN001",
                    severity: Severity::Error,
                    at: el.name.clone(),
                    line: el.line,
                    // The parser's own sentence, kept as written: it names the token it stopped
                    // at, which is the only part a reader can act on.
                    message: el
                        .constraint
                        .clone()
                        .unwrap_or_else(|| "the model could not be parsed here".to_string()),
                });
            }
            for child in &el.children {
                walk(child, out);
            }
        }
        for root in self.roots {
            walk(root, out);
        }
    }

    /// The specification's own well-formedness rules, where this kernel can decide them.
    ///
    /// Only the rules `conformance::implemented()` names run here; each has a test in
    /// `tests/rules.rs` showing it rejects a model that breaks it. Reporting the rest as if they
    /// had been applied is the defect this whole layer exists to prevent.
    fn check_spec_rules(&self, out: &mut Vec<Issue>) {
        let push = |out: &mut Vec<Issue>, code: &'static str, v: conformance::RuleViolation| {
            out.push(Issue {
                code,
                severity: Severity::Error,
                at: v.element,
                line: v.line,
                message: v.message,
            });
        };
        // validateMultiplicityRangeBounds
        for v in conformance::multiplicity_bounds_issues(self.roots) {
            push(out, "SEM013", v);
        }
        // validateMultiplicityRangeBoundResultTypes
        //
        // The bound is a name, not an expression in the tree, so it is evaluated through the same
        // `eval_ref` the rest of the checker uses: `[n..b]` needs `b = n > 3` to yield a Boolean,
        // and `[m1..m2]` needs `m1 = n - 1` to yield -1 when `n = 0`.
        //
        // `b1 = I() > 3` is the case evaluation cannot settle — the checker does not evaluate
        // calls — but whose *type* is still known: a comparison yields a Boolean whatever its
        // operands are. `boolean_result` answers that from the shape of the expression, which is
        // why it is consulted after the value.
        let expressions = crate::expr::ExprTree::build(self.roots);
        for v in conformance::multiplicity_type_issues(
            self.roots,
            |from, name| match self.eval_ref(from, name, 0) {
                Eval::Val(value, _) => Some(value),
                _ => None,
            },
            |from, name| {
                let name_ref = sysml_model::NameRef::new(name, 0);
                let Resolution::Found(id) = self.index.resolve_path(from, &name_ref) else {
                    return false;
                };
                expressions
                    .root_of(id, crate::expr::ExprField::Value)
                    // `Some(false)` means "known not to be a Boolean", which is the opposite
                    // of what this asks. `.is_some()` treated the two as one and reported a
                    // numeric bound as a Boolean one the moment a literal could answer the
                    // question.
                    .and_then(|root| crate::expr::boolean_result(&self.index, &expressions, root))
                    == Some(true)
            },
        ) {
            push(out, "SEM013", v);
        }
        // validateFeatureEndNoDirection
        for v in conformance::end_direction_issues(self.roots) {
            push(out, "SEM014", v);
        }
        // The specification's rules that need the resolver's view, grouped so each module can be
        // written and reviewed on its own. `conformance::implemented()` lists what is enforced.
        for v in rules::all(self.roots, &self.index) {
            out.push(Issue {
                code: v.code,
                severity: v.severity,
                at: v.element,
                line: v.line,
                message: v.message,
            });
        }
        // A literal that contradicts its declared scalar type is a modelling error the resolver
        // cannot see: both names resolve, and the evaluator does not evaluate strings.
        for (at, line, message) in
            typing::scalar_value_issues(&self.index, self.roots, &expressions)
        {
            out.push(Issue {
                code: "SEM015",
                severity: Severity::Error,
                at,
                line,
                message,
            });
        }
    }

    /// A `satisfy R by x` must be well typed: the subject of `R` names a type, and `x` must be
    /// an instance of that type or of something that specializes it.
    ///
    /// This is the same defect class as a dropped constraint clause — the statement parses, the
    /// names resolve, and the claim is nonetheless meaningless. A subsystem cannot satisfy a
    /// requirement whose subject is the whole spacecraft, and without this check the model says
    /// it can.
    fn check_satisfactions(&self, out: &mut Vec<Issue>) {
        for root in self.roots {
            walk_satisfactions(root, self, out);
        }
    }

    fn check_satisfaction(&self, el: &Element, out: &mut Vec<Issue>) {
        let (Some(subject_name), Some(satisfier)) = (&el.to, &el.from) else {
            return;
        };
        let Resolution::Found(req_id) = self.index.resolve(el.id, subject_name) else {
            return; // an unresolved target is reported by the reference pass
        };
        let Resolution::Found(sat_id) = self.index.resolve(el.id, satisfier) else {
            return;
        };
        // The requirement's subject type, e.g. `requirement def R { subject s : Drone; }`.
        let Some(subject_type) = self.subject_type_of(req_id) else {
            return; // a requirement with no subject imposes no type constraint
        };
        let satisfier_type = self
            .type_target(sat_id)
            .or_else(|| self.definition_self(sat_id));
        let Some(satisfier_type) = satisfier_type else {
            return;
        };
        if self.is_subtype(satisfier_type, subject_type) {
            return;
        }
        out.push(Issue {
            code: "SEM009",
            severity: Severity::Error,
            at: self.index.qualified[el.id].clone(),
            line: el.line,
            message: format!(
                "`{}` satisfies `{}`, but its subject is typed `{}` and `{}` is a `{}`, which is not a subtype of it",
                satisfier,
                subject_name,
                self.index.qualified[subject_type],
                satisfier,
                self.index.qualified[satisfier_type]
            ),
        });
    }

    /// The type named by a requirement's `subject` declaration.
    fn subject_type_of(&self, req_id: usize) -> Option<usize> {
        let req = self.flat.get(req_id).copied().flatten()?;
        for child in &req.children {
            if child.kind != Kind::Subject {
                continue;
            }
            let text = child.type_ref.as_deref()?;
            if let Resolution::Found(id) = self.index.resolve(req_id, text) {
                return Some(id);
            }
        }
        None
    }

    /// A definition used as its own type, for `satisfy R by droneDef` where the satisfier is a
    /// definition rather than a typed usage.
    fn definition_self(&self, id: usize) -> Option<usize> {
        if self.index.kind[id].is_definition() {
            Some(id)
        } else {
            None
        }
    }

    /// The declared type of a feature, resolved.
    fn type_of(&self, id: usize) -> Option<usize> {
        let element = self.flat.get(id).copied().flatten()?;
        let text = element.type_ref.as_deref()?;
        let head = text.split(',').next()?.trim();
        self.index.resolve(id, head).found()
    }

    /// Whether `sub` is `sup`, or reaches it through specialization.
    ///
    /// Specialization is a graph, not a chain: `part def C :> A, B` has two parents, and every
    /// one of them can reach `sup`. Walking a single `supertype_ref` at a time answered "is c a
    /// B" with the first parent only, so a model that declared `C :> A, B` was told its `c` is
    /// not a `B`. This walks the transitive closure, breadth first, with a seen set that also
    /// makes a cyclic model terminate instead of relying on a depth cap.
    fn is_subtype(&self, sub: usize, sup: usize) -> bool {
        if sub == sup {
            return true;
        }
        let mut queue = std::collections::VecDeque::from([sub]);
        let mut seen = std::collections::HashSet::from([sub]);
        while let Some(current) = queue.pop_front() {
            let Some(el) = self.flat.get(current).copied().flatten() else {
                continue;
            };
            for text in el.supertype_list() {
                let Resolution::Found(next) = self.index.resolve(current, text) else {
                    continue;
                };
                if next == sup {
                    return true;
                }
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        false
    }

    /// Every requirement's predicate, evaluated.
    pub fn verdicts(&self) -> Vec<RequirementVerdict> {
        let mut out = Vec::new();
        for root in self.roots {
            collect_verdicts(root, self, &mut out);
        }
        out
    }

    /// A connection's two ends must name features that exist.
    fn check_connections(&self, out: &mut Vec<Issue>) {
        for root in self.roots {
            walk_connections(root, self, out);
        }
        for root in self.roots {
            walk_declared_ends(root, self, out);
        }
    }

    fn check_connection(&self, el: &Element, out: &mut Vec<Issue>) {
        let what = if el.kind == Kind::Bind {
            "binding"
        } else if el.kind == Kind::FlowUsage {
            "flow"
        } else {
            "connection"
        };
        // A flow end written with `::` where `.` nests into a feature is not identified, so the flow
        // relates nothing through it. The reference states five findings for that mistake and the
        // pilot asserts all of them (`Relationship_invalid_relatedElement1.sysml.xt:47-54`). Per end,
        // the end cannot be identified (`validateFlowEndSubsetting`, `KerMLValidator.xtend:355`),
        // reported here, and the related feature is not accessible (`validateConnectorTypeFeaturing`,
        // `:271`), reported with the other ends below. Once, the flow relates fewer than two
        // identified features (`validateConnectorRelatedFeatures`, `:269`). The property check counts
        // the ends a connector writes, which for a flow is not the number that resolves, so the
        // identified count is here.
        let mut written = 0usize;
        let mut not_identified = 0usize;
        for end in [&el.from, &el.to].into_iter().flatten() {
            if end.trim().is_empty() || end.starts_with('~') {
                continue;
            }
            written += 1;
            if el.kind == Kind::FlowUsage
                && matches!(self.resolve_endpoint(el, end), Endpoint::NotAccessible)
            {
                not_identified += 1;
                out.push(Issue {
                    code: "REF001",
                    severity: Severity::Error,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: format!(
                        "Cannot identify flow end (use dot notation): the related feature `{end}` \
                         is not accessible from the flow's context (validateFlowEndSubsetting)"
                    ),
                });
            }
        }
        if not_identified > 0 && written - not_identified < 2 {
            out.push(Issue {
                code: "SEM001",
                severity: Severity::Error,
                at: self.index.qualified[el.id].clone(),
                line: el.line,
                message: format!(
                    "{what} `{}` relates {} feature(s) once its unidentified ends are left out; a \
                     connector must relate at least two (validateConnectorRelatedFeatures)",
                    self.index.qualified[el.id],
                    written - not_identified
                ),
            });
        }
        // The ends after the second of an n-ary connection are resolved like the other two. A
        // binding's extra ends are judged by the binding rule, so they are left to it.
        let extra: &[String] = if el.kind == Kind::Connection {
            &el.extra_ends
        } else {
            &[]
        };
        for end in [&el.to, &el.from].into_iter().flatten().chain(extra.iter()) {
            if end.trim().is_empty() || end.starts_with('~') {
                // `connect ~a to b` negates; the name after the tilde is checked like any other.
                continue;
            }
            // A binding's ends go through the same walk, but they are not connector ends: the pilot
            // leaves `bind a.p = …` alone where `a.p` does not resolve
            // (`valid_ActionUsage.sysml.xt:59`), so only the notation verdict is reported here.
            let verdict = self.resolve_endpoint(el, end);
            let verdict =
                if matches!(el.kind, Kind::Bind) && !matches!(verdict, Endpoint::NotAccessible) {
                    Endpoint::Found
                } else {
                    verdict
                };
            match verdict {
                Endpoint::Found => {}
                // validateConnectorRelatedFeatures
                Endpoint::NotAFeature(found) => out.push(Issue {
                    code: "SEM002",
                    severity: Severity::Warning,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: format!(
                        "{what} end `{end}` resolves to `{found}`, which is not a port or part"
                    ),
                }),
                // validateConnectorRelatedFeatures
                Endpoint::Unresolved => out.push(Issue {
                    code: "SEM001",
                    severity: Severity::Error,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: format!("{what} end `{end}` names nothing in this model"),
                }),
                Endpoint::NotAccessible => out.push(Issue {
                    code: "REF001",
                    severity: Severity::Error,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: format!(
                        "Must be an accessible feature (use dot notation for nesting): {what} end \
                         `{end}` reaches a member of a feature with `::` \
                         (validateConnectorTypeFeaturing)"
                    ),
                }),
                Endpoint::OutsideTheInstance(found) => out.push(Issue {
                    code: "REF001",
                    severity: Severity::Error,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: format!(
                        "Must be an accessible feature (use dot notation for nesting): {what} end \
                         `{end}` names `{found}`, which is outside the feature this {what} is \
                         written in (validateConnectorTypeFeaturing)"
                    ),
                }),
            }
        }
    }

    fn resolve_endpoint(&self, el: &Element, end: &str) -> Endpoint {
        let text = strip_multiplicity(end.trim());
        // `[1] cd ::> b.c.d` — the end names a feature it references. What has to resolve is
        // the reference target: `cd` is the end's own name and is declared nowhere, which is
        // exactly why the model writes the subsetting. Only the part after `::>` is a path.
        // The reference target is looked up from the connection's *owner*, not from the
        // connection. A `::>` names something the end refers to, and an end's sibling is not
        // that: `end b ::> f[1]` inside a connector that itself declares `feature f` must not
        // resolve to the sibling (`ConnectorTest_ConnectorEndSubsettingBadCase.kerml.xt:32`
        // expects "Couldn't resolve reference to Feature 'f'").
        let scope = match text.split_once("::>") {
            Some((_, reference)) => {
                let reference = strip_multiplicity(reference.trim());
                if reference.is_empty() {
                    return Endpoint::Unresolved;
                }
                (self.index.parent.get(el.id).copied().flatten(), reference)
            }
            None => (Some(el.id), text),
        };
        let (scope, text) = scope;
        // A bare end name has to be a member of what encloses the connector. `connect x to z;`
        // inside `part y : A { … }` finds `z` by walking outward to the sibling declared in the
        // enclosing part def, and an end that reaches out of its instance is not a feature of it
        // (`Connector_Invalid.sysml.xt:50`).
        if !text.contains('.') && !text.contains("::") {
            if let Some(owner) = self.index.parent.get(el.id).copied().flatten() {
                // A definition is a type: `part def E { connect a to b; }` joins two parts of the
                // package and the pilot accepts it. The rule is about instances.
                if !self.index.kind[owner].is_definition()
                    && self.index.member_of_access(owner, text).found().is_none()
                {
                    if let Resolution::Found(found) = self.index.resolve(owner, text) {
                        // Only a part is out of reach: a requirement or a behaviour is not a
                        // feature of the instance, so referencing one from outside is how every
                        // `satisfy`-style model is written (`14c-Language-Extensions` connects
                        // to a package-level `req1`).
                        if matches!(
                            self.index.kind[found],
                            sysml_model::Kind::PartUsage
                                | sysml_model::Kind::PortUsage
                                | sysml_model::Kind::ItemUsage
                                | sysml_model::Kind::OccurrenceUsage
                        ) {
                            return Endpoint::OutsideTheInstance(
                                self.index.qualified[found].clone(),
                            );
                        }
                    }
                }
            }
        }
        if text.contains("::") {
            if let Some(scope) = scope {
                if self.index.end_is_unreachable(scope, text) {
                    return Endpoint::NotAccessible;
                }
            }
        }
        let scope = scope.unwrap_or(el.id);
        if text.is_empty() {
            return Endpoint::Unresolved;
        }
        if !text.contains('.') {
            return match self.index.resolve(scope, text) {
                Resolution::Found(id) => self.endpoint_kind(id),
                _ => Endpoint::Unresolved,
            };
        }
        let mut segments = text
            .split('.')
            .map(strip_multiplicity)
            .filter(|s| !s.is_empty());
        let Some(head) = segments.next() else {
            return Endpoint::Unresolved;
        };
        let mut current = match self.index.resolve(scope, head) {
            Resolution::Found(id) => id,
            _ => return Endpoint::Unresolved,
        };
        for segment in segments {
            // Resolve each step through the same path walk a predicate uses, so inheritance and
            // specialization are honoured: `rearAxle.leftHalfAxle.axleToWheelPort` names a part
            // of a part, and the port may be declared on either one's type.
            // Each segment is looked up *inside* the previous one, so inheritance and
            // specialization are honoured: `leftHalfAxle` is a part of `rearAxle`, and the port
            // may be declared on either one's type.
            let matches: Vec<usize> = match self.index.member_for_access(current, segment) {
                Resolution::Found(id) => vec![id],
                Resolution::Ambiguous(ids) => ids,
                // A `.` step is never the notation error `NotAccessible` describes, and
                // `member_for_access` cannot produce it.
                Resolution::Unresolved | Resolution::NotAccessible => Vec::new(),
            };
            current = match matches.len() {
                1 => matches[0],
                _ => return Endpoint::Unresolved,
            };
        }
        self.endpoint_kind(current)
    }

    /// Whether an element can be an end of a connection.
    ///
    /// A connector joins features, and SysML lets it join far more than ports: the language
    /// extension models connect a requirement to the failure mode that motivates it, and an
    /// action to the item it consumes. Only a namespace — a package or a definition used as a
    /// container — cannot be an end, because it is not a feature of anything.
    fn endpoint_kind(&self, id: usize) -> Endpoint {
        match self.index.kind[id] {
            Kind::Package | Kind::Import | Kind::Unsupported => {
                Endpoint::NotAFeature(self.index.qualified[id].clone())
            }
            _ => Endpoint::Found,
        }
    }

    /// The definition an element is typed by, resolved from its authored type reference.
    fn type_target(&self, id: usize) -> Option<usize> {
        let el = self.flat.get(id).copied().flatten()?;
        let text = el.type_ref.as_deref()?;
        match self.index.resolve(id, text) {
            Resolution::Found(target) if target != id => Some(target),
            _ => None,
        }
    }

    /// A feature's value clause must be computable: its names have to exist.
    fn check_values(&self, out: &mut Vec<Issue>) {
        for root in self.roots {
            walk_values(root, self, out);
        }
    }

    fn check_value(&self, el: &Element, out: &mut Vec<Issue>) {
        let Some(expr) = &el.value_expr else {
            return;
        };
        for (name, problem) in self.name_statuses(el.id, expr) {
            if problem != NameProblem::Ok {
                out.push(Issue {
                    code: "SEM003",
                    severity: Severity::Error,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: unreadable("value expression", &name, problem),
                });
            }
        }
        // SEM122 — validateInstantiationExpressionInstantiatedType. An instantiation expression has
        // an instantiated type only when the name it reads names something. The name is already
        // reported as an unreadable value expression above; this rule reports the missing type on its
        // own, at the same place, as the reference does (`KerMLValidator.xtend:1164-1166`).
        for name in expr.instantiated_refs() {
            if is_builtin(&name.text) || unit_def(&name.text).is_some() {
                continue;
            }
            if matches!(
                self.index.resolve_path(el.id, name),
                Resolution::Unresolved | Resolution::Ambiguous(_)
            ) {
                out.push(Issue {
                    code: "SEM122",
                    severity: Severity::Error,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: format!(
                        "must have an invoked or instantiated type, and `{}` names nothing",
                        name.text
                    ),
                });
            }
        }
        // A member read from a value — `new D().b`, `(x as T).b` — is not a name in this scope, so
        // the loop above cannot check it: `refs()` reports only the base. The member has to be
        // looked for in the base's type, and reporting it here is what closes the hole that let
        // `(instance1 as D).zzzNoSuchMember` pass: the cast form used to be lowered to its operand
        // with the member dropped, so nothing ever asked whether the member existed.
        for (base, member) in expr.member_accesses() {
            // The base's type is what the member is looked for in, and it is the *base* that has
            // that type — not the whole `base.member` expression, whose type is the member's and so
            // cannot be asked for before the member is found.
            //
            // `typing::type_of` answers the base for both forms that reach here: `new D()` is a `D`,
            // and a cast `(x as T)` is a `T`. A base that is a plain name never reaches this loop:
            // `lower.rs` keeps it as a dotted `Ref`, because a member of a *name* is resolved in that
            // name's scope, which is a question the path walk already answers.
            let Some(root) = self.exprs.root_of(el.id, crate::expr::ExprField::Value) else {
                continue;
            };
            let Some(base_root) = self.exprs.find_base(root, member) else {
                continue;
            };
            // A base whose type this layer cannot settle is left alone. Reporting it was tried and
            // rejected: it turned `KernelLibraryTest.sysml` — a model the pilot states clean — into
            // five errors, because the library types a member's owner through a construct this layer
            // does not resolve. An unknown type is a limit of this kernel, and saying "the member is
            // missing" about it would be a claim the kernel cannot support.
            let Some(base_type) = crate::typing::type_of(&self.index, &self.exprs, base_root)
            else {
                continue;
            };
            let _ = base;
            let _ = base;
            if crate::expr::resolve_member(&self.index, base_type, member).is_none() {
                out.push(Issue {
                    code: "SEM118",
                    severity: Severity::Error,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: format!(
                        "`{member}` is not a member of the type this value reads from ({})",
                        self.index.qualified[base_type]
                    ),
                });
            }
        }
        // A value that nothing asks about is still a claim, and a claim whose operands cannot be
        // combined is wrong whatever the numbers are. Until this existed the check only reached a
        // value through a requirement's predicate, so `attribute sum = mass + alt` over a `kg` and
        // an `m` sat in a clean model as long as no requirement happened to read `sum`.
        //
        // A warning, not an error: the specification does not forbid the model, and this is a
        // product check on what its quantities mean.
        if let Eval::Unknown(why) = self.eval(el.id, expr, 0) {
            // `eval` reports a unit conflict the same way it reports a name it cannot resolve,
            // so the message decides. Only a message that names the problem is promoted: a value
            // written over an unresolved name is `SEM003` above and must not be reported twice.
            if why.contains("not convertible here") || why.contains("one side has no unit") {
                out.push(Issue {
                    code: "UNIT001",
                    severity: Severity::Warning,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: format!("value cannot be computed: {why}"),
                });
            }
        }
    }

    fn check_predicates(&self, out: &mut Vec<Issue>) {
        for root in self.roots {
            walk_predicates(root, self, out);
        }
    }

    /// An `assume` clause is a different claim from a `require` clause, and is reported as one.
    ///
    /// A requirement that does not hold says the design fails. An assumption that does not hold
    /// says the model's basis is wrong, so nothing derived from it can be trusted — a different
    /// message for a different person to act on. It is also not a SEM007 placeholder case: an
    /// assumption is often a bound on the environment rather than a model feature, so a
    /// literal-only assumption is legitimate and gets no placeholder complaint.
    fn check_assumption(&self, el: &Element, out: &mut Vec<Issue>) {
        let Some(assume) = &el.assumptions else {
            return;
        };
        for (name, problem) in self.name_statuses(el.id, assume) {
            if problem != NameProblem::Ok {
                out.push(Issue {
                    code: "SEM010",
                    severity: Severity::Error,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: unreadable("assumption", &name, problem),
                });
            }
        }
        match self.evaluate(el.id, assume) {
            Verdict::Fail(v) => out.push(Issue {
                code: "SEM011",
                severity: Severity::Error,
                at: self.index.qualified[el.id].clone(),
                line: el.line,
                message: format!(
                    "assumption does not hold (evaluates {}); anything judged against it rests on a false premise",
                    v.render()
                ),
            }),
            Verdict::Inconclusive(why) => out.push(Issue {
                code: "SEM012",
                severity: Severity::Warning,
                at: self.index.qualified[el.id].clone(),
                line: el.line,
                message: format!("assumption cannot be decided: {why}"),
            }),
            _ => {}
        }
    }

    fn check_predicate(&self, el: &Element, out: &mut Vec<Issue>) {
        let Some(predicate) = &el.predicate else {
            // A requirement with no predicate at all is not a failure to evaluate; it is a
            // requirement that states nothing checkable. Reported elsewhere, by the ledger.
            return;
        };
        let statuses = self.name_statuses(el.id, predicate);
        for (name, problem) in &statuses {
            if *problem != NameProblem::Ok {
                out.push(Issue {
                    code: "SEM004",
                    severity: Severity::Error,
                    at: self.index.qualified[el.id].clone(),
                    line: el.line,
                    message: unreadable("predicate", name, *problem),
                });
            }
        }
        let model_refs = statuses.len();
        // A predicate whose text is a string literal is already known not to be a Boolean, and the
        // rule that owns that judgement has spoken: `transition … if "test"` is reported as "must be
        // a Boolean expression" by `validateTransitionFeatureMembershipGuardExpression`
        // (`rules/transition_trigger.rs`). Evaluating it anyway produced a second diagnostic —
        // "constraint cannot be decided: string values are not evaluated" — at the transition's own
        // line, which the suite says nothing about. One wrong guard, two reports, one of them at a
        // line with no assertion.
        //
        // Checked on the literal rather than through `ExprTree::boolean_result` because that tree is
        // built locally in `check` and is not reachable here; a string literal is exactly the shape
        // `guard_is_boolean` reports, so the two agree on what "known not to be a Boolean" means.
        if matches!(predicate, Expr::Str(_)) {
            return;
        }
        let verdict = self.evaluate(el.id, predicate);
        match &verdict {
            Verdict::Fail(v) => out.push(Issue {
                code: "SEM005",
                severity: Severity::Error,
                at: self.index.qualified[el.id].clone(),
                line: el.line,
                message: format!("constraint does not hold: {}", v.render()),
            }),
            Verdict::Inconclusive(why) => out.push(Issue {
                code: "SEM006",
                severity: Severity::Warning,
                at: self.index.qualified[el.id].clone(),
                line: el.line,
                message: format!("constraint cannot be decided: {why}"),
            }),
            _ => {}
        }
        // A predicate built only from literals evaluates without reading the model. It is
        // syntactically a requirement and semantically a placeholder: `{1 > 0}` says nothing
        // about the system, and a green light on it is the most misleading result the tool can
        // produce.
        if model_refs == 0 && matches!(verdict, Verdict::Pass(_)) {
            // An error, not a warning. This started as a warning on the reasoning that a
            // literal-only constraint is not necessarily wrong. In the context this tool is
            // built for it always is: an AI that writes `{ 1 > 0 }` has not stated a
            // requirement, it has produced something that will report green forever. The
            // severity is the whole difference between noticing that and not.
            out.push(Issue {
                code: "SEM007",
                severity: Severity::Error,
                at: self.index.qualified[el.id].clone(),
                line: el.line,
                message: "constraint reads no model element and is trivially true; this is a                           placeholder, not a requirement"
                    .to_string(),
            });
        }
        if let Some(why) = self.literals_compared_with_units(predicate) {
            out.push(Issue {
                code: "SEM008",
                severity: Severity::Warning,
                at: self.index.qualified[el.id].clone(),
                line: el.line,
                message: why,
            });
        }
    }

    /// Names in an expression, with whether each resolved.
    fn name_statuses(&self, from: usize, expr: &Expr) -> Vec<(String, NameProblem)> {
        expr.refs()
            .into_iter()
            .map(|name| {
                // A call to a language function is not a reference to a model element, and
                // reporting it as one sends an author hunting for a declaration that should
                // not exist.
                // A unit spelling is not a model element. `= N / mm^2` writes a derived unit as
                // arithmetic over unit symbols; `N` and `mm` are the units, not features the
                // model forgot to declare. Reporting them as unresolved names was a false
                // positive on every model that defined a unit by composition.
                // `f::a` names a member of what `f` is, and the path resolves — `exports_of`
                // follows the type — so the name is present and only the operator is wrong.
                //
                // This runs before the unit escape below, which reads a qualified spelling by its
                // last segment: `SI::m` is the metre, but `v1::m` is `v1`'s `m` and was being
                // excused as a unit because the segment is also one.
                if name.text.contains("::") && self.index.crosses_a_feature(from, &name.text) {
                    return (name.text.clone(), NameProblem::NotAccessible);
                }
                if is_builtin(&name.text) || unit_def(&name.text).is_some() {
                    return (name.text.clone(), NameProblem::Ok);
                }
                let problem = match self.index.resolve_path(from, name) {
                    Resolution::Unresolved | Resolution::Ambiguous(_) => NameProblem::Missing,
                    _ => NameProblem::Ok,
                };
                (name.text.clone(), problem)
            })
            .collect()
    }

    fn evaluate(&self, from: usize, expr: &Expr) -> Verdict {
        match self.eval(from, expr, 0) {
            Eval::Val(Value::Bool(true), _) => Verdict::Pass(Value::Bool(true)),
            Eval::Val(Value::Bool(false), _) => Verdict::Fail(Value::Bool(false)),
            Eval::Val(other, _) => Verdict::Inconclusive(format!(
                "expression yields {} rather than a truth value",
                other.render()
            )),
            Eval::Unknown(why) => Verdict::Inconclusive(why),
            Eval::Err(why) => Verdict::Error(why),
        }
    }

    fn eval(&self, from: usize, expr: &Expr, depth: usize) -> Eval {
        if depth > 32 {
            return Eval::Unknown("expression nests too deeply or cycles".to_string());
        }

        match expr {
            Expr::Int(v) => Eval::Val(Value::Int(*v), None),
            Expr::Real(text) => match text.parse::<f64>() {
                Ok(v) => Eval::Val(Value::Real(v), None),
                Err(_) => Eval::Err(format!("`{text}` is not a number")),
            },
            Expr::Bool(v) => Eval::Val(Value::Bool(*v), None),
            // `null` is a value the grammar states, and it is not a number, a string or a truth
            // value: a constraint built on it has no verdict. Saying that is more useful than
            // the empty-`Opaque` form's "expression form is not evaluated", which claimed the
            // front end could not read the construct at all.
            Expr::Null => Eval::Unknown("`null` has no value here".to_string()),
            Expr::Str(_) => Eval::Unknown("string values are not evaluated".to_string()),
            Expr::Ref(name) => self.eval_ref(from, &name.text, depth),
            // Reading a feature of a constructed or cast value needs a value for the base, and this
            // layer has no object model to build one from: `D().b` names a feature of `D`, but
            // nothing here represents the instance that would hold it. Saying so is the honest
            // answer — the alternative is to resolve the member in the enclosing scope, which is
            // how `D().b` came to be reported as naming nothing.
            Expr::Member { base, member } => match self.eval(from, base, depth + 1) {
                Eval::Err(why) => Eval::Err(why),
                _ => Eval::Unknown(format!(
                    "`{member}` is read from a value this layer does not construct"
                )),
            },
            Expr::Unary { op, operand } => match self.eval(from, operand, depth + 1) {
                Eval::Val(v, unit) => match (op, v) {
                    (UnaryOp::Minus, Value::Int(i)) => Eval::Val(Value::Int(-i), None),
                    (UnaryOp::Minus, Value::Real(r)) => Eval::Val(Value::Real(-r), unit),
                    (UnaryOp::Plus, v) => Eval::Val(v, unit),
                    (UnaryOp::Not, Value::Bool(b)) => Eval::Val(Value::Bool(!b), None),
                    (op, v) => {
                        Eval::Err(format!("`{}` cannot apply to {}", op.symbol(), v.render()))
                    }
                },
                other => other,
            },
            Expr::Binary { op, left, right } => {
                // A unit bracket is not arithmetic: `20 [min]` is the value 20 carrying a unit.
                // The unit is recorded by the caller, not honoured in arithmetic, because this
                // layer has no dimension algebra and pretending otherwise would be worse than
                // saying so.
                if *op == BinaryOp::Unit {
                    // `20 [min]` means twenty minutes, not the bare number 20 tagged `min`.
                    // The value is converted into the unit's base at the point it is read, so
                    // every later operation works in one scale and `360 [W*h] / 900 [W]` comes
                    // out as seconds rather than as a number that still needs scaling.
                    return match self.eval(from, left, depth + 1) {
                        Eval::Val(v, _) => {
                            let spelling = right.render_unit();
                            match parse_unit_text(&spelling) {
                                Some(unit) => {
                                    // The value is now in base units, so the spelling it carries
                                    // must be one whose scale is 1. Carrying the authored spelling
                                    // instead made the comparison apply the factor a second time:
                                    // `60 [km/h]` became 16.67 base units still labelled `km/h`,
                                    // and comparing it multiplied by 0.2778 again.
                                    let carried = unit.spelling();
                                    match v.as_real() {
                                        Some(n) => Eval::Val(
                                            Value::Real(n * unit.scale + unit.offset),
                                            Some(carried),
                                        ),
                                        None => Eval::Val(v, Some(carried)),
                                    }
                                }
                                // An unknown unit is not the same as no unit. Dropping the
                                // bracket made `10 [kN] >= 5000 [N]` a comparison of bare
                                // numbers — decided, wrong, and indistinguishable from a real
                                // result. The authored spelling is carried through instead, so
                                // the comparison below can see that two sides do not share a
                                // known dimension and answer undecided.
                                None => Eval::Val(v, Some(spelling.clone())),
                            }
                        }
                        other => other,
                    };
                }
                let l = self.eval(from, left, depth + 1);
                let r = self.eval(from, right, depth + 1);
                apply_binary(*op, l, r)
            }
            Expr::Sequence(items) => match items.first() {
                Some(first) => self.eval(from, first, depth + 1),
                None => Eval::Unknown("empty expression".to_string()),
            },
            Expr::Call {
                callee,
                args,
                arg_names,
            } => self.eval_call(from, callee, args, arg_names, depth),
            Expr::Chain { op, .. } => {
                Eval::Unknown(format!("sequence operation `->{op}` is not evaluated"))
            }
            Expr::Constructor { type_ref, .. } => Eval::Unknown(format!(
                "construction of `{}` is not evaluated",
                type_ref.text
            )),
            // `a ? b : c` and the `if a then b else c` spelling lower to the same node. The
            // condition decides which branch is evaluated, so only that branch is read: a
            // model may legitimately write `if x > 0 then y else z` where one branch is
            // undefined for the current inputs, and reporting the whole thing undecided would
            // hide an answer that is actually determined.
            Expr::Conditional {
                test,
                then_expr,
                else_expr,
            } => match self.eval(from, test, depth + 1) {
                Eval::Val(v, _) => match v.as_bool() {
                    Some(true) => self.eval(from, then_expr, depth + 1),
                    Some(false) => self.eval(from, else_expr, depth + 1),
                    None => Eval::Err(format!(
                        "`if` needs a truth to choose a branch, got {}",
                        v.render()
                    )),
                },
                Eval::Unknown(why) => Eval::Unknown(why),
                Eval::Err(e) => Eval::Err(e),
            },
            // `x istype T` asks whether x's type is T or specializes T. Both the operand and the
            // named type resolve through the same scope walk the rest of the checker uses, and
            // the answer comes from the specialization closure — no extra machinery needed.
            Expr::TypeCheck {
                kind,
                operand,
                type_name,
            } => {
                // A metadata cast or test reads the operand's metaobject, which this evaluator has
                // no model of. Answering from the operand's declared type instead would be a
                // different question.
                if matches!(
                    kind,
                    sysml_model::TypeCheckKind::Meta | sysml_model::TypeCheckKind::MetaTest
                ) {
                    return Eval::Unknown(format!(
                        "`{}` reads metadata, which is not evaluated",
                        expr.render()
                    ));
                }
                let Some(target) = self.index.resolve_path(from, type_name).found() else {
                    return Eval::Err(format!(
                        "`{}` names nothing, so the type test cannot be answered",
                        type_name.text
                    ));
                };
                // The operand is a feature; its declared type is what participates in the test.
                let operand_id = match &**operand {
                    Expr::Ref(name) => self.index.resolve_path(from, name).found(),
                    _ => None,
                };
                let Some(operand_id) = operand_id else {
                    return Eval::Unknown(
                        "the type test's operand is not a resolvable feature".to_string(),
                    );
                };
                let Some(operand_type) = self.type_of(operand_id) else {
                    return Eval::Unknown(format!(
                        "`{}` has no declared type, so `{}` cannot be answered",
                        operand.render(),
                        type_name.text
                    ));
                };
                let holds = match kind {
                    sysml_model::TypeCheckKind::IsType => {
                        if self.is_subtype(operand_type, target) {
                            true
                        } else if self.is_subtype(target, operand_type) {
                            // The declared type is a *supertype* of the tested one, so a subtype
                            // instance is possible and the model does not say which one this is.
                            // `part engine : Engine[1]` tested with `engine istype
                            // '4CylEngine'` is undecided, not false; answering false made a
                            // two-branch disequality assert fail on a model that never chose.
                            return Eval::Unknown(format!(
                                "`{}` is declared as `{}`, which does not settle whether it is `{}`",
                                operand.render(),
                                self.index.qualified[operand_type],
                                type_name.text
                            ));
                        } else {
                            false
                        }
                    }
                    // `hastype` asks the same question of the actual types; with no runtime
                    // instance model, the declared type is the only evidence there is, and a
                    // model that declares the type satisfies it.
                    sysml_model::TypeCheckKind::HasType => {
                        if operand_type == target {
                            true
                        } else if self.is_subtype(target, operand_type) {
                            return Eval::Unknown(format!(
                                "`{}` is declared as `{}`, which does not settle whether it is `{}`",
                                operand.render(),
                                self.index.qualified[operand_type],
                                type_name.text
                            ));
                        } else {
                            false
                        }
                    }
                    // `as` is a cast: it yields the operand unchanged, read as the named type. It
                    // is not a truth value and must not be evaluated as one — that is what folding
                    // it into `IsType` did, and it made every `(x as T)` value a Boolean.
                    // Returned before the target was resolved; kept so the match stays exhaustive.
                    sysml_model::TypeCheckKind::Meta | sysml_model::TypeCheckKind::MetaTest => {
                        return Eval::Unknown(format!(
                            "`{}` reads metadata, which is not evaluated",
                            expr.render()
                        ));
                    }
                    sysml_model::TypeCheckKind::As => {
                        return match self.eval(from, operand, depth + 1) {
                            // The value passes through a cast, so an evaluable operand stays
                            // evaluable and the cast changes only how the member above it is read.
                            other => other,
                        };
                    }
                };
                Eval::Val(Value::Bool(holds), None)
            }
            // A lambda has no value until it is applied, and this layer does not apply one.
            Expr::Lambda { .. } => Eval::Unknown("a lambda body is not evaluated here".to_string()),
            Expr::Opaque(_) => Eval::Unknown("expression form is not evaluated".to_string()),
        }
    }

    /// The language's own sequence functions, evaluated rather than resolved.
    ///
    /// `sum`, `size`, `abs`, `min`, `max`, `product`, `isEmpty`, `notEmpty` are not model
    /// elements, so the resolver finds nothing and each call used to come back "names no
    /// calculation". `10a-Analysis` writes `mass = sum((vehicle.engine.mass, ...))` and got no
    /// number from it.
    ///
    /// The sequence is the parenthesised list, which lowers to `Expr::Sequence`; `min(a, b)` and
    /// `min((a, b))` state the same thing, so both shapes are read. A unit survives `sum` and
    /// `abs` when every value carries the same one — a sum over kilograms is kilograms — and is
    /// dropped when they differ, because this layer has no unit algebra to combine them.
    fn eval_builtin(&self, from: usize, name: &str, args: &[Expr], depth: usize) -> Option<Eval> {
        let leaf = name.rsplit("::").next().unwrap_or(name);
        let leaf = leaf.rsplit('.').next().unwrap_or(leaf);
        if !matches!(
            leaf,
            "sum" | "size" | "count" | "abs" | "min" | "max" | "product" | "isEmpty" | "notEmpty"
        ) {
            return None;
        }
        let items: Vec<&Expr> = if args.len() == 1 {
            match &args[0] {
                Expr::Sequence(items) => items.iter().collect(),
                other => vec![other],
            }
        } else {
            args.iter().collect()
        };
        let mut values: Vec<(Value, Option<String>)> = Vec::new();
        for item in items {
            match self.eval(from, item, depth + 1) {
                Eval::Val(v, unit) => values.push((v, unit)),
                other => return Some(other),
            }
        }
        Some(match leaf {
            "size" | "count" => Eval::Val(Value::Int(values.len() as i64), None),
            "isEmpty" => Eval::Val(Value::Bool(values.is_empty()), None),
            "notEmpty" => Eval::Val(Value::Bool(!values.is_empty()), None),
            "abs" => match values.first() {
                Some((v, unit)) => match v.as_real() {
                    Some(r) => {
                        let abs = if let Value::Int(i) = v {
                            Value::Int(i.abs())
                        } else {
                            Value::Real(r.abs())
                        };
                        Eval::Val(abs, unit.clone())
                    }
                    None => Eval::Err(format!("`abs` needs a number, got {}", v.render())),
                },
                None => Eval::Unknown("`abs` needs one value".to_string()),
            },
            "min" | "max" => {
                let mut best: Option<(Value, Option<String>)> = None;
                for (v, unit) in values {
                    let Some(r) = v.as_real() else {
                        return Some(Eval::Err(format!(
                            "`{leaf}` needs numbers, got {}",
                            v.render()
                        )));
                    };
                    let take = match &best {
                        None => true,
                        Some((current, _)) => match current.as_real() {
                            Some(c) if leaf == "min" => r < c,
                            Some(c) => r > c,
                            None => true,
                        },
                    };
                    if take {
                        best = Some((v, unit));
                    }
                }
                match best {
                    Some((v, unit)) => Eval::Val(v, unit),
                    None => Eval::Unknown(format!("`{leaf}` needs at least one value")),
                }
            }
            "sum" | "product" => {
                if values.is_empty() {
                    return Some(Eval::Val(
                        Value::Int(if leaf == "sum" { 0 } else { 1 }),
                        None,
                    ));
                }
                let mut unit: Option<String> = None;
                let mut real = false;
                let mut acc_int: i64 = if leaf == "sum" { 0 } else { 1 };
                let mut acc_real: f64 = if leaf == "sum" { 0.0 } else { 1.0 };
                for (v, u) in &values {
                    let Some(r) = v.as_real() else {
                        return Some(Eval::Err(format!(
                            "`{leaf}` needs numbers, got {}",
                            v.render()
                        )));
                    };
                    if matches!(v, Value::Real(_)) {
                        real = true;
                    }
                    match (&unit, u) {
                        (None, Some(next)) => unit = Some(next.clone()),
                        (Some(held), Some(next)) if held != next => {
                            return Some(Eval::Unknown(format!(
                                "`{leaf}` mixes `{held}` with `{next}`"
                            )))
                        }
                        _ => {}
                    }
                    if let Value::Int(i) = v {
                        acc_int = if leaf == "sum" {
                            acc_int + i
                        } else {
                            acc_int * i
                        };
                    }
                    acc_real = if leaf == "sum" {
                        acc_real + r
                    } else {
                        acc_real * r
                    };
                }
                // A product of two lengths has a unit this layer cannot name, so the unit is
                // dropped rather than carried wrong.
                let unit = if leaf == "product" { None } else { unit };
                let value = if real {
                    Value::Real(acc_real)
                } else {
                    Value::Int(acc_int)
                };
                Eval::Val(value, unit)
            }
            _ => unreachable!("the match above lists every name this branch accepts"),
        })
    }

    /// A `calc` invocation: bind the arguments to the parameters and evaluate the result.
    ///
    /// `Area(2.0, 3.0)` with `calc def Area { in w; in h; return a = w * h; }` is 6.0. Three
    /// decisions are worth their reasons:
    ///
    ///   * every argument is evaluated *before* anything is bound, so an argument that reads a name
    ///     one of the parameters also uses sees the caller's;
    ///   * the body is read with `from` set to the calculation, not the caller, so a name the body
    ///     uses resolves where it was written;
    ///   * an argument this layer cannot compute leaves the call undecided rather than substituting
    ///     a zero. A wrong number is worse than no number.
    fn eval_call(
        &self,
        from: usize,
        callee: &Expr,
        args: &[Expr],
        arg_names: &[Option<String>],
        depth: usize,
    ) -> Eval {
        let Expr::Ref(callee) = callee else {
            return Eval::Unknown("a call whose callee is not a name is not evaluated".to_string());
        };
        // The language's own sequence functions are not model elements: the resolver finds
        // nothing for `sum`, and every one of them used to come back "names no calculation".
        if let Some(result) = self.eval_builtin(from, &callee.text, args, depth) {
            return result;
        }
        let id = match self.index.resolve(from, &callee.text) {
            Resolution::Found(id) => id,
            _ => return Eval::Unknown(format!("`{}` names no calculation", callee.text)),
        };
        if self.index.kind[id] != Kind::CalcDef {
            return Eval::Unknown(format!(
                "`{}` is a {}, not a calculation",
                callee.text,
                self.index.kind[id].keyword()
            ));
        }
        let Some(calc) = self.flat.get(id).copied().flatten() else {
            return Eval::Unknown(format!(
                "`{}` names an element outside this model",
                callee.text
            ));
        };

        let mut values = Vec::new();
        for arg in args {
            match self.eval(from, arg, depth + 1) {
                Eval::Val(value, unit) => values.push((value, unit)),
                other => return other,
            }
        }

        // A parameter is every child that takes an input; the result is the one that writes an
        // output. `in calc calculation { … }` in the standard library is one of these, which is why
        // this reads children rather than a fixed argument list.
        let parameters: Vec<&Element> = calc
            .children
            .iter()
            .filter(|c| {
                c.kind == Kind::Parameter && c.direction != Some(sysml_model::Direction::Out)
            })
            .collect();
        let mut bound: Vec<(String, (Value, Option<String>))> = Vec::new();
        for (position, value) in values.into_iter().enumerate() {
            let named = arg_names.get(position).and_then(|n| n.as_deref());
            let target = match named {
                Some(written) => parameters.iter().find(|p| p.name == written),
                None => parameters.get(position),
            };
            let Some(parameter) = target else {
                return Eval::Unknown(format!(
                    "`{}` takes no argument in position {}",
                    callee.text,
                    position + 1
                ));
            };
            bound.push((parameter.name.clone(), value));
        }

        let body = calc
            .children
            .iter()
            .find(|c| c.direction == Some(sysml_model::Direction::Out))
            .and_then(|c| c.value_expr.as_ref())
            .or(calc.value_expr.as_ref());
        let Some(body) = body else {
            return Eval::Unknown(format!("calculation `{}` states no result", callee.text));
        };

        let mut saved: Vec<(String, Option<(Value, Option<String>)>)> = Vec::new();
        for (name, value) in bound {
            let previous = self.locals.borrow_mut().insert(name.clone(), value);
            saved.push((name, previous));
        }
        let outcome = self.eval(id, body, depth + 1);
        {
            let mut locals = self.locals.borrow_mut();
            for (name, previous) in saved.into_iter().rev() {
                match previous {
                    Some(value) => {
                        locals.insert(name, value);
                    }
                    None => {
                        locals.remove(&name);
                    }
                }
            }
        }
        outcome
    }

    /// Whether `expr` is true with `bindings` in scope, restoring the scope afterwards.
    fn holds(&self, from: usize, expr: &Expr, bindings: Vec<(String, Value)>) -> bool {
        let mut saved: Vec<(String, Option<(Value, Option<String>)>)> = Vec::new();
        for (name, value) in bindings {
            // A filter compares written numbers: `level >= 2` has no unit to carry.
            let previous = self.locals.borrow_mut().insert(name.clone(), (value, None));
            saved.push((name, previous));
        }
        let verdict = self.eval(from, expr, 0);
        {
            let mut locals = self.locals.borrow_mut();
            for (name, previous) in saved.into_iter().rev() {
                match previous {
                    Some(value) => {
                        locals.insert(name, value);
                    }
                    None => {
                        locals.remove(&name);
                    }
                }
            }
        }
        matches!(verdict, Eval::Val(Value::Bool(true), _))
    }

    fn eval_ref(&self, from: usize, text: &str, depth: usize) -> Eval {
        // A parameter shadows everything outside the calculation: `return a = w * h` must read the
        // argument, not a package member that happens to be called `w`.
        if !text.contains('.') && !text.contains("::") {
            if let Some((value, unit)) = self.locals.borrow().get(text) {
                // The unit travels with the value: a parameter bound to `10 [N]` still measures
                // newtons inside the body, which is what lets `force * speed` come out in watts.
                return Eval::Val(value.clone(), unit.clone());
            }
        }
        // `subject u : Ue = deployment;` binds the subject to one instance, and `u.cycle` in the
        // requirement's constraint has to read that instance's value rather than the type's
        // unset attribute. The binding is on the subject element; resolving the path through the
        // type is what the resolver is for, so it is rewired here, at the point where a value is
        // being asked for.
        let rewritten: std::borrow::Cow<str> = match text.split_once('.') {
            Some((head, rest)) if !head.is_empty() && !rest.is_empty() => {
                match self.index.resolve(from, head) {
                    Resolution::Found(head_id) => match self.flat.get(head_id).copied().flatten() {
                        Some(el) if el.kind == Kind::Subject => match &el.value_expr {
                            Some(Expr::Ref(name)) => {
                                std::borrow::Cow::Owned(format!("{}.{}", name.text, rest))
                            }
                            _ => std::borrow::Cow::Borrowed(text),
                        },
                        _ => std::borrow::Cow::Borrowed(text),
                    },
                    _ => std::borrow::Cow::Borrowed(text),
                }
            }
            _ => std::borrow::Cow::Borrowed(text),
        };
        let text = rewritten.as_ref();
        let name = sysml_model::NameRef::new(text, 0);
        let id = match self.index.resolve_path(from, &name) {
            Resolution::Found(id) => id,
            Resolution::Ambiguous(_) => return Eval::Err(format!("`{text}` is ambiguous")),
            Resolution::Unresolved => return Eval::Err(format!("`{text}` names nothing")),
            // `NotAccessible` is set on the reference a `::` path produces, never returned by
            // `resolve_path` itself.
            Resolution::NotAccessible => return Eval::Err(format!("`{text}` names nothing")),
        };
        let Some(el) = self.flat.get(id).copied().flatten() else {
            return Eval::Err(format!("`{text}` names an element outside this model"));
        };
        match el.kind {
            // A KerML `feature` carries a value the same way an attribute usage does, and the
            // multiplicity rules read `feature m1 = n - 1;` in KerML models.
            Kind::AttributeUsage | Kind::KermlFeature => match &el.value_expr {
                Some(expr) => self.eval(id, expr, depth + 1),
                // A usage that writes no value takes the one its type declares: `part wheels :
                // Wheel` reads `Wheel`'s `attribute mass = 10 [kg]`. A model that states its data
                // on the definition and its structure on the usage computes nothing without this.
                None => match self.value_from_type(id, depth) {
                    Some(inherited) => inherited,
                    None => Eval::Unknown(format!("`{text}` has no value")),
                },
            },
            Kind::EnumValue => Eval::Unknown(format!("`{text}` is an enumeration value")),
            _ => Eval::Unknown(format!("`{text}` is not a value-bearing feature")),
        }
    }

    /// The value a feature inherits from its type.
    ///
    /// `part wheels : Wheel` with `attribute mass = 10 [kg]` on `Wheel` reads 10 when
    /// `wheels.mass` is asked for. A model that states its data on the definition and its
    /// structure on the usage has no number otherwise, and `sum(...)` over such usages cannot
    /// answer.
    ///
    /// Only the member with the same name is read, and only when it writes a value. The type's own
    /// children are asked first and the specialization closure second: `inherited_member_of` walks
    /// supertypes only, so a value declared on the type itself would be missed by it.
    fn value_from_type(&self, id: usize, depth: usize) -> Option<Eval> {
        let name = self.flat.get(id).copied().flatten()?.name.clone();
        if name.is_empty() {
            return None;
        }
        let typing = self.type_of(id)?;
        let direct = self.index.children.get(typing).and_then(|children| {
            children
                .iter()
                .copied()
                .find(|&child| self.index.name[child] == name)
        });
        let member = direct.or_else(|| self.index.inherited_member_of(typing, &name))?;
        let expr = self
            .flat
            .get(member)
            .copied()
            .flatten()?
            .value_expr
            .clone()?;
        Some(self.eval(member, &expr, depth + 1))
    }

    /// A comparison between two literals that carry units is worth a look: the check here does
    /// not compare dimensions, so `1 [s] <= 1 [m]` would otherwise pass silently.
    fn literals_compared_with_units(&self, expr: &Expr) -> Option<String> {
        let Expr::Binary { op, left, right } = expr else {
            return None;
        };
        if !op.requires_same_dimension() {
            return None;
        }
        let (Some((_lv, lu)), Some((_rv, ru))) =
            (literal_with_unit(left), literal_with_unit(right))
        else {
            return None;
        };
        if lu.is_some() && ru.is_some() && lu != ru {
            return Some(format!(
                "compares a `{}` with a `{}`; dimensions are not checked here, so this comparison is unverified",
                lu.unwrap_or_default(),
                ru.unwrap_or_default()
            ));
        }
        None
    }
}

enum Endpoint {
    Found,
    NotAFeature(String),
    Unresolved,
    /// A `::` path that reaches its target through a feature. The name resolves; the operator is
    /// the wrong one.
    NotAccessible,
    /// A bare name that resolves by walking out of the instance the relationship is written in.
    OutsideTheInstance(String),
}

/// What a name written in an expression turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameProblem {
    /// It resolves, and the path uses the operator the notation asks for.
    Ok,
    /// Nothing by that name.
    Missing,
    /// It resolves, but the path reaches a feature with `::`.
    NotAccessible,
}

/// The sentence for a name that cannot be read, phrased for the clause it was written in.
fn unreadable(clause: &str, name: &str, problem: NameProblem) -> String {
    match problem {
        NameProblem::NotAccessible => format!(
            "{clause} reads `{name}`, which reaches a member of a feature with `::`; use `.` to \
             nest into a feature"
        ),
        _ => format!("{clause} reads `{name}`, which names nothing"),
    }
}

/// A computed value together with what is known about its unit.
///
/// This layer does not do dimension algebra, and pretending a division of `W*h` by `W` yields
/// something comparable to `min` is how a true requirement gets reported false. So a unit is
/// carried when the expression preserves it (literals, addition and subtraction of like units)
/// and dropped when it does not (multiplication, division, anything computed). A comparison
/// whose two sides do not share a known unit is `inconclusive`, never `pass` or `fail`.
#[derive(Debug)]
enum Eval {
    Val(Value, Option<String>),
    Unknown(String),
    Err(String),
}

fn apply_binary(op: BinaryOp, l: Eval, r: Eval) -> Eval {
    use BinaryOp::*;
    let (lv, lu, rv, ru) = match (l, r) {
        (Eval::Err(e), _) | (_, Eval::Err(e)) => return Eval::Err(e),
        (Eval::Unknown(a), _) => return Eval::Unknown(a),
        (_, Eval::Unknown(b)) => return Eval::Unknown(b),
        (Eval::Val(a, au), Eval::Val(b, bu)) => (a, au, b, bu),
    };
    match op {
        Add | Sub => {
            let (Some(a), Some(b)) = (lv.as_real(), rv.as_real()) else {
                return Eval::Err(format!(
                    "`{}` needs two numbers, got {} and {}",
                    op.symbol(),
                    lv.render(),
                    rv.render()
                ));
            };
            // Addition needs one dimension, not one spelling: `3 [m] + 200 [cm]` is a length,
            // and reporting it undecided because the units differ is a false negative.
            let (a, b, unit) = match (&lu, &ru) {
                (None, None) => (a, b, None),
                (Some(ua), Some(ub)) => match convert_pair(a, ua, b, ub) {
                    Some((ca, cb)) => (ca, cb, Some(ua.clone())),
                    None => {
                        return Eval::Unknown(format!(
                            "`{}` adds {} to {}; those units are not convertible here",
                            op.symbol(),
                            ua,
                            ub
                        ))
                    }
                },
                _ => {
                    return Eval::Unknown(format!(
                        "`{}` mixes {} with {}; one side has no unit",
                        op.symbol(),
                        lu.clone().unwrap_or_else(|| "a bare number".to_string()),
                        ru.clone().unwrap_or_else(|| "a bare number".to_string())
                    ))
                }
            };
            let v = if op == Add { a + b } else { a - b };
            // The result is reported in the first operand's unit. Converting it back would need
            // the inverse offset, and every caller either compares or prints — neither needs
            // the original spelling back.
            Eval::Val(Value::Real(v), unit)
        }
        Mul | Div | Rem | Pow => {
            let (Some(a), Some(b)) = (lv.as_real(), rv.as_real()) else {
                return Eval::Err(format!(
                    "`{}` needs two numbers, got {} and {}",
                    op.symbol(),
                    lv.render(),
                    rv.render()
                ));
            };
            let v = match op {
                Mul => a * b,
                Div => {
                    if b == 0.0 {
                        return Eval::Err("division by zero".to_string());
                    }
                    a / b
                }
                Rem => a % b,
                Pow => a.powf(b),
                _ => unreachable!(),
            };
            // Multiplication and division combine dimensions, so `[W*h] / [W]` comes out as a
            // time and can be compared with `[min]`. Without this an energy-over-power budget
            // is permanently undecided, which is a false negative on a requirement that is in
            // fact decidable.
            let unit = match (op, &lu, &ru) {
                (Mul, Some(a), Some(b)) | (Div, Some(a), Some(b)) => {
                    let left = parse_unit_text(a);
                    let right = parse_unit_text(b);
                    match (left, right) {
                        (Some(l), Some(r)) => {
                            let sign = if op == Mul { 1 } else { -1 };
                            Some(l.combine(r, sign).spelling())
                        }
                        _ => None,
                    }
                }
                (Mul | Div, _, _) => None,
                (_, _, _) => lu.clone(),
            };
            Eval::Val(Value::Real(v), unit)
        }
        Less | LessEq | Greater | GreaterEq => {
            let (Some(a), Some(b)) = (lv.as_real(), rv.as_real()) else {
                return Eval::Err(format!(
                    "`{}` compares numbers, got {} and {}",
                    op.symbol(),
                    lv.render(),
                    rv.render()
                ));
            };
            // A comparison is trustworthy when both sides share a known dimension. Identical
            // units used to be required, which left `0.4 h >= 20 min` undecided even though
            // every unit in it is known; now the two sides are converted to the dimension's
            // base unit and compared there. A pair outside the table stays undecided, because
            // a guessed conversion would be worse than no answer.
            let (a, b) = match (&lu, &ru) {
                (None, None) => (a, b),
                (Some(ua), Some(ub)) => match convert_pair(a, ua, b, ub) {
                    Some(pair) => pair,
                    None => {
                        return Eval::Unknown(format!(
                            "compares {} with {}; those units are not convertible here",
                            ua, ub
                        ))
                    }
                },
                _ => {
                    return Eval::Unknown(format!(
                        "compares {} with {}; one side has no unit",
                        lu.clone().unwrap_or_else(|| "a bare number".to_string()),
                        ru.clone().unwrap_or_else(|| "a bare number".to_string())
                    ))
                }
            };
            let v = match op {
                Less => a < b,
                LessEq => a <= b,
                Greater => a > b,
                GreaterEq => a >= b,
                _ => unreachable!(),
            };
            Eval::Val(Value::Bool(v), None)
        }
        Eq | NotEq => {
            let equal = match (&lv, &rv) {
                (Value::Bool(a), Value::Bool(b)) => a == b,
                _ => match (lv.as_real(), rv.as_real()) {
                    (Some(a), Some(b)) => {
                        // Equality of two quantities goes through the same dimension check a
                        // comparison does, and for the same reason: `1 [N] == 1 [V]` is not a
                        // question this layer can answer, and answering `true` because both
                        // sides happen to be the number 1 is the worst possible outcome.
                        let (a, b) = match (&lu, &ru) {
                            (None, None) => (a, b),
                            (Some(ua), Some(ub)) => match convert_pair(a, ua, b, ub) {
                                Some(pair) => pair,
                                // Two values written in the same unit are comparable even when
                                // this layer has no definition for it: `1 [furlong] == 1
                                // [furlong]` needs no conversion table.
                                None if ua == ub => (a, b),
                                None => {
                                    let shown = |u: &Option<String>| {
                                        u.clone().unwrap_or_else(|| "a bare number".to_string())
                                    };
                                    return Eval::Unknown(format!(
                                        "compares {} with {}; those units are not comparable here",
                                        shown(&lu),
                                        shown(&ru)
                                    ));
                                }
                            },
                            _ => {
                                let shown = |u: &Option<String>| {
                                    u.clone().unwrap_or_else(|| "a bare number".to_string())
                                };
                                return Eval::Unknown(format!(
                                    "compares {} with {}; one side has no unit",
                                    shown(&lu),
                                    shown(&ru)
                                ));
                            }
                        };
                        (a - b).abs() < 1e-12
                    }
                    _ => lv == rv,
                },
            };
            Eval::Val(Value::Bool(if op == Eq { equal } else { !equal }), None)
        }
        And | Or | Xor | Implies => {
            let (Some(a), Some(b)) = (lv.as_bool(), rv.as_bool()) else {
                return Eval::Err(format!(
                    "`{}` needs two truths, got {} and {}",
                    op.symbol(),
                    lv.render(),
                    rv.render()
                ));
            };
            let v = match op {
                And => a && b,
                Or => a || b,
                Xor => a ^ b,
                Implies => !a || b,
                _ => unreachable!(),
            };
            Eval::Val(Value::Bool(v), None)
        }
        _ => Eval::Unknown(format!("operator `{}` is not evaluated", op.symbol())),
    }
}

/// A compound unit: dimension name to exponent, plus the scale to the base.
///
/// `W*h` is power^1 * time^1; dividing by `W` leaves time^1, which is what makes
/// `360 [W*h] / 900 [W]` comparable with minutes. Only integer exponents are produced, and only
/// the dimensions in the table above take part.
#[derive(Debug, Clone, PartialEq)]
struct CompoundUnit {
    /// Exponents over the eight base quantities (L, M, T, I, temperature, amount, luminosity,
    /// information). A vector rather than a name because composed units have to reduce.
    dimensions: Vec<(usize, i32)>,
    /// Multiply a value in this unit by `scale` to reach the base unit of each dimension.
    scale: f64,
    /// Straight conversion to base units, in the first dimension's terms. Kept separate from
    /// `scale` because temperature offsets do not compose.
    offset: f64,
}

impl CompoundUnit {
    fn simple(unit: &str) -> Option<CompoundUnit> {
        let def = unit_def(unit)?;
        // `unit_def` already returns base-quantity exponents, so no expansion table is needed:
        // a derived unit and its `M*L^2*T^-2` form are literally the same vector.
        let dimensions: Vec<(usize, i32)> = def
            .dims
            .iter()
            .enumerate()
            .filter(|(_, e)| **e != 0)
            .map(|(base, e)| (base, *e))
            .collect();
        Some(CompoundUnit {
            dimensions,
            scale: def.factor,
            offset: def.offset,
        })
    }

    /// This unit raised to `power`: the exponents multiply, the scale is raised too.
    ///
    /// A scale that is not 1 cannot be raised without arithmetic that can overflow or lose
    /// precision quietly, so only a scale-1 unit may carry an exponent. `[km^2]` is therefore
    /// unknown rather than silently 1e6 — a gap in the table, not a wrong number.
    fn raised(self, power: i32) -> Option<CompoundUnit> {
        if power == 1 {
            return Some(self);
        }
        // A negative power is a division: `km^-1` is 1/1000 of a reciprocal metre, so the scale
        // has to invert rather than be rejected. Rejecting it made `km/h` unparseable — `km` has
        // a scale of 1000, `raised(-1)` refused, and the whole compound failed — which is how
        // `30 [m/s] <= 60 [km/h]` came back undecided after the lookup learned to reach the
        // compound reader.
        if self.offset != 0.0 {
            return None;
        }
        let dimensions = self
            .dimensions
            .iter()
            .map(|(base, exponent)| (*base, exponent * power))
            .collect();
        let scale = self.scale.powi(power);
        Some(CompoundUnit {
            dimensions,
            scale,
            offset: 0.0,
        })
    }

    fn combine(mut self, other: CompoundUnit, sign: i32) -> CompoundUnit {
        for (dim, exp) in other.dimensions {
            match self.dimensions.iter_mut().find(|(d, _)| *d == dim) {
                Some(slot) => slot.1 += exp * sign,
                None => self.dimensions.push((dim, exp * sign)),
            }
        }
        self.dimensions.retain(|(_, e)| *e != 0);
        self.dimensions.sort_by(|a, b| a.0.cmp(&b.0));
        if sign > 0 {
            self.scale *= other.scale;
            self.offset = 0.0;
        } else {
            self.scale /= other.scale;
            self.offset = 0.0;
        }
        self
    }

    fn is_known(&self) -> bool {
        !self.dimensions.is_empty() && self.dimensions.iter().all(|(_, e)| *e != 0)
    }

    /// Render back as a unit spelling whose *scale is 1* in this dimension.
    ///
    /// The evaluator converts a value into base units when it reads the bracket (`20 [min]` becomes
    /// 1200 carrying `s`), so the spelling it carries afterwards must denote the base, not the
    /// authored unit. It used to return the authored spelling whenever the dimension had no named
    /// SI unit — and then the comparison multiplied by the factor a second time. `30 [m/s]` hid
    /// that, because metres per second has factor 1; `60 [km/h]` was scaled twice and a true
    /// statement was reported false.
    ///
    /// Named units come first so a picture or a message says `J` rather than `m^2*kg/s^2`; the
    /// fallback covers every other dimension with base symbols, which is always a scale-1 spelling.
    /// A human-readable name for the dimension, independent of how it is spelled in units.
    ///
    /// `spelling()` answers "what base unit denotes this", which is what the evaluator needs to
    /// carry. A report answers a different question — "what kind of quantity is this" — and wants
    /// `mass` and `length/time`, not `kg` and `m/s`.
    fn spelling_name(&self) -> String {
        let names = [
            "length",
            "mass",
            "time",
            "current",
            "temperature",
            "amount",
            "intensity",
            "information",
        ];
        let mut above = Vec::new();
        let mut below = Vec::new();
        for (base, exponent) in &self.dimensions {
            let name = names.get(*base).copied().unwrap_or("unknown");
            let power = exponent.unsigned_abs();
            let piece = if power == 1 {
                name.to_string()
            } else {
                format!("{name}^{power}")
            };
            if *exponent > 0 {
                above.push(piece);
            } else {
                below.push(piece);
            }
        }
        if above.is_empty() && below.is_empty() {
            return "no dimension".to_string();
        }
        let top = if above.is_empty() {
            "1".to_string()
        } else {
            above.join("*")
        };
        if below.is_empty() {
            top
        } else {
            format!("{top}/{}", below.join("*"))
        }
    }

    fn spelling(&self) -> String {
        // Reduce a base-dimension vector to the named SI unit it corresponds to. Exponents are
        // indexed by base quantity (L, M, T, I, temperature, amount, luminosity, information),
        // so each arm is the vector itself rather than a spelling that has to be re-parsed.
        let mut dims: Vec<(usize, i32)> = self.dimensions.clone();
        dims.sort_by(|a, b| a.0.cmp(&b.0));
        let named = match dims.as_slice() {
            [(D_L, 1)] => Some("m"),
            [(D_M, 1)] => Some("kg"),
            [(D_T, 1)] => Some("s"),
            [(D_TEMP, 1)] => Some("K"),
            [(D_INFO, 1)] => Some("bit"),
            [(D_T, -1)] => Some("Hz"),
            [(D_N, 1)] => Some("mol"),
            [(D_J, 1)] => Some("cd"),
            [(D_T, -1), (D_INFO, 1)] => Some("bps"),
            [(D_L, 1), (D_M, 1), (D_T, -2)] => Some("N"),
            [(D_L, -1), (D_M, 1), (D_T, -2)] => Some("Pa"),
            [(D_L, 2), (D_M, 1), (D_T, -2)] => Some("J"),
            [(D_L, 2), (D_M, 1), (D_T, -3)] => Some("W"),
            [(D_T, 1), (D_I, 1)] => Some("C"),
            [(D_L, 2), (D_M, 1), (D_T, -3), (D_I, -1)] => Some("V"),
            _ => None,
        };
        if let Some(name) = named {
            return name.to_string();
        }
        // Base symbols, in the same index order as the exponent vector.
        const BASE: [&str; 8] = ["m", "kg", "s", "A", "K", "mol", "cd", "bit"];
        let mut above: Vec<String> = Vec::new();
        let mut below: Vec<String> = Vec::new();
        for (base, exponent) in &dims {
            let symbol = BASE.get(*base).copied().unwrap_or("1");
            let power = exponent.unsigned_abs();
            let piece = if power == 1 {
                symbol.to_string()
            } else {
                format!("{symbol}^{power}")
            };
            if *exponent > 0 {
                above.push(piece);
            } else {
                below.push(piece);
            }
        }
        let top = if above.is_empty() {
            "1".to_string()
        } else {
            above.join("*")
        };
        if below.is_empty() {
            top
        } else {
            format!("{top}/{}", below.join("*"))
        }
    }
}
/// One unit, expressed as a factor and offset into its dimension's base unit.
///
/// This is deliberately a small closed table rather than a general unit system. It covers the
/// units that actually appear in the models this tool has been run on, and anything outside it
/// stays `inconclusive` — which is honest, while a guessed conversion would not be. The offset
/// exists for temperatures: 0 degC is 273.15 K, so a difference and an absolute value are not
/// the same operation, and only the forms listed here are converted.
/// SI prefix (or one of the spellings a model actually writes) to its power of ten.
///
/// Prefixes are handled generically rather than by enumerating every prefixed spelling. The
/// previous table listed `km` and `mm` but not `um`, `nm`, `MN` or `hPa`, and an unknown unit did
/// not stay undecided — it fell through to a bare number comparison, so `10 [kN] >= 5000 [N]`
/// was reported as false and `1 [N] == 1 [V]` as true. That is the worst failure mode available
/// to this tool: a wrong verdict that looks decided.
fn si_prefix(name: &str) -> Option<(&'static str, f64, &str)> {
    // Longest first, so `da` wins over `d` and `da` is not read as deci + `a`.
    const PREFIXES: &[(&str, f64)] = &[
        ("da", 1e1),
        ("Y", 1e24),
        ("Z", 1e21),
        ("E", 1e18),
        ("P", 1e15),
        ("T", 1e12),
        ("G", 1e9),
        ("M", 1e6),
        ("k", 1e3),
        ("h", 1e2),
        ("d", 1e-1),
        ("c", 1e-2),
        ("m", 1e-3),
        ("u", 1e-6),
        ("n", 1e-9),
        ("p", 1e-12),
        ("f", 1e-15),
        ("a", 1e-18),
        ("z", 1e-21),
        ("y", 1e-24),
    ];
    for &(prefix, factor) in PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix) {
            if !rest.is_empty() {
                return Some((prefix, factor, rest));
            }
        }
    }
    None
}

/// One unit: its ISQ dimension vector and the scale to SI base units.
///
/// The dimension vector is `(symbol, exponent)` over the seven ISQ base quantities — L, M, T, I,
/// Θ, N, J — plus `information`, which SysML's library models as its own base quantity even
/// though SI does not. Keeping the vector rather than a dimension *name* is what lets derived
/// units compose: `W*h / W` has to reduce to time, and a name-based scheme cannot do that.
struct UnitDef {
    /// Base-quantity exponents: L, M, T, I, temperature, amount, luminosity, information.
    dims: [i32; 8],
    /// Multiply a value in this unit by `factor` to reach SI base units.
    factor: f64,
    /// Added after scaling, for scales that do not start at zero (temperature).
    offset: f64,
}

const D_L: usize = 0;
const D_M: usize = 1;
const D_T: usize = 2;
const D_I: usize = 3;
const D_TEMP: usize = 4;
const D_N: usize = 5;
const D_J: usize = 6;
const D_INFO: usize = 7;

/// Build a dimension vector from `(exponent, base)` pairs.
fn dims(pairs: &[(usize, i32)]) -> [i32; 8] {
    let mut out = [0i32; 8];
    for &(base, exp) in pairs {
        out[base] += exp;
    }
    out
}

/// The SI base or derived unit named `name`, without any prefix.
fn base_unit(name: &str) -> Option<UnitDef> {
    let (d, f) = match name {
        // SI base units.
        "m" | "metre" | "meter" => (dims(&[(D_L, 1)]), 1.0),
        "s" | "sec" | "second" => (dims(&[(D_T, 1)]), 1.0),
        "g" | "gram" => (dims(&[(D_M, 1)]), 1e-3),
        "A" | "ampere" => (dims(&[(D_I, 1)]), 1.0),
        "K" | "kelvin" => (dims(&[(D_TEMP, 1)]), 1.0),
        "mol" | "mole" => (dims(&[(D_N, 1)]), 1.0),
        "cd" | "candela" => (dims(&[(D_J, 1)]), 1.0),
        // Named SI derived units, and the two dimensionless ones.
        "Hz" | "hertz" => (dims(&[(D_T, -1)]), 1.0),
        "N" | "newton" => (dims(&[(D_M, 1), (D_L, 1), (D_T, -2)]), 1.0),
        "Pa" | "pascal" => (dims(&[(D_M, 1), (D_L, -1), (D_T, -2)]), 1.0),
        "J" | "joule" => (dims(&[(D_M, 1), (D_L, 2), (D_T, -2)]), 1.0),
        "W" | "watt" => (dims(&[(D_M, 1), (D_L, 2), (D_T, -3)]), 1.0),
        "C" | "coulomb" => (dims(&[(D_I, 1), (D_T, 1)]), 1.0),
        "V" | "volt" => (dims(&[(D_M, 1), (D_L, 2), (D_T, -3), (D_I, -1)]), 1.0),
        "F" | "farad" => (dims(&[(D_M, -1), (D_L, -2), (D_T, 4), (D_I, 2)]), 1.0),
        "ohm" | "Ohm" => (dims(&[(D_M, 1), (D_L, 2), (D_T, -3), (D_I, -2)]), 1.0),
        "S" | "siemens" => (dims(&[(D_M, -1), (D_L, -2), (D_T, 3), (D_I, 2)]), 1.0),
        "Wb" | "weber" => (dims(&[(D_M, 1), (D_L, 2), (D_T, -2), (D_I, -1)]), 1.0),
        "T" | "tesla" => (dims(&[(D_M, 1), (D_T, -2), (D_I, -1)]), 1.0),
        "H" | "henry" => (dims(&[(D_M, 1), (D_L, 2), (D_T, -2), (D_I, -2)]), 1.0),
        "rad" | "radian" => (dims(&[]), 1.0),
        "sr" | "steradian" => (dims(&[]), 1.0),
        // Temperature scale with an offset. Only used where an absolute value is meant; a
        // difference of two degC values is a difference of kelvins.
        "degC" | "cel" | "celsius" => {
            return Some(UnitDef {
                dims: dims(&[(D_TEMP, 1)]),
                factor: 1.0,
                offset: 273.15,
            })
        }
        // Time units whose ratio to the second is not a power of ten.
        "min" | "minute" => (dims(&[(D_T, 1)]), 60.0),
        "h" | "hr" | "hour" => (dims(&[(D_T, 1)]), 3600.0),
        "d" | "day" => (dims(&[(D_T, 1)]), 86400.0),
        // Mass: the SI base unit for mass is the kilogram, so `t` needs the gram factor too.
        "t" | "tonne" => (dims(&[(D_M, 1)]), 1e3),
        // Energy and power spelled with time units rather than seconds.
        "Wh" => (dims(&[(D_M, 1), (D_L, 2), (D_T, -2)]), 3600.0),
        // Angle that is not a power of ten off the radian.
        "deg" | "degree" => (dims(&[]), std::f64::consts::PI / 180.0),
        // Information, which ISQ does not have but SysML's library does.
        "bit" | "b" => (dims(&[(D_INFO, 1)]), 1.0),
        "B" | "byte" => (dims(&[(D_INFO, 1)]), 8.0),
        _ => return None,
    };
    Some(UnitDef {
        dims: d,
        factor: f,
        offset: 0.0,
    })
}

/// Resolve a unit spelling to its dimension vector and scale.
///
/// Order matters. The whole name is tried first so that a unit whose own symbol happens to be a
/// prefix-plus-base spelling (`min` is not milli-inch, `Pa` is not peta-year) wins over the
/// prefixed reading. Only then is an SI prefix peeled off, and only when the remainder names a
/// real unit — which is what keeps `kWh` = kilo + Wh working while leaving an unknown spelling
/// unknown.
fn unit_def(unit: &str) -> Option<UnitDef> {
    // A qualified spelling (`SI::kg`, `ISQ::MassValue`) names the unit in its last segment.
    let name = unit.rsplit("::").next().unwrap_or(unit).trim();
    if name.is_empty() {
        return None;
    }
    if let Some(def) = base_unit(name) {
        return Some(def);
    }
    if let Some((_prefix, factor, rest)) = si_prefix(name) {
        // `kg` is the SI base for mass: the prefix applies to the gram, so the factor composes
        // with the gram's own 1e-3 rather than replacing it.
        if let Some(def) = base_unit(rest) {
            return Some(UnitDef {
                dims: def.dims,
                factor: def.factor * factor,
                offset: 0.0,
            });
        }
    }
    // A compound spelling: `m/s`, `km/h`, `W*h`. Each piece is looked up on its own and the
    // exponents and factors compose. Without this a value written `30 [m/s]` had no dimension at
    // all, so `0.4 h >= 20 min` decided while `30 [m/s] <= 60 [km/h]` was reported as "not
    // convertible" — a legal model rejected because of how its unit was spelled.
    // Only a spelling with a separator or an exponent is compound: calling the parser for a bare
    // symbol would
    // recurse through CompoundUnit::simple for every unit this table does not know, and recurse
    // forever, because that lookup comes back here.
    if name.contains(['*', '/', '^']) {
        let compound = parse_unit_text(name)?;
        let mut dims = [0i32; 8];
        for (base, exponent) in &compound.dimensions {
            dims[*base] += *exponent;
        }
        return Some(UnitDef {
            dims,
            factor: compound.scale,
            offset: compound.offset,
        });
    }
    None
}

/// Read a stored unit string back into a compound unit. The evaluator carries units as
/// strings, so combining them means parsing what it carries.
fn parse_unit_text(text: &str) -> Option<CompoundUnit> {
    // The renderer prints `W * h` with spaces; the authored form is `W*h`. Both must read the
    // same, so the separator test ignores surrounding whitespace.
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut pieces: Vec<(i32, &str)> = Vec::new();
    let mut sign = 1i32;
    let mut start = 0usize;
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'*' || bytes[i] == b'/' {
            let piece = text[start..i].trim();
            if !piece.is_empty() {
                pieces.push((sign, piece));
            }
            sign = if bytes[i] == b'/' { -1 } else { 1 };
            start = i + 1;
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        pieces.push((sign, tail));
    }
    if pieces.is_empty() {
        return None;
    }
    let mut acc: Option<CompoundUnit> = None;
    for (exp, piece) in pieces {
        // `m^2`, `s^-1`: the exponent is part of the piece. `spelling()` emits this form for any
        // dimension without a named unit, so without it a value that carried `m^2` came back as an
        // unknown unit and `m^2` compared to `m^2` was reported "not convertible".
        let (name, power) = match piece.split_once('^') {
            Some((name, exponent)) => (name.trim(), exponent.trim().parse::<i32>().ok()?),
            None => (piece, 1),
        };
        if name.is_empty() || power == 0 {
            return None;
        }
        let simple = CompoundUnit::simple(name)?.raised(power * exp)?;
        acc = Some(match acc {
            None => simple,
            Some(existing) => existing.combine(simple, 1),
        });
    }
    acc.filter(|u| u.is_known())
}

/// Two values in the same dimension, both brought to the base unit.
///
/// `None` when either unit is unknown or the pair spans two dimensions — the caller turns that
/// into `inconclusive`, never into a verdict.
fn convert_pair(a: f64, ua: &str, b: f64, ub: &str) -> Option<(f64, f64)> {
    let da = unit_def(ua)?;
    let db = unit_def(ub)?;
    // Same dimension vector, not just the same dimension name: `N` and `J` both have names, and
    // comparing them would be comparing a force with an energy. The vector is the comparison.
    if da.dims != db.dims {
        return None;
    }
    Some((a * da.factor + da.offset, b * db.factor + db.offset))
}

/// A literal carrying a unit bracket, as `20 [min]` writes it.
fn literal_with_unit(expr: &Expr) -> Option<(f64, Option<String>)> {
    match expr {
        Expr::Int(v) => Some((*v as f64, None)),
        Expr::Real(text) => text.parse::<f64>().ok().map(|v| (v, None)),
        Expr::Binary {
            op: BinaryOp::Unit,
            left,
            right,
        } => {
            let (value, _) = literal_with_unit(left)?;
            Some((value, Some(right.render_unit())))
        }
        _ => None,
    }
}

fn walk_connections(el: &Element, checker: &Checker, out: &mut Vec<Issue>) {
    // A `bind a = b;` is a binary relationship whose ends are written exactly like a connection's,
    // and the pilot reports the same "Must be an accessible feature" for `bind B0::p0.x = …`
    // (`BindingConnector_redefine.sysml.xt:106`). Reading only `Kind::Connection` left every
    // binding end unjudged.
    if el.kind == Kind::Connection || el.kind == Kind::Bind || el.kind == Kind::FlowUsage {
        checker.check_connection(el, out);
    }
    for child in &el.children {
        walk_connections(child, checker, out);
    }
}

/// Ends written as declarations rather than as a `connect` statement.
///
/// `connector c { end b ::> f[1]; }` states a reference subsetting on the end, and the pilot
/// reports `f` as unresolved when the reference names nothing
/// (`ConnectorTest_ConnectorEndSubsettingBadCase.kerml.xt:32`). The `connect` path handles the
/// statement form; this handles the declared one, which used to be invisible to both.
fn walk_declared_ends(el: &Element, checker: &Checker, out: &mut Vec<Issue>) {
    if el.is_end && !el.references.is_empty() {
        for reference in &el.references {
            let head = reference.split(',').next().unwrap_or(reference).trim();
            // From the connection's *owner*, not from the connection: an end's reference names
            // something the end refers to, and a sibling of the end is not that. Resolving from
            // the connection would find `f` in `connector c { feature f; end b ::> f[1]; }`,
            // which the pilot reports as unresolved.
            let scope = checker
                .index
                .parent
                .get(el.id)
                .copied()
                .flatten()
                .and_then(|connection| checker.index.parent.get(connection).copied().flatten())
                .unwrap_or(el.id);
            // A dotted reference names a feature through its owner — `end part ::>
            // tankAssy.fuel` — and the single-name lookup asks whether the whole path is
            // declared here, which it never is. Two ends of that shape in
            // `InterfaceUsage_Invalid.sysml.xt` (`:79`, `:80`) were reported as naming nothing.
            let target = if head.contains('.') {
                checker
                    .index
                    .resolve_path(scope, &sysml_model::NameRef::new(head, el.line))
            } else {
                checker.index.resolve(scope, head)
            };
            if matches!(target, Resolution::Found(_)) {
                continue;
            }
            out.push(Issue {
                code: "SEM001",
                severity: Severity::Error,
                at: checker.index.qualified[el.id].clone(),
                line: el.line,
                message: format!("connection end `{reference}` names nothing in this model"),
            });
        }
    }
    for child in &el.children {
        // A shorthand connector's ends are projections of `from`/`to`, and the connector-end walk
        // already resolved those endpoints. Checking the projected end's reference again reported
        // the same fault twice on one line, which the pilot counts as an extra diagnostic
        // (`Connector_Invalid.sysml.xt:45`, `:54`).
        if child.is_end && (el.from.is_some() || el.to.is_some()) {
            continue;
        }
        walk_declared_ends(child, checker, out);
    }
}

fn walk_values(el: &Element, checker: &Checker, out: &mut Vec<Issue>) {
    if el.value_expr.is_some() {
        checker.check_value(el, out);
    }
    for child in &el.children {
        walk_values(child, checker, out);
    }
}

fn walk_satisfactions(el: &Element, checker: &Checker, out: &mut Vec<Issue>) {
    if el.kind == Kind::Satisfy {
        checker.check_satisfaction(el, out);
    }
    for child in &el.children {
        walk_satisfactions(child, checker, out);
    }
}

fn walk_predicates(el: &Element, checker: &Checker, out: &mut Vec<Issue>) {
    if el.predicate.is_some() {
        checker.check_predicate(el, out);
    }
    if el.assumptions.is_some() {
        checker.check_assumption(el, out);
    }
    for child in &el.children {
        walk_predicates(child, checker, out);
    }
}

fn collect_verdicts(el: &Element, checker: &Checker, out: &mut Vec<RequirementVerdict>) {
    if let Some(predicate) = el.predicate.as_ref() {
        let model_refs = predicate.refs().len();
        out.push(RequirementVerdict {
            qualified: checker.index.qualified[el.id].clone(),
            line: el.line,
            verdict: checker.evaluate(el.id, predicate),
            model_refs,
        });
    }
    for child in &el.children {
        collect_verdicts(child, checker, out);
    }
}

/// Everything a report needs, in one call.
pub struct Report {
    pub issues: Vec<Issue>,
    pub verdicts: Vec<RequirementVerdict>,
}

impl Report {
    pub fn error_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count()
    }

    /// Requirements whose verdict is not `pass`. A model is not verified while any of these
    /// exist: `inconclusive` means the requirement was never actually tested.
    pub fn unresolved_requirements(&self) -> usize {
        self.verdicts
            .iter()
            .filter(|v| !matches!(v.verdict, Verdict::Pass(_)))
            .count()
    }
}

/// A behaviour graph must only name things that exist.
///
/// Three places where a name could vanish without a word:
///
///   * `transition off then nosuchstate` — an end of a succession. The ends are authored text, not
///     element references, so a typo on either end removed an edge from the graph and left nothing
///     behind. The state and action views draw exactly these edges, so the hole was in the drawing
///     as well as in the model.
///   * `entry nosuchaction;` — a state's entry, do or exit behaviour. The action name never
///     reached the tree at all.
///   * `transition a then b do nosuchaction;` — the effect a transition performs.
///
/// This is a rule over the tree rather than an entry in the reference table. Adding the ends to
/// that table made `impact` stop reporting anything: the table is also the dependency graph, and a
/// transition naming a state would have claimed the state depends on the transition.
fn check_behaviour_ends(checker: &Checker, out: &mut Vec<Issue>) {
    fn unresolved(checker: &Checker, el: &Element, text: &str, what: &str, out: &mut Vec<Issue>) {
        // `first s then [2] d` writes the end's multiplicity in front of the name. Resolving the
        // whole text looks for an element called `[2] d`, which never exists, so the end was
        // reported as naming nothing even when `d` was right there.
        let bare = match text.find("] ") {
            Some(close) if text.starts_with('[') => text[close + 2..].trim(),
            _ => text,
        };
        // Excluding the element itself: `entry missingAction;` has the same text in its own
        // name, so resolving from the element finds the element and every behaviour name reads
        // as satisfied by itself.
        match checker.index.resolve_excluding(el.id, bare, el.id) {
            sysml_resolve::Resolution::Found(_) => {}
            sysml_resolve::Resolution::Unresolved => out.push(Issue {
                code: "SEM096",
                severity: Severity::Error,
                at: checker.index.qualified[el.id].clone(),
                line: el.line,
                message: format!(
                    "this {what} names `{text}`, which is not a state or an action in scope; the \
                     behaviour it states is not in the model"
                ),
            }),
            // An ambiguous name is reported by the type check; saying it twice would make one
            // problem look like two.
            sysml_resolve::Resolution::Ambiguous(_) => {}
            // A transition end is a plain name; `resolve_excluding` cannot produce a `::` path
            // verdict.
            sysml_resolve::Resolution::NotAccessible => {}
        }
    }

    fn walk(el: &Element, checker: &Checker, out: &mut Vec<Issue>) {
        if matches!(el.kind, Kind::Transition | Kind::PseudoState) {
            // `transition off then on` carries its source in both `from` and `name`; reporting it
            // twice would make one name look like two elements.
            let mut seen: Vec<&str> = Vec::new();
            for text in [&el.from, &el.to].into_iter().flatten() {
                let text = text.trim();
                if text.is_empty() || seen.contains(&text) {
                    continue;
                }
                seen.push(text);
                unresolved(checker, el, text, "succession", out);
            }
        }
        if el.kind == Kind::StateAction {
            // `entry A;` names the action in `to`; `entry A { ... }` names it the same way.
            if let Some(text) = el.to.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                unresolved(checker, el, text, "state behaviour", out);
            }
        }
        if el.kind == Kind::Transition {
            if let Some(text) = el.effect_text.as_deref().map(str::trim) {
                // `do send x to y` writes a message, not a reference to an action in scope; the
                // lowering records it as the literal word `message` and that is not a name.
                if !text.is_empty() && text != "message" && !text.starts_with("assign") {
                    unresolved(checker, el, text, "transition effect", out);
                }
            }
        }
        for child in &el.children {
            walk(child, checker, out);
        }
    }

    for root in checker.roots {
        walk(root, checker, out);
    }
}

pub fn check(roots: &[Element], library: &[Element]) -> Report {
    let checker = Checker::new(roots, library);
    Report {
        issues: checker.issues(),
        verdicts: checker.verdicts(),
    }
}

/// A check, the computed values and the unit readings of a model, from one checker.
///
/// The canvas shows all three and asks for them on every load and every edit. Asked separately,
/// each built its own checker, and a checker indexes the whole standard library: that index was
/// most of what each call cost. The answers are the ones [`check`] and [`values_and_units`] give.
pub fn check_with_values(
    roots: &[Element],
    library: &[Element],
) -> (Report, HashMap<String, Value>, HashMap<String, UnitReading>) {
    let checker = Checker::new(roots, library);
    let report = Report {
        issues: checker.issues(),
        verdicts: checker.verdicts(),
    };
    (report, computed_with(&checker), units_with(&checker))
}

/// The elements a view filtered by an annotation shows, optionally narrowed by a constraint.
///
/// That is the annotated elements, the annotations themselves, and the metadata definition they
/// name — a view needs the definition to draw what the elements point at, and the annotations to say
/// what was written.
///
/// `annotated_by(roots, library, "Safety", None)` is every element annotated `@Safety{…}`;
/// with a constraint it is those whose *arguments* satisfy it — `level >= 2` reads `level` from the
/// annotation, not from the annotated element. The name is matched on its last segment, so a model
/// that writes `@Safety` and a filter that writes `@FMEA::Safety` agree.
///
/// The constraint is evaluated against the annotation's own features, which is what makes a filter
/// `@Safety{level >= 2}` a statement about what was written on each element rather than about what
/// some global setting says.
pub fn annotated_by(
    roots: &[Element],
    library: &[Element],
    annotation: &str,
    constraint: Option<&Expr>,
) -> std::collections::HashSet<usize> {
    let checker = Checker::new(roots, library);
    let wanted = annotation.rsplit("::").next().unwrap_or(annotation);
    let mut out = std::collections::HashSet::new();

    fn walk(el: &Element, check: &mut dyn FnMut(&Element)) {
        check(el);
        for child in &el.children {
            walk(child, check);
        }
    }

    let mut check = |el: &Element| {
        for child in &el.children {
            if child.kind != Kind::MetadataUsage {
                continue;
            }
            let Some(type_ref) = child.type_ref.as_deref() else {
                continue;
            };
            if type_ref.rsplit("::").next() != Some(wanted) {
                continue;
            }
            let holds = match constraint {
                None => true,
                Some(expr) => {
                    // The arguments are `level = 3` written inside the annotation, so their values
                    // are evaluated in the annotation's scope and bound as its parameters would be.
                    let mut bindings = Vec::new();
                    for argument in &child.children {
                        let Some(value) = argument.value_expr.as_ref() else {
                            continue;
                        };
                        if let Eval::Val(value, _) = checker.eval(child.id, value, 0) {
                            bindings.push((argument.name.clone(), value));
                        }
                    }
                    checker.holds(child.id, expr, bindings)
                }
            };
            if holds {
                out.insert(el.id);
                // The definition stays too: it is the node every annotated element points at, and a
                // view that dropped it would draw one node per annotation instead of one shared
                // type — or, in the traceability view, no shared centre at all.
                if let Some(definition) = checker.index.resolve(child.id, type_ref).found() {
                    out.insert(definition);
                }
                // The annotation travels with the element it annotates: a view filtered to
                // `@Safety` that dropped the levels would show which parts are safety-relevant and
                // not what was said about them.
                fn subtree(el: &Element, out: &mut std::collections::HashSet<usize>) {
                    out.insert(el.id);
                    for child in &el.children {
                        subtree(child, out);
                    }
                }
                subtree(child, &mut out);
                break;
            }
        }
    };

    for root in roots {
        walk(root, &mut check);
    }
    out
}

/// Every value of the model this layer can compute, by qualified name.
///
/// One `Checker` for the lot: `value_of` builds one per call, and a report that asks about every
/// value in a model would build one per value. A caller that wants the units as well asks
/// [`values_and_units`], which builds one checker for both.
pub fn computed_values(roots: &[Element], library: &[Element]) -> HashMap<String, Value> {
    computed_with(&Checker::new(roots, library))
}

/// The values of the model's own elements, with a checker the caller already built.
///
/// The library is skipped. Its ids follow the model's in the index, and it holds far more values
/// than a model writes: measured on a 514-line model, 1089 values were computed where the model
/// writes 92, and that was 1.7 of the 3.6 seconds a canvas document took. No caller asks what a
/// library constant comes to.
fn computed_with(checker: &Checker) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for id in 0..checker.index.len() {
        if checker.index.is_library(id) {
            continue;
        }
        let Some(el) = checker.flat.get(id).copied().flatten() else {
            continue;
        };
        let Some(expr) = el.value_expr.as_ref() else {
            continue;
        };
        if let Eval::Val(value, _) = checker.eval(id, expr, 0) {
            out.insert(checker.index.qualified[id].clone(), value);
        }
    }
    out
}

/// The computed values and the unit readings of a model, from one checker.
///
/// A report that shows both used to ask for them separately, and each question built its own
/// checker, which indexes the whole standard library. This builds it once.
pub fn values_and_units(
    roots: &[Element],
    library: &[Element],
) -> (HashMap<String, Value>, HashMap<String, UnitReading>) {
    let checker = Checker::new(roots, library);
    (computed_with(&checker), units_with(&checker))
}

/// Look up a value by qualified name, for a ledger that needs to re-evaluate after a change.
pub fn value_of(roots: &[Element], library: &[Element], qualified: &str) -> Option<Value> {
    let checker = Checker::new(roots, library);
    let id = (0..checker.index.len()).find(|&i| checker.index.qualified[i] == qualified)?;
    let el = checker.flat.get(id).copied().flatten()?;
    match checker.eval(id, el.value_expr.as_ref()?, 0) {
        Eval::Val(v, _) => Some(v),
        _ => None,
    }
}

/// Used by tests and by callers that want the attribute list without building a full report.
pub fn iteration_order(roots: &[Element]) -> Vec<&Element> {
    let mut out = Vec::new();
    fn walk<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
        out.push(el);
        for child in &el.children {
            walk(child, out);
        }
    }
    for root in roots {
        walk(root, &mut out);
    }
    out
}

/// Map from qualified name to id, for callers that key by name.
pub fn name_index(roots: &[Element], library: &[Element]) -> HashMap<String, usize> {
    let checker = Checker::new(roots, library);
    (0..checker.index.len())
        .map(|i| (checker.index.qualified[i].clone(), i))
        .collect()
}

/// What a value's unit is, for callers that report on it without evaluating the arithmetic.
///
/// The evaluator already computes dimensions internally; this is the read-only view of that for
/// the ledger, the CLI and the canvas, none of which should have to re-derive a unit table. A
/// value with no unit written returns `None`, and a unit outside the table returns its authored
/// spelling in `symbol` with `dimension: None` — the caller can then say "not checked" rather
/// than "dimensionless", which are different claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitReading {
    /// The unit as written (`Wh`, `m/s`), empty when the value states none.
    pub symbol: String,
    /// The dimension, when the table knows the unit: `mass`, `length/time`, `no dimension`.
    pub dimension: Option<String>,
    /// Whether the two are the same kind of quantity. False when either is unknown.
    pub known: bool,
}

/// Read the unit of one value, by qualified name.
pub fn unit_of(roots: &[Element], library: &[Element], qualified: &str) -> Option<UnitReading> {
    let checker = Checker::new(roots, library);
    let id = (0..checker.index.len()).find(|&i| checker.index.qualified[i] == qualified)?;
    let el = checker.flat.get(id).copied().flatten()?;
    reading_for(&checker, id, el)
}

/// The reading for one element, with a checker the caller already built.
fn reading_for(checker: &Checker, id: usize, el: &Element) -> Option<UnitReading> {
    let text = el.value_text.as_deref()?;
    let symbol = unit_in_text(text).unwrap_or_default().to_string();
    if symbol.is_empty() {
        // Nothing was written, but a calculation or a product of features can still have a
        // dimension: `Power(f, v)` with newtons and metres per second is a power. Reporting "no
        // unit" there would be a claim the evaluator can check and would get wrong — and the whole
        // point of carrying units through a call is that the answer is computable.
        return match el.value_expr.as_ref().map(|expr| checker.eval(id, expr, 0)) {
            Some(Eval::Val(_, Some(spelling))) => Some(reading_of(&spelling)),
            _ => Some(UnitReading {
                symbol,
                dimension: None,
                known: false,
            }),
        };
    }
    match parse_unit_text(&symbol) {
        Some(unit) if unit.is_known() => Some(UnitReading {
            dimension: Some(unit.spelling_name()),
            known: true,
            symbol,
        }),
        _ => Some(UnitReading {
            symbol,
            dimension: None,
            known: false,
        }),
    }
}

/// Every unit of the model this layer can read, by qualified name.
///
/// `unit_of` builds a `Checker` per call, which is fine for one name and quadratic for a report that
/// asks about every value in a model. The canvas rebuilds its document on every edit, so it asks
/// about every value, every time.
pub fn units_of(roots: &[Element], library: &[Element]) -> HashMap<String, UnitReading> {
    units_with(&Checker::new(roots, library))
}

/// The readings of the model's own elements, with a checker the caller already built.
///
/// The library is skipped, for the reason `computed_with` gives: on the same 514-line model 3077
/// readings were taken where the model writes 92 values, and that was 2.0 of the 3.6 seconds.
fn units_with(checker: &Checker) -> HashMap<String, UnitReading> {
    let mut out = HashMap::new();
    for id in 0..checker.index.len() {
        if checker.index.is_library(id) {
            continue;
        }
        let Some(el) = checker.flat.get(id).copied().flatten() else {
            continue;
        };
        if el.value_text.is_none() {
            continue;
        }
        if let Some(reading) = reading_for(checker, id, el) {
            out.insert(checker.index.qualified[id].clone(), reading);
        }
    }
    out
}

/// The reading for a unit spelling the evaluator produced, rather than one the model wrote.
fn reading_of(spelling: &str) -> UnitReading {
    match parse_unit_text(spelling) {
        Some(unit) if unit.is_known() => UnitReading {
            symbol: spelling.to_string(),
            dimension: Some(unit.spelling_name()),
            known: true,
        },
        _ => UnitReading {
            symbol: spelling.to_string(),
            dimension: None,
            known: false,
        },
    }
}

/// The unit written at the end of a value text: `900 [Wh]` → `Wh`.
fn unit_in_text(text: &str) -> Option<&str> {
    let close = text.rfind(']')?;
    let open = text[..close].rfind('[')?;
    let unit = text[open + 1..close].trim();
    if unit.is_empty() {
        None
    } else {
        Some(unit)
    }
}

#[cfg(test)]
mod probe_trigger {
    use super::*;

    #[test]
    fn print_trigger() {
        let src = "package P {\n private import ScalarValues::*;\n state def Power {\n state off;\n state on;\n transition off then on accept when 5;\n }\n}\n";
        let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
        fn walk(el: &Element, depth: usize) {
            if el.kind == Kind::Transition {
                eprintln!(
                    "transition: trigger_kind={:?} trigger_text={:?} trigger_expr={:?}",
                    el.trigger_kind,
                    el.trigger_text,
                    el.trigger_expr.is_some()
                );
            }
            for c in &el.children {
                walk(c, depth + 1);
            }
        }
        for r in &loaded.roots {
            walk(r, 0);
        }
    }
}
