//! `typed-by-definition` — a usage names the kind of definition it is a usage of.
//!
//! Every usage keyword in the notation names a kind of thing, and a type given to it must be the
//! definition of that same kind:
//!
//! ```text
//! port def FuelOut;
//! part def Tank;
//! part def Engine {
//!     port outlet : FuelOut;   // legal
//!     port wrong  : Tank;      // rejected: a part definition is not a port
//! }
//! ```
//!
//! # Where these rules come from
//!
//! They are not among the specification's 180 OCL constraints. They live in the Xtext grammar's
//! validator, and the authority for them here is the OMG pilot implementation's own test models:
//! `PortUsage_Invalid.sysml.xt` marks `port p1 : B;` as "A port must be typed by port
//! definitions.", and seventeen sibling messages cover the other kinds. 82 assertions over 18
//! messages, all of them in `research/pilot`.
//!
//! So this module is not counted in `conformance` — that ledger is the 180 and stays the 180. It is
//! counted in [`EXTENDED`](crate::conformance::EXTENDED) instead, which is the ledger for rules whose
//! authority is the reference implementation's own validator rather than a normative OCL constraint.
//!
//! That claim was checked rather than assumed. Searching `generated.rs` for the phrasing of every
//! message this module reports — "must be typed by <kind> definitions", across all 25 kinds — finds
//! **none** of them, with `must be typed by` as a positive control showing the search itself works.
//! The rules are real, the pilot enforces them, and the specification does not state them as OCL.
//!
//! # One table, not eighteen checks
//!
//! The rule is the same for every kind: every type of the usage must be an instance of one
//! metaclass, and for most kinds there must be exactly one type. The table below is the whole of
//! it, row for row the reference's `checkAllTypes` and `checkOneType` calls in
//! `SysMLValidator.xtend`. Eighteen modules would have been eighteen chances to disagree with each
//! other about a bound that is identical.
//!
//! The metaclass, not the definition word, is what the reference checks. An action may be typed by
//! any `Behavior`, so a state, calculation, case or constraint definition will do; an item by any
//! `Class`; a constraint by any `Predicate`, which includes a requirement definition. A table of
//! definition words disagreed with the reference on thirteen typings: eight it reported that the
//! reference accepts, and five it accepted that the reference rejects.
//!
//! # What it does not judge
//!
//! An untyped usage passes: the pilot implementation leaves `part aPart;` alone in the same file
//! where it marks the typed-but-wrong ones. An unresolved type is skipped — a dangling name is a
//! reference problem. `part` is left out entirely because `validatePartUsagePartDefinition`
//! already covers it under a rule of its own, and a metadata usage is judged by
//! `validateMetadataFeatureMetaclass` (`SEM045`).

use sysml_model::{metaclass, Element, Kind};
use sysml_resolve::{Index, Resolution};

use super::Violation;

/// The code this family reports under. One code for the family, as with the OCL rule groups.
pub const CODE: &str = "SEM090";

/// One row of the table: the metaclass a usage kind's types must be instances of.
struct Row {
    usage: Kind,
    /// The metaclass the reference requires of every type (`checkAllTypes`) or of the one type
    /// (`checkOneType`).
    required: &'static str,
    /// Names the usage kind in the report, e.g. "a port".
    noun: &'static str,
    /// Whether the reference requires exactly one type (`checkOneType`). An untyped usage takes its
    /// implicit library type, so only a second written type can break the count.
    at_most_one: bool,
}

