//! What the parser accepts, and what it does when it cannot.
//!
//! Every case here was a construct the previous parser (`sysml-v2-parser`) rejected: they are the
//! reason this parser exists. Six standard-library and validation files failed on them, and none
//! of the six was fixable from outside that crate.

use sysml_parser::ast::{BodyMember, Declaration, Kind, Statement};
use sysml_parser::parse;

fn doc(src: &str) -> sysml_parser::ParsedDocument {
    parse(src).unwrap_or_else(|e| panic!("must parse: {e}\nsource:\n{src}"))
}

/// Every declaration in the tree, in source order.
fn declarations(d: &sysml_parser::ParsedDocument) -> Vec<Declaration> {
    fn walk(members: &[BodyMember], out: &mut Vec<Declaration>) {
        for m in members {
            match m {
                BodyMember::Declaration(d) => {
                    out.push((**d).clone());
                    walk(&d.body, out);
                }
                BodyMember::Statement(Statement::ControlFlow { branches, .. }) => {
                    for (_, b) in branches {
                        walk(b, out);
                    }
                }
                BodyMember::Statement(Statement::ControlNode { body, .. })
                | BodyMember::Statement(Statement::Objective { body, .. }) => walk(body, out),
                BodyMember::Statement(Statement::For { body, .. }) => walk(body, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&d.document.members, &mut out);
    out
}

fn named<'a>(decls: &'a [Declaration], name: &str) -> &'a Declaration {
    decls
        .iter()
        .find(|d| d.head.name.text == name)
        .unwrap_or_else(|| {
            panic!(
                "no declaration named {name}; found {:?}",
                decls.iter().map(|d| &d.head.name.text).collect::<Vec<_>>()
            )
        })
}

#[test]
fn an_association_structure_is_one_declaration_with_both_specializations() {
    let parsed = doc(
        "package Objects { abstract assoc struct LinkObject specializes Link, Object { \
         end participant : Object; } struct Following; }",
    );
    assert!(
        parsed.document.errors().is_empty(),
        "{:?}",
        parsed.document.errors()
    );
    let decls = declarations(&parsed);
    assert_eq!(
        decls
            .iter()
            .map(|d| d.head.name.text.as_str())
            .collect::<Vec<_>>(),
        ["Objects", "LinkObject", "participant", "Following"]
    );
    let link = named(&decls, "LinkObject");
    assert_eq!(link.kind, Kind::Kerml("assoc struct".to_string()));
    assert!(link.head.prefixes.is_abstract);
    assert_eq!(
        link.head
            .targets_of(sysml_parser::ast::RelationOp::Specializes)
            .iter()
            .map(|target| target.text.as_str())
            .collect::<Vec<_>>(),
        ["Link", "Object"]
    );
    assert_eq!(link.body.len(), 1);
}

#[test]
fn a_bare_end_keeps_its_marker_without_overriding_explicit_kinds() {
    let parsed = doc(
        "connection def C { end bare : T; end port explicitPort : T; \
         end part explicitPart : T; end feature explicitFeature : T; }",
    );
    assert!(
        parsed.document.errors().is_empty(),
        "{:?}",
        parsed.document.errors()
    );
    let decls = declarations(&parsed);
    for (name, kind) in [
        ("bare", Kind::Kerml("end".to_string())),
        ("explicitPort", Kind::Port),
        ("explicitPart", Kind::Part),
        ("explicitFeature", Kind::Kerml("feature".to_string())),
    ] {
        let end = named(&decls, name);
        assert_eq!(end.kind, kind);
        assert!(end.head.prefixes.is_end);
    }
}

// -- the six constructs the previous parser rejected -------------------------

#[test]
fn a_named_flow_reads_its_ends() {
    // `flow XXX from a.b to c.d` is the plain spelling, and it used to fail: the name was only
    // read when a `:` followed it, so `from` stayed in the stream and the parser reported
    // "expected `to` in a flow, found `from`". `Relationship_invalid_relatedElement1` never parsed
    // because of it.
    let d = doc(r#"
package P {
    part a {
        part b;
    }
    part c {
        part d;
    }
    flow XXX from a.b to c.d;
}
"#);
    let flow = declarations(&d)
        .into_iter()
        .find_map(|decl| {
            decl.body.iter().find_map(|member| match member {
                BodyMember::Statement(Statement::Flow { head, .. }) => {
                    Some((head.name.text.clone(), head.from.clone(), head.to.clone()))
                }
                _ => None,
            })
        })
        .expect("a flow statement");
    assert_eq!(flow.0, "XXX");
    assert_eq!(flow.1.as_deref(), Some("a.b"));
    assert_eq!(flow.2.as_deref(), Some("c.d"));
}

/// The flows and successions a body holds, as `(name, from, to)` for each.
fn flows(d: &sysml_parser::ParsedDocument) -> Vec<(String, Option<String>, String)> {
    fn walk(members: &[BodyMember], out: &mut Vec<(String, Option<String>, String)>) {
        for m in members {
            match m {
                BodyMember::Statement(Statement::Flow { head, .. }) => {
                    out.push((
                        head.name.text.clone(),
                        head.from.clone(),
                        head.to.clone().expect("flow has ends"),
                    ));
                }
                BodyMember::Declaration(d) => walk(&d.body, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&d.document.members, &mut out);
    out
}

#[test]
fn a_flow_without_from_reads_its_first_end() {
    // `flow a to b` is the second alternative of `FlowDeclaration` — the `from` is optional and
    // the declaration is optional too, so the word before `to` is the first end and not a name.
    // Read the other way the ends started at `to` and the parse stopped with "expected `to` in a
    // flow, found `b`".
    let d = doc(r#"
package P {
    action a;
    action b;
    flow a to b;
}
"#);
    let flows = flows(&d);
    assert_eq!(flows.len(), 1, "one flow, got {flows:?}");
    assert_eq!(flows[0].0, "", "a bare `a to b` names nothing");
    assert_eq!(flows[0].1.as_deref(), Some("a"));
    assert_eq!(flows[0].2, "b");
}

#[test]
fn a_succession_flow_is_a_flow() {
    // `succession flow` is one keyword pair (`SuccessionFlowKeyword : SuccessionKeyword
    // FlowKeyword`, `SysML.xtext:1274-1279`) sharing `FlowDeclaration` with `flow`. Dispatched as
    // a succession, `flow` became the succession's *name* and `a.q to b.q` spilled into the body
    // as three members called `a`, `q` and `b` — which is where `valid_ActionUsage`'s bogus
    // duplicate members came from.
    let d = doc(r#"
package P {
    part a {
        part q;
    }
    part b {
        part q;
    }
    succession flow a.q to b.q;
}
"#);
    let flows = flows(&d);
    assert_eq!(flows.len(), 1, "one flow, got {flows:?}");
    assert_eq!(flows[0].0, "");
    assert_eq!(flows[0].1.as_deref(), Some("a.q"));
    assert_eq!(flows[0].2, "b.q");
    // The ends are the statement's, not the enclosing body's members.
    let names: Vec<String> = declarations(&d)
        .into_iter()
        .map(|d| d.head.name.text)
        .collect();
    assert_eq!(
        names,
        ["P", "a", "q", "b", "q"],
        "only the two parts and their features are declarations"
    );
}

/// The text of an expression, for the assertions below: `first`/`then`/`guard` are `Expr`s, and
/// what matters for a transition is which name each one reads.
fn expr_text(e: &sysml_parser::ast::Expr) -> String {
    match e {
        sysml_parser::ast::Expr::Ref(r) => r.text.clone(),
        sysml_parser::ast::Expr::Str(s, _) => format!("\"{s}\""),
        other => format!("{other:?}"),
    }
}

#[test]
fn a_transition_reads_its_source_without_first() {
    // `TransitionUsageKeyword (UsageDeclaration? 'first')?` (`SysML.xtext:1854-1864`) makes the
    // declaration conditional on a following `first`, so with no `first` the first word is the
    // *source*. Reading it as the name left the source to be filled in from the enclosing state,
    // and `transition init accept A then S2` then looked like a transition out of the state it
    // was written in — so the rule that says an accepter needs a state source never fired.
    let d = doc(r#"
package P {
    state def S2 {
        state init;
        state S2_1;
        state S2_2;
        transition init accept A then S2_1;
    }
}
"#);
    let succession = declarations(&d)
        .into_iter()
        .find_map(|decl| {
            decl.body.iter().find_map(|member| match member {
                BodyMember::Statement(Statement::Succession { first, then, .. }) => {
                    Some((first.clone(), then.clone()))
                }
                _ => None,
            })
        })
        .expect("a transition");
    let source = succession.0.expect("a source");
    assert_eq!(
        expr_text(&source),
        "init",
        "`init` is the source, not the name"
    );
    let target = succession.1.expect("a target");
    assert_eq!(expr_text(&target), "S2_1");
}

#[test]
fn a_first_transition_keeps_its_guard_and_target() {
    // `first S2_1 if "test" then S2_2` — the leading guard sits between the source and the
    // `then`, and the `then` introduces the target (`TargetTransitionUsage`, `SysML.xtext:1866`).
    // Dropping the guard lost it before any element could carry it, and the parse fell apart into
    // a control flow plus an `attribute S2_2`.
    let d = doc(r#"
package P {
    state def S2 {
        state S2_1;
        state S2_2;
        transition first S2_1 if "test" then S2_2;
    }
}
"#);
    let succession = declarations(&d)
        .into_iter()
        .find_map(|decl| {
            decl.body.iter().find_map(|member| match member {
                BodyMember::Statement(Statement::Succession {
                    first, then, guard, ..
                }) => Some((first.clone(), then.clone(), guard.clone())),
                _ => None,
            })
        })
        .expect("a transition");
    let source = succession.0.expect("a source");
    assert_eq!(
        expr_text(&source),
        "S2_1",
        "the source is the word after `first`"
    );
    let target = succession.1.expect("a target");
    assert_eq!(expr_text(&target), "S2_2", "`then` introduces the target");
    let guard = succession.2.expect("the guard survives");
    assert_eq!(expr_text(&guard), "\"test\"");
}

#[test]
fn a_transition_keeps_trigger_guard_and_effect_in_order() {
    // `TransitionUsageKeyword (UsageDeclaration? 'first')? TransitionSourceMember EmptyParameterMember
    // (EmptyParameterMember TriggerActionMember)? GuardExpressionMember? EffectBehaviorMember?
    // 'then' …` (`SysML.xtext:1854-1864`): the source comes first and is mandatory, then the
    // trigger, the guard and the effect. `transition init accept A if x do f then b` writes all
    // four, and each has to land in its own field rather than one swallowing the rest.
    let d = doc(r#"
package P {
    state def S {
        state init;
        state b;
        transition init accept A if x do f then b;
    }
}
"#);
    let succession = declarations(&d)
        .into_iter()
        .find_map(|decl| {
            decl.body.iter().find_map(|member| match member {
                BodyMember::Statement(Statement::Succession {
                    first,
                    trigger,
                    trigger_kind,
                    guard,
                    effect,
                    then,
                    ..
                }) => Some((
                    first.clone(),
                    trigger.clone(),
                    trigger_kind.clone(),
                    guard.clone(),
                    effect.clone(),
                    then.clone(),
                )),
                _ => None,
            })
        })
        .expect("a transition");
    let source = succession.0.expect("a source");
    assert_eq!(expr_text(&source), "init");
    let trigger = succession.1.expect("the accepter");
    assert_eq!(expr_text(&trigger), "A");
    assert_eq!(
        succession.2.as_deref(),
        Some("accept"),
        "the word that introduced the trigger is kept: it decides what the argument must be"
    );
    let guard = succession.3.expect("the guard");
    assert_eq!(expr_text(&guard), "x");
    let effect = succession.4.expect("the effect");
    assert_eq!(expr_text(&effect), "f");
    let target = succession.5.expect("a target");
    assert_eq!(expr_text(&target), "b");
}

#[test]
fn a_reserved_word_may_name_a_feature() {
    // `step entry[1];` declares a step named `entry`. The previous parser read `entry` as the
    // keyword it is inside a state body and refused the declaration.
    let d = doc("package P { step entry[1]; step do[1] subsets middle; }");
    let decls = declarations(&d);
    assert_eq!(named(&decls, "entry").kind, Kind::Kerml("step".into()));
    assert_eq!(named(&decls, "do").kind, Kind::Kerml("step".into()));
}

#[test]
fn a_return_parameter_may_be_a_declaration() {
    // `return attribute [0..*] = expr;` declares the result parameter. The previous parser had no
    // production for the `attribute` spelling and stopped at `[`.
    let d = doc(r#"
package P {
    calc def F {
        in x : Real;
        return attribute [0..*] = x;
    }
}
"#);
    let decls = declarations(&d);
    let f = named(&decls, "F");
    let returns = f.body.iter().filter(|m| {
        matches!(
            m,
            BodyMember::Statement(Statement::ReturnDeclaration { .. })
        )
    });
    assert_eq!(returns.count(), 1, "the return parameter is a declaration");
}

#[test]
fn a_feature_may_be_redefined_with_a_new_value() {
    // `feature redefines X = y;` — the `redefines` keyword between the name and the value. The
    // previous parser required `:>>` and rejected the keyword spelling.
    let d = doc(r#"
package P {
    part def A { attribute m; }
    part def B :> A { attribute m redefines A::m = 10; }
}
"#);
    let decls = declarations(&d);
    let b = named(&decls, "B");
    let m = b
        .body
        .iter()
        .find_map(|x| match x {
            BodyMember::Declaration(d) if d.head.name.text == "m" => Some(d),
            _ => None,
        })
        .expect("the redefinition");
    assert!(
        m.head.value.is_some(),
        "the value survives the redefines clause"
    );
}

#[test]
fn a_parameter_may_carry_a_default() {
    // `in frame : SpatialFrame[1] default defaultFrame;` — the `default` keyword after the type.
    let d = doc("package P { step s { in frame : Frame[1] default defaultFrame; } }");
    let decls = declarations(&d);
    assert!(decls.iter().any(|x| x.head.name.text == "frame"));
}

// -- bodies that must not be truncated ---------------------------------------

#[test]
fn an_import_body_does_not_swallow_the_next_member() {
    // `private import Definitions::* { doc ... }` — an import with a relationship body. The
    // previous parser skipped to `;`, so the body's `}` closed the package and every member after
    // it vanished from the tree.
    let d = doc(r#"
package P {
    package D { part def T; }
    private import D::* {
        doc /* rationale */
    }
    part x : T;
}
"#);
    let decls = declarations(&d);
    let names: Vec<&str> = decls.iter().map(|x| x.head.name.text.as_str()).collect();
    assert!(
        names.contains(&"x"),
        "the member after the import survives: {names:?}"
    );
}

#[test]
fn an_alias_body_does_not_swallow_the_next_member() {
    let d = doc(r#"
package P {
    part def T;
    alias U for T {
        doc /* an alias with a body */
    }
    part x : U;
}
"#);
    let decls = declarations(&d);
    let names: Vec<&str> = decls.iter().map(|x| x.head.name.text.as_str()).collect();
    assert!(
        names.contains(&"x"),
        "the member after the alias survives: {names:?}"
    );
}

#[test]
fn a_performance_keeps_its_own_body() {
    // `perform 'provide power' { bind a = b; }` — the body belongs to the performance, not to the
    // part that contains it.
    let d = doc(r#"
package P {
    action def Act;
    part def Part { perform act : Act { bind a = b; } }
}
"#);
    let decls = declarations(&d);
    let part = named(&decls, "Part");
    let perform = part.body.iter().find_map(|m| match m {
        BodyMember::Statement(Statement::Behavior { body, .. }) => Some(body),
        _ => None,
    });
    let perform = perform.expect("the performance");
    assert!(
        perform
            .iter()
            .any(|m| matches!(m, BodyMember::Statement(Statement::Bind { .. }))),
        "the bind is inside the performance: {perform:#?}"
    );
}

// -- expressions -------------------------------------------------------------

#[test]
fn an_if_without_an_else_yields_the_then_branch() {
    // `if c ? a` is legal; the value is partial.
    let d = doc("package P { attribute x = if 1 > 0 ? 5; }");
    let decls = declarations(&d);
    assert!(named(&decls, "x").head.value.is_some());
}

#[test]
fn a_sequence_index_is_an_operator_not_a_metadata_tag() {
    // `seq#(i)` indexes; `#Tag` names metadata. The two share a sigil.
    let d = doc("package P { attribute x = seq#(1); }");
    let decls = declarations(&d);
    let value = named(&decls, "x").head.value.clone().expect("a value");
    assert!(
        matches!(value, sysml_parser::ast::Expr::ChainOp { .. }),
        "the index is a chain operation: {value:?}"
    );
}

#[test]
fn a_chained_operation_keeps_its_qualified_name() {
    // `x->ControlFunctions::collect { in y; y }` names the library function by path. Reading one
    // segment as the operator left `::collect` in the stream, and the clause failed with
    // "expected a name, found `::`" — after the filter's condition had already been read as
    // something else, so the model parsed into a shape nothing wrote.
    let d = doc("package P { attribute x = s->ControlFunctions::collect { in y; y }; }");
    let decls = declarations(&d);
    let value = named(&decls, "x").head.value.clone().expect("a value");
    match value {
        sysml_parser::ast::Expr::ChainOp { op, params, .. } => {
            assert_eq!(op, "ControlFunctions::collect");
            assert_eq!(params, vec!["y".to_string()]);
        }
        other => panic!("the operation is a chain: {other:?}"),
    }
}

#[test]
fn a_cast_without_an_operand_keeps_its_member_access() {
    // `(as Safety).isMandatory` casts the element the clause is written on. The parenthesis branch
    // read an empty list and let `as` attach to it, so the member access became a stray name in
    // the enclosing body and the clause lost the condition it stated.
    let d = doc("package P { attribute x = (as Safety).isMandatory; }");
    let decls = declarations(&d);
    let value = named(&decls, "x").head.value.clone().expect("a value");
    let sysml_parser::ast::Expr::Member { base, member, .. } = value else {
        panic!("the member access is the value: {value:?}");
    };
    assert_eq!(member, "isMandatory");
    match *base {
        sysml_parser::ast::Expr::TypeTest {
            kind, type_name, ..
        } => {
            assert_eq!(kind, sysml_parser::ast::TypeTestKind::As);
            assert_eq!(type_name.text, "Safety");
        }
        other => panic!("the base is the cast: {other:?}"),
    }
}

#[test]
fn a_flow_with_two_types_keeps_both_and_its_ends() {
    // `message : F, G from a to c` types the flow with both. The connector lookahead stopped at the
    // comma and read the whole statement as a declaration, so `from`, `b`, `to` and `c` became
    // members of the enclosing body and the flow kept only its first type
    // (`FlowUsage_Invalid.sysml.xt:64`).
    let d = doc("package P { part def A { part b; part c; } flow def F; flow def G; part apart : A { message : F, G from b to c; } }");
    let decls = declarations(&d);
    let apart = named(&decls, "apart");
    let flow = apart
        .body
        .iter()
        .find_map(|m| match m {
            BodyMember::Statement(Statement::Flow { head, .. }) => Some((
                head.target_of(sysml_parser::ast::RelationOp::Typing)
                    .cloned(),
                head.targets_of(sysml_parser::ast::RelationOp::Typing)
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>(),
            )),
            _ => None,
        })
        .expect("the flow is a statement of the body");
    assert_eq!(flow.1.len(), 2, "both types are kept: {flow:?}");
    assert_eq!(flow.0.map(|t| t.text), Some("F".to_string()));
    assert_eq!(
        flow.1.iter().map(|t| t.text.clone()).collect::<Vec<_>>(),
        vec!["F".to_string(), "G".to_string()]
    );
    let strays: Vec<&str> = apart
        .body
        .iter()
        .filter_map(|m| match m {
            BodyMember::Declaration(d) => Some(d.head.name.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        !strays.iter().any(|n| matches!(*n, "from" | "to")),
        "the ends are the flow's, not the body's: {strays:?}"
    );
}

#[test]
fn a_connector_declaration_keeps_its_ends() {
    // `connector a : A from x to y;` writes its ends after the head, not after a `connect`
    // keyword. Read as a declaration they spilled into the body as four members — `from`, `x`,
    // `to`, `y` — and the two ends then counted as declared twice in the enclosing namespace
    // (`ConnectorTest_NestedConnectorConnectingEndsGoodCase.kerml.xt:34`).
    let d = doc("package P { feature x; feature y; connector a : A from x to y; }");
    let decls = declarations(&d);
    let a = named(&decls, "a");
    assert_eq!(a.head.from.as_deref(), Some("x"));
    assert_eq!(a.head.to.as_deref(), Some("y"));
    let strays: Vec<&str> = decls
        .iter()
        .map(|d| d.head.name.text.as_str())
        .filter(|n| matches!(*n, "from" | "to"))
        .collect();
    assert!(
        strays.is_empty(),
        "the ends are the connector's: {strays:?}"
    );
}

#[test]
fn a_standalone_specialization_declares_nothing() {
    // `subtype C specializes B;` states a `Specialization` between two types that already exist
    // (`KerML.xtext:388-398`). Read as a declaration it became two members — `subtype` and `C` —
    // and the second then read as a duplicate of the type it names
    // (`Specialization_invalid.kerml.xt:30`).
    let d = doc("package P { classifier A; classifier B; classifier C conjugates A; subtype C specializes B; }");
    let decls = declarations(&d);
    let names: Vec<&str> = decls.iter().map(|d| d.head.name.text.as_str()).collect();
    assert_eq!(
        names,
        vec!["P", "A", "B", "C"],
        "the statement adds no member: {names:?}"
    );
    let package = decls.first().expect("the package");
    let statement = package
        .body
        .iter()
        .find_map(|m| match m {
            BodyMember::Relationship(sysml_parser::ast::Relationship::Specialization {
                specific,
                general,
                ..
            }) => Some((specific.text.as_str(), general.text.as_str())),
            _ => None,
        })
        .expect("the specialization statement is in the package body");
    assert_eq!(statement, ("C", "B"));
}

#[test]
fn the_arrow_spelling_of_a_specialization_reads_the_same() {
    let d = doc("package P { classifier A; classifier B; subtype A :> B; }");
    let package = declarations(&d).into_iter().next().expect("the package");
    let found = package.body.iter().any(|m| {
        matches!(
            m,
            BodyMember::Relationship(sysml_parser::ast::Relationship::Specialization { general, .. })
                if general.text == "B"
        )
    });
    assert!(found, "`subtype A :> B` is the same statement");
}

#[test]
fn a_feature_named_subtype_is_still_a_declaration() {
    // The word is not reserved by position alone: without a name and an operator after it, it is
    // an ordinary member name.
    let d = doc("package P { feature subtype; }");
    let decls = declarations(&d);
    assert!(named(&decls, "subtype").head.name.text == "subtype");
}

#[test]
fn a_multi_parent_clause_keeps_every_parent() {
    let d = doc("package P { part def A; part def B; part def C :> A, B; }");
    let decls = declarations(&d);
    let c = named(&decls, "C");
    let supers: Vec<&str> = c
        .head
        .targets_of(sysml_parser::ast::RelationOp::Specializes)
        .into_iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(supers, vec!["A", "B"]);
}

// -- error handling ----------------------------------------------------------

#[test]
fn a_bad_member_is_reported_and_the_rest_of_the_file_still_parses() {
    // One unreadable statement must not cost the other members.
    let d = doc(r#"
package P {
    part def Good1;
    part def :: broken here ;
    part def Good2;
}
"#);
    let errors = d.document.errors();
    assert!(!errors.is_empty(), "the bad member is reported");
    let decls = declarations(&d);
    let names: Vec<&str> = decls.iter().map(|x| x.head.name.text.as_str()).collect();
    assert!(names.contains(&"Good1"));
    assert!(
        names.contains(&"Good2"),
        "the member after the error survives: {names:?}"
    );
}

#[test]
fn a_non_ascii_character_is_a_diagnostic_not_a_crash() {
    // The degree sign occupies two bytes. Slicing one byte at the failure point panicked.
    let e = parse("package P { attribute x = 1 °C; }").expect_err("must fail to lex");
    assert!(
        e.message.contains('°'),
        "the character is named: {}",
        e.message
    );
}

#[test]
fn deep_nesting_is_refused_rather_than_overflowing_the_stack() {
    // A model is data. Ten thousand open parens must be a diagnostic, not a crash.
    let src = format!(
        "package P {{ attribute x = {}1{}; }}",
        "(".repeat(400),
        ")".repeat(400)
    );
    // The refusal is a member-level error, not a whole-file failure: the parser keeps what it can
    // read and reports where it stopped.
    let d = doc(&src);
    let errors = d.document.errors();
    assert!(
        errors.iter().any(|(_, m)| m.contains("nesting")),
        "the depth limit is reported: {errors:?}"
    );
}

#[test]
fn a_quoted_short_name_is_kept() {
    // `requirement <'1.1'> massRequirement : MassLimit` — a requirement identifier as a short
    // name.
    let d = doc(r#"
package P {
    requirement def R;
    requirement <'1.1'> r1 : R;
}
"#);
    let decls = declarations(&d);
    let r1 = named(&decls, "r1");
    assert_eq!(r1.head.name.short.as_deref(), Some("1.1"));
}

// -- the loop and conditional productions, both ways ----------------------

#[test]
fn a_loop_without_its_required_member_is_a_syntax_error() {
    // `IfNode` writes a condition and a body, `WhileLoopNode` one of the two and a body, and
    // `ForLoopNode` a variable before `in` (`SysML.xtext:1596-1630`). The parser used to accept
    // all three missing forms and lower them to control flows with the member absent, which made
    // three rules look inapplicable.
    for src in [
        "package P {
	action def A { if then b; }
}
",
        "package P {
	action def A { if c; }
}
",
        "package P {
	action def A { while; }
}
",
        "package P {
	action def A { while c; }
}
",
        "package P {
	action def A { for in 1..3 { } }
}
",
    ] {
        let parsed = parse(src).expect("parses to a document");
        assert!(
            !parsed.document.errors().is_empty(),
            "must report a syntax error: {src}"
        );
    }
}

#[test]
fn the_legal_spellings_still_parse() {
    for src in [
        "package P {
	action def A { if c then b; }
	action b;
}
",
        "package P {
	action def A { if c { b; } }
	action b;
}
",
        "package P {
	action def A { while c { b; } }
	action b;
}
",
        "package P {
	action def A { loop { b; } }
	action b;
}
",
        "package P {
	action def A { for i in 1..3 { b; } }
	action b;
}
",
        "package P {
	attribute x = if c ? 1 else 2;
}
",
    ] {
        let parsed = parse(src).expect("parses");
        assert!(
            parsed.document.errors().is_empty(),
            "must parse cleanly: {src}
{:?}",
            parsed.document.errors()
        );
    }
}

#[test]
fn an_event_naming_a_feature_declares_nothing() {
    // `event a;` is `'event' OwnedReferenceSubsetting FeatureSpecializationPart?`
    // (`SysML.xtext:865-870`): what follows the keyword is a reference, and the production
    // writes no `declaredName`. Read as a declaration, `a` became a member of the package and
    // collided with the `ref a : A` it names (`OccurrenceUsage_invalid.sysml.xt:56`).
    let d = doc("package P { occurrence def A; ref a : A; event a; }");
    let decls = declarations(&d);
    let names: Vec<&str> = decls.iter().map(|d| d.head.name.text.as_str()).collect();
    assert_eq!(
        names,
        vec!["P", "A", "a"],
        "the event adds no member: {names:?}"
    );
    let package = decls.first().expect("the package");
    let event = package
        .body
        .iter()
        .find_map(|m| match m {
            BodyMember::Statement(Statement::Event { reference, .. }) => {
                Some(reference.text.as_str())
            }
            _ => None,
        })
        .expect("the event is a statement in the package body");
    assert_eq!(event, "a");
}

#[test]
fn an_event_naming_a_dotted_path_keeps_the_whole_path() {
    // `event publish_message.sourceEvent;` denotes a member of what `publish_message` is typed
    // by. The declaration path took the first segment for a name and left the rest to the body,
    // so `17b-Sequence-Modeling.sysml`'s `producer` gained an `attribute sourceEvent` it never
    // wrote.
    let d = doc("package P { occurrence def A; ref a : A; event a.areal; }");
    let decls = declarations(&d);
    assert_eq!(
        decls.len(),
        3,
        "no member is left over: {:?}",
        decls
            .iter()
            .map(|d| d.head.name.text.clone())
            .collect::<Vec<_>>()
    );
    let package = decls.first().expect("the package");
    let event = package
        .body
        .iter()
        .find_map(|m| match m {
            BodyMember::Statement(Statement::Event { reference, .. }) => {
                Some(reference.text.as_str())
            }
            _ => None,
        })
        .expect("the event is a statement in the package body");
    assert_eq!(event, "a.areal");
}

#[test]
fn an_event_followed_by_its_own_keyword_is_still_a_declaration() {
    // `OccurrenceUsageKeyword` is the single word `occurrence` (`SysML.xtext:796-798`), and that
    // is what separates the two alternatives: `event occurrence e : E;` declares `e`.
    let d = doc("package P { occurrence def E; event occurrence e : E; }");
    let decls = declarations(&d);
    let names: Vec<&str> = decls.iter().map(|d| d.head.name.text.as_str()).collect();
    assert_eq!(names, vec!["P", "E", "e"], "{names:?}");
}

#[test]
fn all_after_the_kind_keyword_marks_the_type_sufficient() {
    // `assoc all BinaryLink specializes Link` — `isSufficient ?= 'all'` sits *after* the kind
    // keyword (`KerML.xtext:324`, `:468`), while the prefix run reads the `all` that comes before
    // it (`all conn def C;`). The word after the keyword was read as the declaration's name, so
    // every `assoc all` / `struct all` / `class all` / `feature all` in the kernel library became
    // an element called `all` beside a nameless one that had lost its supertype: `BinaryLink`,
    // `Body`, `Point`, `Surface`, `Curve` and `Life` all arrived as the wrong kind.
    let d = doc("package Links { assoc all BinaryLink specializes Link; }");
    let decls = declarations(&d);
    let binary = named(&decls, "BinaryLink");
    assert_eq!(binary.kind, Kind::Kerml("assoc".to_string()));
    assert!(
        binary.head.prefixes.is_sufficient,
        "`all` marks the type sufficient"
    );
    let supers: Vec<&str> = binary
        .head
        .targets_of(sysml_parser::ast::RelationOp::Specializes)
        .into_iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(supers, vec!["Link"]);
    assert!(
        !decls.iter().any(|d| d.head.name.text == "all"),
        "the word is not a name: {:?}",
        decls.iter().map(|d| &d.head.name.text).collect::<Vec<_>>()
    );
}

#[test]
fn a_feature_named_all_keeps_its_name() {
    // The punctuation after the word decides. `part all : T;` names a part `all`; the modifier
    // form is `part all x : T;`, where a name follows. Without that guard the declaration would
    // lose its own name to the modifier.
    let d = doc("package P { part def T; part all : T; }");
    let decls = declarations(&d);
    let decl = named(&decls, "all");
    assert!(!decl.head.prefixes.is_sufficient);
}

#[test]
fn a_clause_after_the_multiplicity_part_is_still_a_specialization() {
    // `FeatureSpecializationPart` is `(FeatureSpecialization)+ MultiplicityPart?
    // FeatureSpecialization*` (`KerML.xtext:573-576`), so `: Wheel[2] nonunique subsets rearWheel`
    // is a typing, a multiplicity part and a subsetting — not a typing followed by a body member.
    // Reading only up to the modifier left `subsets rearWheel` to the enclosing body: the feature
    // had no subsetting, and the rule that compares its uniqueness with the subsetted feature's had
    // nothing to compare (`Subsetting_UniquenessConformance_Invalid.sysml.xt:39`).
    let d = doc("package P { part def Wheel; part rearWheel : Wheel[2]; \
         part a { part w : Wheel[2] nonunique subsets rearWheel; } }");
    let decls = declarations(&d);
    let w = named(&decls, "w");
    assert!(
        w.head.prefixes.is_nonunique,
        "the modifier belongs to the feature"
    );
    let subsets: Vec<&str> = w
        .head
        .targets_of(sysml_parser::ast::RelationOp::Subsets)
        .into_iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(subsets, vec!["rearWheel"]);
    assert!(
        !decls.iter().any(|d| d.head.name.text == "subsets"),
        "`subsets` is an operator, not a member: {:?}",
        decls.iter().map(|d| &d.head.name.text).collect::<Vec<_>>()
    );
}

#[test]
fn an_import_without_a_visibility_keyword_is_a_syntax_error() {
    // `ImportPrefix` is `visibility = VisibilityIndicator 'import'` in both grammars
    // (`SysML.xtext:239-241`, `KerML.xtext:168-170`), so the keyword is not optional:
    // `Import_Visibility_Invalid.sysml.xt:23` asserts an error on the bare spelling. This parser
    // accepted it, which is drift in the one direction a conformance parser must not take — wider
    // than the grammar it claims.
    let parsed = parse("package P {\n\timport ScalarValues;\n}\n").expect("parses to a document");
    assert!(
        !parsed.document.errors().is_empty(),
        "a bare `import` must be a syntax error"
    );
    let ok = parse("package P {\n\tprivate import ScalarValues;\n}\n").expect("parses");
    assert!(
        ok.document.errors().is_empty(),
        "a private import is legal: {:?}",
        ok.document.errors()
    );
}

#[test]
fn an_enumeration_definition_takes_no_abstract_or_variation_prefix() {
    // `EnumerationDefinition` has no `BasicDefinitionPrefix` (`SysML.xtext:767-770`), so `abstract`
    // and `variation` before `enum def` are syntax errors. The keyword is still legal on an
    // enumeration usage, which takes `UsagePrefix`.
    for src in [
        "package P {\n\tabstract enum def E { enum a; }\n}\n",
        "package P {\n\tvariation enum def E { enum a; }\n}\n",
    ] {
        let parsed = parse(src).expect("parses to a document");
        assert!(
            !parsed.document.errors().is_empty(),
            "`{src}` must be a syntax error"
        );
    }
    for src in [
        "package P {\n\tenum def E { enum a; }\n}\n",
        "package P {\n\tpart def D {\n\t\tvariation enum e;\n\t}\n}\n",
    ] {
        let parsed = parse(src).expect("parses to a document");
        assert!(
            parsed.document.errors().is_empty(),
            "`{src}` is legal: {:?}",
            parsed.document.errors()
        );
    }
}

#[test]
fn an_expose_takes_no_visibility_keyword() {
    // `ExposePrefix` is `visibility = ExposeVisibilityKind`, and its only value is the keyword
    // `expose` itself (`SysML.xtext:2366-2372`). `public expose` and `private expose` are not
    // spellings the grammar admits.
    for src in [
        "package P {\n\tpart def Q;\n\tview def V { public expose Q; }\n}\n",
        "package P {\n\tpart def Q;\n\tview def V { private expose Q::*; }\n}\n",
    ] {
        let parsed = parse(src).expect("parses to a document");
        assert!(
            !parsed.document.errors().is_empty(),
            "`{src}` must be a syntax error"
        );
    }
    let ok = parse("package P {\n\tpart def Q;\n\tview def V { expose Q; }\n}\n").expect("parses");
    assert!(
        ok.document.errors().is_empty(),
        "a bare expose is legal: {:?}",
        ok.document.errors()
    );
}

#[test]
fn an_alias_needs_no_name() {
    // `AliasMember` is `'alias' ( '<' memberShortName = Name '>' )? ( memberName = Name )? 'for' …`
    // (`KerML.xtext:161-167`). Reading a name unconditionally consumed `for` as the name and then
    // failed on the target, which cost the whole body after it:
    // `MultiplicityAlias_valid.kerml.xt:42` parsed as one syntax error and nothing else.
    let d = doc("package P { class M [0..1] { alias for Base::exactlyOne; } }");
    assert!(
        d.document.errors().is_empty(),
        "the anonymous alias must parse: {:?}",
        d.document.errors()
    );
    // The symptom of the old reading: `for` became a member's name.
    assert!(
        !declarations(&d).iter().any(|d| d.head.name.text == "for"),
        "`for` is the keyword, not a name"
    );
}

#[test]
fn a_connect_clause_belongs_to_the_declaration() {
    // `ConnectionUsageKeyword UsageDeclaration? ValuePart? ( ConnectorKeyword ConnectorPart )?`
    // (`SysML.xtext:1052-1062`): `connection bc : BC connect b to c;` is one declaration whose
    // ends are `b` and `c`. Left unread, the clause became a *sibling* — a phantom member named
    // `connect` — and the connector had no ends at all (`ConnectionUsage_Invalid.sysml.xt:54`).
    for (src, keyword) in [
        (
            "package P { connection def BC; part p { connection bc : BC connect b to c; } }",
            "connect",
        ),
        (
            "package P { allocation def AD; part p { allocation al : AD allocate b to c; } }",
            "allocate",
        ),
    ] {
        let d = doc(src);
        let decls = declarations(&d);
        let named_decl = decls
            .iter()
            .find(|d| d.head.name.text == "bc" || d.head.name.text == "al")
            .unwrap_or_else(|| panic!("no declaration in {src}"));
        assert_eq!(
            named_decl.head.from.as_deref(),
            Some("b"),
            "the first end belongs to the declaration, in {src}"
        );
        assert_eq!(named_decl.head.to.as_deref(), Some("c"), "in {src}");
        assert!(
            !decls.iter().any(|d| d.head.name.text == keyword),
            "`{keyword}` is a keyword, not a member name, in {src}"
        );
    }
}

#[test]
fn an_anonymous_end_written_as_a_redefinition_is_an_end() {
    // `connection { end ::> b0; }` — the `end` run stopped at the operator, so `end` became the
    // member's name and the connection had no ends at all
    // (`Relationship_invalid_relatedElement0.sysml.xt:33`).
    let d = doc("package P { part b0; part v { connection { end ::> b0; } } }");
    let decls = declarations(&d);
    assert!(
        decls.iter().any(|d| d.head.prefixes.is_end),
        "the member must be an end: {:?}",
        decls.iter().map(|d| &d.head.name.text).collect::<Vec<_>>()
    );
    assert!(
        !decls.iter().any(|d| d.head.name.text == "end"),
        "`end` is a keyword, not a name"
    );
}

#[test]
fn an_end_named_like_a_statement_keyword_stays_a_declaration() {
    // A name that also begins a statement must not steal the member. `end first : T :>> left`
    // declares an end called `first`; dispatching the statement run before the declaration path
    // read it as a `first … then …` succession, so the owned end vanished from the association
    // (both `effective_ends_replace_explicit_reordered_and_diamond_identities` and
    // `interface_shorthand_that_leaves_a_third_inherited_end_is_rejected` depend on this).
    let d = doc(r#"
package P {
    part def T;
    assoc Base { end left : T; end middle : T; end right : T; }
    assoc Keywords :> Base { end first : T :>> left; end then : T :>> right; }
    assoc Ordinals :> Base {
        end first : T :>> left;
        end second : T :>> middle;
        end third : T :>> right;
    }
}
"#);
    assert!(d.document.errors().is_empty(), "{:?}", d.document.errors());
    let decls = declarations(&d);
    let ends = |owner: &str| -> Vec<(String, Kind, String, String)> {
        named(&decls, owner)
            .body
            .iter()
            .map(|member| match member {
                BodyMember::Declaration(end) => {
                    assert!(
                        end.head.prefixes.is_end,
                        "`{}` must keep its end prefix",
                        end.head.name.text
                    );
                    assert!(!end.is_definition, "an end is a usage");
                    assert!(end.body.is_empty(), "an end declares no body of its own");
                    (
                        end.head.name.text.clone(),
                        end.kind.clone(),
                        end.head
                            .target_of(sysml_parser::ast::RelationOp::Typing)
                            .map(|t| t.text.clone())
                            .unwrap_or_default(),
                        end.head
                            .target_of(sysml_parser::ast::RelationOp::Redefines)
                            .map(|t| t.text.clone())
                            .unwrap_or_default(),
                    )
                }
                other => panic!("an end must be a declaration, found {other:?}"),
            })
            .collect()
    };
    let bare_end = Kind::Kerml("end".to_string());
    assert_eq!(
        ends("Keywords"),
        vec![
            (
                "first".to_string(),
                bare_end.clone(),
                "T".to_string(),
                "left".to_string()
            ),
            (
                "then".to_string(),
                bare_end.clone(),
                "T".to_string(),
                "right".to_string()
            ),
        ]
    );
    assert_eq!(
        ends("Ordinals"),
        vec![
            (
                "first".to_string(),
                bare_end.clone(),
                "T".to_string(),
                "left".to_string()
            ),
            (
                "second".to_string(),
                bare_end.clone(),
                "T".to_string(),
                "middle".to_string()
            ),
            (
                "third".to_string(),
                bare_end.clone(),
                "T".to_string(),
                "right".to_string()
            ),
        ]
    );
    assert!(
        !decls.iter().any(|d| d.head.name.text == "end"),
        "`end` is a keyword, not a name: {:?}",
        decls.iter().map(|d| &d.head.name.text).collect::<Vec<_>>()
    );
}

#[test]
fn a_braced_body_expression_is_a_value_not_the_body() {
    // `in whileTest default {true} { … }` writes an `ExpressionBody` as the value
    // (`KerML.xtext:1121-1123`). Reading `{true}` as the declaration's own body left the real body
    // unopened, so the file ended one `}` richer than the model and the drift showed only at its
    // last brace (`Systems Library/Actions.sysml:485`, `:554`).
    let d = doc("package P { calc def C { in whileTest default {true} { attribute x; } } }");
    assert!(
        d.document.errors().is_empty(),
        "must parse without errors: {:?}",
        d.document.errors()
    );
    let decls = declarations(&d);
    let parameter = decls
        .iter()
        .find(|d| d.head.name.text == "whileTest")
        .expect("the parameter");
    assert!(
        parameter.head.value.is_some(),
        "`{{true}}` is the value, not the body"
    );
    assert!(
        !parameter.body.is_empty(),
        "the second brace pair is the body, and it holds `attribute x;`"
    );
}

#[test]
fn a_lambda_body_may_hold_a_parameter_with_a_body_of_its_own() {
    // `-> selectOne { in ref a { … } tradeStudyObjective(a) }`: `ExpressionBodyMember` is
    // `'{' FunctionBodyPart '}'` (`SysML.xtext:1580`), so the braces nest. Ending the lambda at the
    // inner `}` left `tradeStudyObjective(…)` to the enclosing declaration, which then finished one
    // `}` short (`Domain Libraries/Analysis/TradeStudies.sysml:162`).
    let d = doc("package P { attribute r = s->selectOne {in ref a { attr z; } b(a)}; }");
    assert!(
        d.document.errors().is_empty(),
        "must parse without errors: {:?}",
        d.document.errors()
    );
    let decls = declarations(&d);
    let result = decls
        .iter()
        .find(|d| d.head.name.text == "r")
        .expect("the attribute");
    assert!(result.head.value.is_some(), "the chain is the value");
}

#[test]
fn a_featuring_statement_parses_with_or_without_an_identification() {
    // `TypeFeaturing` is `'featuring' ( Identification? 'of' )? featureOfType 'by' featuringType
    // RelationshipBody` (`KerML.xtext:651-655`). The identification is optional, and the statement
    // stands on its own at package level or inside a multiplicity.
    for src in [
        "package P {\n\tfeature X;\n\tclass C {\n\t\tmultiplicity M [0..1] { featuring of M by X; }\n\t}\n}\n",
        "package P {\n\tfeature X;\n\tclass C {\n\t\tmultiplicity M [0..1] { featuring f1 of M by X; }\n\t}\n}\n",
        "package P {\n\tfeature X;\n\tfeaturing of C::M by X;\n}\n",
    ] {
        let parsed = parse(src).expect("parses to a document");
        assert!(
            parsed.document.errors().is_empty(),
            "`{src}` is legal: {:?}",
            parsed.document.errors()
        );
    }
}

#[test]
fn a_featuring_statement_without_its_by_clause_is_a_syntax_error() {
    let parsed = parse("package P {\n\tfeature X;\n\tfeaturing of C::M;\n}\n")
        .expect("parses to a document");
    assert!(
        !parsed.document.errors().is_empty(),
        "a featuring statement needs a `by` clause"
    );
}