/// The table. Each row is one of the reference's typing checks.
///
/// The reference's exclusions are kept by the kinds themselves: a state, a calculation and a flow
/// are not the action row (`checkActionUsage` skips them), and the case rows are not the
/// calculation row (`checkCalculationUsage` skips cases).
fn rows() -> &'static [Row] {
    use Kind::*;
    &[
        Row {
            usage: PortUsage,
            required: "PortDefinition",
            noun: "a port",
            at_most_one: false,
        },
        Row {
            usage: ActionUsage,
            required: "Behavior",
            noun: "an action",
            at_most_one: false,
        },
        Row {
            usage: StateUsage,
            required: "Behavior",
            noun: "a state",
            at_most_one: false,
        },
        Row {
            usage: AttributeUsage,
            required: "DataType",
            noun: "an attribute",
            at_most_one: false,
        },
        Row {
            usage: ItemUsage,
            required: "Class",
            noun: "an item",
            at_most_one: false,
        },
        Row {
            usage: OccurrenceUsage,
            required: "Class",
            noun: "an occurrence",
            at_most_one: false,
        },
        // `event occurrence e : Real;` — an event occurrence is an occurrence usage
        // (`EventOccurrenceUsage :> OccurrenceUsage`, `generated.rs:1638`), so its type is judged
        // the same way. The declaration form is the one that reaches this row; the reference form
        // has no authored name and is judged by `event_occurrence_type` instead.
        Row {
            usage: EventOccurrenceUsage,
            required: "Class",
            noun: "an occurrence",
            at_most_one: false,
        },
        // A connection may be typed by any association: `connection : A connect b to c;` with
        // `assoc def A` is a connection, and the pilot's `RedefinitionEnd` model is clean with
        // exactly that shape.
        Row {
            usage: Connection,
            required: "Association",
            noun: "a connection",
            at_most_one: false,
        },
        Row {
            usage: InterfaceUsage,
            required: "InterfaceDefinition",
            noun: "an interface",
            at_most_one: false,
        },
        Row {
            usage: FlowUsage,
            required: "Interaction",
            noun: "a flow",
            at_most_one: false,
        },
        Row {
            usage: AllocationUsage,
            required: "AllocationDefinition",
            noun: "an allocation",
            at_most_one: false,
        },
        // `requirement <'1.0'> r10 : R1def, R11def;` — the message says "one requirement
        // definition", and two types is one too many.
        Row {
            usage: RequirementUsage,
            required: "RequirementDefinition",
            noun: "a requirement",
            at_most_one: true,
        },
        // A concern is a requirement (`ConcernUsage :> RequirementUsage`), and the reference judges
        // it by the requirement rule. The tree keeps a concern usage under the definition's kind.
        Row {
            usage: ConcernDef,
            required: "RequirementDefinition",
            noun: "a concern",
            at_most_one: true,
        },
        // `assert constraint aConstraint0 : ABlock;` — the assertion itself. The pilot message is
        // "A constraint must be typed by one constraint definition", and the count is part of it:
        // `assert constraint aConstraint1 : AConstraint, AConstraint1;` names two definitions and
        // is reported too (`ConstraintUsage_Invalid.sysml.xt:44`).
        Row {
            usage: AssertConstraint,
            required: "Predicate",
            noun: "a constraint",
            at_most_one: true,
        },
        Row {
            usage: CalcUsage,
            required: "Function",
            noun: "a calculation",
            at_most_one: true,
        },
        Row {
            usage: CaseDef,
            required: "CaseDefinition",
            noun: "a case",
            at_most_one: true,
        },
        Row {
            usage: AnalysisCaseDef,
            required: "AnalysisCaseDefinition",
            noun: "an analysis case",
            at_most_one: true,
        },
        Row {
            usage: UseCaseUsage,
            required: "UseCaseDefinition",
            noun: "a use case",
            at_most_one: true,
        },
        Row {
            usage: VerificationUsage,
            required: "VerificationCaseDefinition",
            noun: "a verification case",
            at_most_one: true,
        },
        Row {
            usage: ConstraintUsage,
            required: "Predicate",
            noun: "a constraint",
            at_most_one: true,
        },
        Row {
            usage: EnumDef,
            required: "EnumerationDefinition",
            noun: "an enumeration",
            at_most_one: true,
        },
        // `enum e2 : A;` — one value of an enumeration. The message is "An enumeration must be
        // typed by one enumeration definition", and `at_most_one` covers `enum e3 : E, EE;`:
        // two types is one too many for the same rule.
        Row {
            usage: EnumValue,
            required: "EnumerationDefinition",
            noun: "an enumeration",
            at_most_one: true,
        },
        Row {
            usage: ViewUsage,
            required: "ViewDefinition",
            noun: "a view",
            at_most_one: true,
        },
        Row {
            usage: ViewpointUsage,
            required: "ViewpointDefinition",
            noun: "a viewpoint",
            at_most_one: true,
        },
        Row {
            usage: RenderingUsage,
            required: "RenderingDefinition",
            noun: "a rendering",
            at_most_one: true,
        },
        // A usage that is neither an attribute nor an occurrence must be typed by classifiers
        // (`checkUsage`). A parameter and a subject are reference usages, and `in x : someAttr;`
        // types one by an attribute usage. A `ref` written as such is judged by the reference
        // branch below, which states the same rule for the kinds that carry the word.
        Row {
            usage: Parameter,
            required: "Classifier",
            noun: "a usage",
            at_most_one: false,
        },
        Row {
            usage: Subject,
            required: "Classifier",
            noun: "a usage",
            at_most_one: false,
        },
        // The reference applies each check to every subclass of its usage: a performed action is
        // an action (`PerformActionUsage :> ActionUsage`), an exhibited state a state, and an
        // objective, a framed concern and a verified requirement are requirements. The reference
        // forms `perform b.a;` and `exhibit s.sa;` write no `: T` and are judged by
        // `denoted_type` instead.
        Row {
            usage: Perform,
            required: "Behavior",
            noun: "an action",
            at_most_one: false,
        },
        Row {
            usage: ExhibitState,
            required: "Behavior",
            noun: "a state",
            at_most_one: false,
        },
        Row {
            usage: Objective,
            required: "RequirementDefinition",
            noun: "a requirement",
            at_most_one: true,
        },
        Row {
            usage: Frame,
            required: "RequirementDefinition",
            noun: "a concern",
            at_most_one: true,
        },
        Row {
            usage: Verification,
            required: "RequirementDefinition",
            noun: "a requirement",
            at_most_one: true,
        },
        // `entry action e : T;` — an `entry`, `do` or `exit` action is an action usage owned through a
        // `StateSubactionMembership`, and the reference judges it by the action rule.
        Row {
            usage: StateAction,
            required: "Behavior",
            noun: "an action",
            at_most_one: false,
        },
    ]
}

/// Whether the element at `target` is an instance of the metaclass `required`.
///
/// Three kinds stand for a definition and its usage at once (`ConcernDef`, `ViewDef`,
/// `ViewpointDef`), and only the definition half is a classifier. For those, `Index::is_definition`
/// says which half the target is. A kind that names one metaclass is read as that metaclass.
fn is_instance(index: &Index, target: usize, required: &str) -> bool {
    let names = index.kind[target].metaclasses();
    let defined = index.is_definition[target];
    names.iter().any(|name| {
        if names.len() > 1 && metaclass::is_a(name, "Classifier") != defined {
            return false;
        }
        metaclass::is_a(name, required)
    })
}

/// The metaclass every row and every denoted row requires, for the test that each one exists.
pub fn required_metaclasses() -> Vec<&'static str> {
    rows()
        .iter()
        .map(|row| row.required)
        .chain(denoted_rows().iter().map(|row| row.required))
        .collect()
}

fn qualified(el: &Element, index: &Index) -> String {
    index
        .qualified
        .get(el.id)
        .cloned()
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| el.name.clone())
}

/// Follow `alias A is B` to `B`. A chain of aliases is followed to its end.
fn resolve_through_alias(index: &Index, mut id: usize) -> usize {
    for _ in 0..8 {
        if index.kind[id] != Kind::Alias {
            return id;
        }
        let Some(target_text) = index.type_of(id) else {
            return id;
        };
        let head = target_text.split(',').next().unwrap_or(target_text).trim();
        match index.resolve(id, head) {
            Resolution::Found(next) if next != id => id = next,
            _ => return id,
        }
    }
    id
}

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let table = rows();
    let mut out = Vec::new();
    out.extend(denoted_type(roots, index));
    super::walk(roots, |el| {
        // A bare feature (`x : T;`, `in x : T;`) is a reference usage in the specification, not an
        // attribute, so any classifier may type it. The attribute row would demand a data type and
        // report every item, part and port typing a directed port feature (`out image : Payload;`
        // inside a port definition), which the reference accepts (`checkUsage`).
        let bare_feature = Row {
            usage: Kind::AttributeUsage,
            required: "Classifier",
            noun: "a usage",
            at_most_one: false,
        };
        let row = if el.kind == Kind::AttributeUsage && el.is_bare {
            &bare_feature
        } else {
            let Some(row) = table.iter().find(|r| r.usage == el.kind) else {
                return;
            };
            row
        };
        // A redefinition that writes no type takes the one it replaces: `port pp redefines p1`
        // is a port of `p1`'s type, and judging it as untyped would let a port be typed by a part
        // definition through a redefinition the rule never looked at.
        //
        // A subsetting does the same. `occurrence avalue :> aValue` is an occurrence of
        // `aValue`'s type — here a data type — and reading only the typing clause let it through
        // (`OccurrenceUsage_invalid.sysml.xt:45`). Redefinition first: a declaration that writes
        // both names what it replaces before what it narrows.
        // A subject, a return parameter, an objective, a framed concern, a performed or exhibited
        // declaration and an `entry`, `do` or `exit` action keep their type in `type_ref` alone
        // (`lower.rs`), so it is read from there. Two of them can hold something else there. An
        // ordinary parameter keeps a subsetting target (`in attribute deltaT :> ISQ::time`), and
        // a subject bound to a value keeps the value (`subject vehicle = vehicle1;`,
        // `parser.rs`). Neither is a typing, so a parameter is read only when it is a return
        // parameter, and a subject only when its `type_ref` is not the value it is bound to.
        let reads_type_ref = match el.kind {
            Kind::Objective
            | Kind::Frame
            | Kind::Perform
            | Kind::ExhibitState
            | Kind::StateAction => true,
            Kind::Parameter => el.clause.as_deref() == Some("return"),
            Kind::Subject => !matches!(
                (&el.value_expr, el.type_ref.as_deref()),
                (Some(sysml_model::Expr::Ref(value)), Some(typed)) if value.text == typed
            ),
            _ => false,
        };
        let written: Vec<String> = if el.type_refs.is_empty() && reads_type_ref {
            el.type_ref.iter().cloned().collect()
        } else {
            el.type_refs.clone()
        };
        let inherited_types: Vec<String> = if written.is_empty() {
            let mut found = Vec::new();
            for targets in [&el.redefines, &el.specializes, &el.subsets] {
                for target in targets {
                    let head = target.split(',').next().unwrap_or(target).trim();
                    let Resolution::Found(id) = index.resolve(el.id, head) else {
                        continue;
                    };
                    // `in attribute deltaT :> ISQ::time` subsets an alias: `ISQ::time` is a second
                    // name for `ISQBase::duration`, and an alias keeps its target where a feature
                    // keeps its type. The feature behind the alias is what is subsetted.
                    let id = resolve_through_alias(index, id);
                    if let Some(t) = index.type_of(id) {
                        found.push(t.to_string());
                    }
                }
                if !found.is_empty() {
                    break;
                }
            }
            found
        } else {
            Vec::new()
        };
        let types: Vec<String> = if written.is_empty() {
            inherited_types
        } else {
            written
        };
        if types.is_empty() {
            return;
        }
        // A synthesised element is not a usage anyone wrote a kind for: `destroy connection of
        // trailer to vehicle` inside an action becomes an attribute with that text as its name and
        // no authored name at all. `name_span` is the evidence the author wrote one.
        //
        // A flow statement is the exception, and it is an authored element: `message : A from b to
        // c` declares an unnamed flow usage, which the pilot implementation marks "A flow usage
        // must be typed by flow definitions". Its name is the keyword because the author wrote
        // none, not because a lowering invented the member.
        // Three statement kinds write a type without writing a name: `message : A from b to c`
        // (a flow usage), `allocation :P allocate b to c;`, and `connection :A connect b to c;`.
        // The pilot asserts a rule about each of them, so the fact that no name is authored is
        // not a reason to skip them. Every other element without a `name_span` is synthesised by
        // the lowering (`destroy connection of trailer to vehicle` becomes an attribute with that
        // text as its name) and is not a usage anyone wrote a kind for.
        // The statements that write a type without the lowering recording a name span: a subject,
        // an objective, a framed concern, a performed or exhibited declaration, an `entry`, `do` or
        // `exit` action, and a `return` parameter. They are authored even when unnamed
        // (`subject : V;`), so the guards below do not take them for synthesised members.
        let authored_statement = matches!(
            el.kind,
            Kind::FlowUsage
                | Kind::AllocationUsage
                | Kind::Connection
                | Kind::AssertConstraint
                | Kind::Subject
                | Kind::Objective
                | Kind::Frame
                | Kind::Perform
                | Kind::ExhibitState
                | Kind::StateAction
        ) || (el.kind == Kind::Parameter
            && el.clause.as_deref() == Some("return"));
        if !index.has_authored_name(el.id) && !authored_statement {
            return;
        }
        // An expression that became an element and a `ref` that kept the keyword carry a name and
        // a `name_span` from the syntax they came from, so the test is on the text instead. The
        // statements above are exempt for the same reason as the previous guard.
        if (el.name.starts_with('<') || el.name == "ref") && !authored_statement {
            return;
        }
        // An element with a `clause` used to be skipped here, for `verify requirement r : R`, which
        // once lowered to an attribute. A verification has its own kind now. The elements that
        // carry a clause today are a return parameter and an `entry`, `do` or `exit` action, and
        // the reference types both: a return parameter by the generic usage rule and the action by
        // the action rule.
        // A metadata definition's members name metaclasses, not model elements: `metadata def
        // X { attribute annotatedElement : SysML::Systems::PartUsage }` says which metaclass the
        // annotation applies to. Those types are metadata definitions by design.
        if let Some(owner) = index.parent.get(el.id).copied().flatten() {
            if matches!(index.kind[owner], Kind::MetadataDef) {
                return;
            }
        }
        // `ref hitchBall : HitchBall` is a reference usage: it names something that already
        // exists, so the kind of the thing referenced is its own business. `lower.rs` gives a
        // reference the attribute kind, which is why this has to be excluded by the flag rather
        // than by the kind.
        // `ref X : Y` names something that already exists, so the kind of what it names is its
        // own business — except that a reference to a *usage* is not a reference to a thing but a
        // second name for a member, and the pilot implementation's `ref a1 : a;` (with `attribute
        // a;`) is marked "A usage must be typed by definitions". The type must be a definition.
        //
        // This reads `modifiers.is_reference`, not `Element::is_reference`, and the difference is
        // load-bearing. The modifier records the word the declaration wrote. The element field also
        // carries the metaclass constants — an `AttributeUsage` and an `EventOccurrenceUsage` are
        // `isReference` by definition (`generated.rs:2033`, `:2212`) — and a rule about what the
        // author wrote must not see those. Measured: reading the element field here made
        // `event occurrence e : Real;` report "a reference typed by a data type" instead of
        // "an occurrence typed by a data type".
        if el.modifiers.is_reference {
            for text in &el.type_refs {
                let head = text.split(',').next().unwrap_or(text).trim();
                let Resolution::Found(target) = index.resolve(el.id, head) else {
                    continue;
                };
                if index.kind[target].is_definition() {
                    continue;
                }
                let name = index
                    .qualified
                    .get(target)
                    .cloned()
                    .filter(|q| !q.is_empty())
                    .unwrap_or_else(|| head.to_string());
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` is a reference typed by `{name}`, a {}; a reference names a \
                         definition, not another usage",
                        qualified(el, index),
                        index.kind[target].keyword()
                    ),
                ));
            }
            return;
        }
        // An `end` is typed by whatever the connector relates. Its default kind depends on the
        // owner, so exclude it by the end flag rather than assuming every end is a port.
        if el.is_end {
            return;
        }
        let mut wrong: Vec<(String, String, bool)> = Vec::new();
        let mut right = 0usize;
        for text in &types {
            let head = text.split(',').next().unwrap_or(text).trim();
            // A member the declaration redefines is hidden from its type name, as the specification
            // resolves it (`Index::resolve_typing`; `CalculationUsage_Invalid2.sysml.xt:38-53`).
            let Resolution::Found(target) = index.resolve_typing(el.id, head) else {
                continue;
            };
            // An alias is a second name for something, not the thing itself: `alias Torque is
            // ISQ::TorqueValue` used as a type means `ISQ::TorqueValue`, and judging the alias
            // element's own kind would report every aliased type.
            let target = resolve_through_alias(index, target);
            let kind = index.kind[target].clone();
            // A KerML type from the library is a function or classifier, not a definition of a
            // SysML kind: `destroy connection of trailer to vehicle` resolves its name to
            // `OccurrenceFunctions::destroy`. Judging the library's own shape is not this rule.
            if kind == Kind::KermlType || kind == Kind::FunctionDef || kind == Kind::ExpressionDef {
                continue;
            }
            // `snapshot ctx : VehicleRoadContext_ID1` states a portion of an occurrence, and
            // `lower.rs` gives it the attribute kind because the prefix is a portion word. What it
            // can be typed by is what an occurrence can: the definition it is a portion of.
            let portioned = el.modifiers.portion.is_some();
            let right_kind = is_instance(index, target, row.required)
                || (portioned && is_instance(index, target, "Class"));
            // ... and, for an interface, it must be a *definition*. `interface opposite1 : fi`
            // names an interface usage, and since `Kind::InterfaceUsage` exists the kind test
            // alone still says usage, not definition (`InterfaceUsage_Invalid.sysml.xt:75`), so
            // the usage/definition split is asked here.
            //
            // Asked of every row it is wrong: the corpus types usages by usages in 58 places
            // (`part wheel : someWheel`), which the metamodel allows — `Feature::type` is any
            // `Type`. Only the interface row has the evidence, so only it asks.
            let needs_definition = row.usage == Kind::InterfaceUsage;
            if right_kind && (!needs_definition || index.is_definition[target]) {
                right += 1;
                continue;
            }
            let name = index
                .qualified
                .get(target)
                .cloned()
                .filter(|q| !q.is_empty())
                .unwrap_or_else(|| head.to_string());
            wrong.push((name, kind.keyword().to_string(), right_kind));
        }
        for (name, kind, right_kind) in &wrong {
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                if *right_kind {
                    // The kind was right and the declaration is a usage: `interface opposite1 :
                    // fi` where `fi` is an interface usage.
                    format!(
                        "`{}` is {} typed by `{name}`, a {kind} usage; its type must be a \
                         definition",
                        qualified(el, index),
                        row.noun
                    )
                } else if row.required == "Classifier" {
                    // The reference's message for the generic rule is "A usage must be typed by
                    // definitions": there is no kind for the type to be the same as.
                    format!(
                        "`{}` is a usage typed by `{name}`, a {kind}; a usage must be typed by \
                         definitions",
                        qualified(el, index)
                    )
                } else {
                    format!(
                        "`{}` is {} typed by `{name}`, a {kind}; its type must be a definition of \
                         the same kind ({} definitions required)",
                        qualified(el, index),
                        row.noun,
                        row.noun.split_once(' ').map_or(row.noun, |(_, noun)| noun)
                    )
                },
            ));
        }
        // `An enumeration attribute cannot have more than one type` — a value of an
        // enumeration is one value, so `attribute a2 : E, A;` with `E` an enumeration names two
        // things at once. The pilot states this separately from the typing rule above, and the
        // condition is its own: at least one of the types is an enumeration and there is more
        // than one of them.
        if el.kind == Kind::AttributeUsage && types.len() > 1 {
            let names_an_enum = types.iter().any(|text| {
                let head = text.split(',').next().unwrap_or(text).trim();
                match index.resolve(el.id, head) {
                    Resolution::Found(target) => index.kind[target] == Kind::EnumDef,
                    _ => false,
                }
            });
            if names_an_enum {
                out.push(Violation::new(
                    CODE,
                    el,
                    qualified(el, index),
                    format!(
                        "`{}` is an attribute typed by {} types, one of them an enumeration; an \r
                         enumeration attribute cannot have more than one type",
                        qualified(el, index),
                        types.len()
                    ),
                ));
            }
        }
        if row.at_most_one && right > 1 {
            out.push(Violation::new(
                CODE,
                el,
                qualified(el, index),
                format!(
                    "`{}` is {} typed by {right} definitions; at most one is allowed",
                    qualified(el, index),
                    row.noun
                ),
            ));
        }
    });
    out
}

/// One row of [`denoted_rows`]: a usage that denotes an existing feature is typed by what that
/// feature is typed by.
struct DenotedRow {
    usage: Kind,
    /// The metaclass the denoted feature's type must be an instance of.
    required: &'static str,
    /// Names the usage in the report ("an occurrence") and, minus the article, its definitions
    /// ("occurrence definitions").
    noun: &'static str,
    /// The same noun without its article.
    definitions: &'static str,
}

/// The three usages whose notation has a reference alternative and no declared name.
///
/// `event a.areal;` is `EventOccurrenceUsage : 'event' ( OwnedReferenceSubsetting
/// FeatureSpecializationPart? | OccurrenceUsageKeyword UsageDeclaration? )` (`SysML.xtext:865-870`);
/// `perform` and `exhibit` have the same shape (`SysML.xtext:1411-1425,1840-1848`). In the first
/// alternative the element declares nothing and denotes a feature that already exists, so it is
/// typed by that feature's type — and the pilot says so: `exhibit s.sa;` with `sa : A` draws the
/// same "A state must be typed by state definitions" it draws at `state sa : A;`
/// (`StateUsage_invalid.sysml.xt:72,86`).
///
/// This question is here rather than in [`rows`] because the element has no authored name, and the
/// table starts with the name a usage was given.
fn denoted_rows() -> &'static [DenotedRow] {
    &[
        DenotedRow {
            usage: Kind::EventOccurrenceUsage,
            required: "Class",
            noun: "an occurrence",
            definitions: "occurrence",
        },
        DenotedRow {
            usage: Kind::Perform,
            required: "Behavior",
            noun: "an action",
            definitions: "action",
        },
        DenotedRow {
            usage: Kind::ExhibitState,
            required: "Behavior",
            noun: "a state",
            definitions: "state",
        },
    ]
}

/// The three usages that denote a feature are typed by what it is typed by.
///
/// The reference is resolved as a path: `exhibit s.sa` names a member of what `s` is typed by, not
/// a name in the exhibit's own scope.
fn denoted_type(roots: &[Element], index: &Index) -> Vec<Violation> {
    let table = denoted_rows();
    let mut out = Vec::new();
    super::walk(roots, |el| {
        let Some(row) = table.iter().find(|r| r.usage == el.kind) else {
            return;
        };
        // The reference form. A `: T` clause on the same declaration is checked against the
        // table's row instead, which reads it from `type_refs`.
        let Some(text) = el.performs.as_deref() else {
            return;
        };
        if !el.type_refs.is_empty() {
            return;
        }
        // The host a dotted performance is lowered onto carries the same clause, and the outer
        // element is the one the statement is about.
        if !el.redefines.is_empty() {
            return;
        }
        let head = text.split(',').next().unwrap_or(text).trim();
        let path = sysml_model::NameRef::new(head, el.line);
        let Resolution::Found(denoted) = index.resolve_path(el.id, &path) else {
            return;
        };
        let Some(type_text) = index.type_of(denoted) else {
            return;
        };
        let type_head = type_text.split(',').next().unwrap_or(type_text).trim();
        let Resolution::Found(typing) = index.resolve(denoted, type_head) else {
            return;
        };
        let typing = resolve_through_alias(index, typing);
        let kind = index.kind[typing].clone();
        // A library type is not a definition of a SysML kind, the same exclusion the table
        // makes: judging the library's own shape is not this rule.
        if is_instance(index, typing, row.required)
            || matches!(
                kind,
                Kind::KermlType | Kind::FunctionDef | Kind::ExpressionDef
            )
        {
            return;
        }
        let name = index
            .qualified
            .get(typing)
            .cloned()
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| type_head.to_string());
        out.push(Violation::new(
            CODE,
            el,
            head.to_string(),
            format!(
                "`{head}` denotes `{}`, which is typed by the {} `{name}`; {} must be typed by \
                 {} definitions",
                index.qualified[denoted],
                kind.keyword(),
                row.noun,
                row.definitions
            ),
        ));
    });
    out
}

/// How many rows the table has — the denominator of the extended ledger.
pub fn rule_count() -> usize {
    rows().len()
}
