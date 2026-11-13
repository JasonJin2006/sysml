//! The `case-membership` rules, each proved by a model that violates it.
//!
//! Every rule asserted here is proved twice: once by a model that breaks it and produces SEM020,
//! once by the nearest legal model that must stay silent. Asserting only the second half would
//! not distinguish "the rule runs" from "the rule is absent".
//!
//! All ten rules in this group run. The two objective rules used to be out of reach —
//! `objective { ... }` lowered to `Kind::RequirementUsage`, so an objective could not be told from
//! a nested requirement — until the lowering gave `objective` its own kind; the test
//! `an_objective_keeps_its_own_kind` is what keeps that true.

use sysml_model::Kind;
use sysml_semantics::{check, Severity};

/// The helper `tests/rules.rs` uses, so a diagnostic is read the same way here.
fn issues(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

/// Every SEM020 diagnostic, as `(element, message)`.
fn sem020(src: &str) -> Vec<(String, String)> {
    issues(src)
        .into_iter()
        .filter(|(code, _, _)| code == "SEM020")
        .map(|(_, at, message)| (at, message))
        .collect()
}

fn has(src: &str, at: &str, needle: &str) -> bool {
    sem020(src)
        .iter()
        .any(|(element, message)| element == at && message.contains(needle))
}

// -- validateCaseDefinitionOnlyOneSubject / validateCaseUsageOnlyOneSubject ----
//
// The tree keeps case definitions and case usages in one kind (`lower.rs:225`), so both rules run
// through one predicate. Each is shown on the spelling that names it.

#[test]
fn a_case_definition_with_two_subjects_is_rejected() {
    let src = r#"
package P {
    part def A;
    case def C {
        subject a : A;
        subject b : A;
    }
}
"#;
    assert!(
        has(src, "P::C::b", "further subject"),
        "two subjects on a case definition must be reported, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_case_usage_with_two_subjects_is_rejected() {
    let src = r#"
package P {
    part def A;
    case def CD;
    case c : CD {
        subject a : A;
        subject b : A;
    }
}
"#;
    assert!(
        has(src, "P::c::b", "further subject"),
        "two subjects on a case usage must be reported, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_case_with_one_subject_is_accepted() {
    let src = r#"
package P {
    part def A;
    case def C { subject a : A; }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "one subject is legal, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_case_with_no_subject_is_accepted() {
    let src = r#"
package P {
    part def A;
    case def C { in p : A; }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "`size() <= 1` allows none, got {:?}",
        sem020(src)
    );
}

#[test]
fn an_analysis_case_and_a_use_case_are_checked() {
    // `AnalysisCaseDefinition specializes CaseDefinition` (generated.rs:1612) and
    // `UseCaseDefinition specializes CaseDefinition` (generated.rs:1770), so the inherited rule
    // applies to both.
    let src = r#"
package P {
    part def A;
    analysis case def AC {
        subject a : A;
        subject b : A;
    }
    use case def UC {
        subject a : A;
        subject b : A;
    }
}
"#;
    let found = sem020(src);
    assert!(
        found.iter().any(|(at, _)| at == "P::AC::b"),
        "an analysis case is a CaseDefinition, got {found:?}"
    );
    assert!(
        found.iter().any(|(at, _)| at == "P::UC::b"),
        "a use case is a CaseDefinition, got {found:?}"
    );
}

#[test]
fn a_verification_case_is_a_case() {
    // `VerificationCaseDefinition specializes CaseDefinition` and
    // `VerificationCaseUsage specializes CaseUsage` (generated.rs:1773-1774).
    let src = r#"
package P {
    part def A;
    verification def VD {
        subject a : A;
        subject b : A;
    }
}
"#;
    assert!(
        has(src, "P::VD::b", "further subject"),
        "a verification case is a case definition, got {:?}",
        sem020(src)
    );
}

// -- validateRequirementDefinitionOnlyOneSubject /
//    validateRequirementUsageOnlyOneSubject ----------------------------------

#[test]
fn a_requirement_definition_with_two_subjects_is_rejected() {
    let src = r#"
package P {
    part def A;
    requirement def R {
        subject a : A;
        subject b : A;
    }
}
"#;
    assert!(
        has(src, "P::R::b", "further subject"),
        "two subjects on a requirement definition must be reported, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_requirement_usage_with_two_subjects_is_rejected() {
    let src = r#"
package P {
    part def A;
    requirement def RD;
    requirement r : RD {
        subject a : A;
        subject b : A;
    }
}
"#;
    assert!(
        has(src, "P::r::b", "further subject"),
        "two subjects on a requirement usage must be reported, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_requirement_with_one_subject_is_accepted() {
    let src = r#"
package P {
    part def A;
    requirement def RD;
    requirement def R { subject a : A; }
    requirement r : RD { subject a : A; }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "one subject is legal, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_concern_and_a_viewpoint_are_requirements() {
    // `ConcernDefinition specializes RequirementDefinition` (generated.rs:1634) and
    // `ViewpointDefinition specializes RequirementDefinition` (generated.rs:1778).
    let src = r#"
package P {
    part def A;
    concern def K {
        subject a : A;
        subject b : A;
    }
    viewpoint def V {
        subject a : A;
        subject b : A;
    }
}
"#;
    let found = sem020(src);
    assert!(
        found.iter().any(|(at, _)| at == "P::K::b"),
        "a concern is a RequirementDefinition, got {found:?}"
    );
    assert!(
        found.iter().any(|(at, _)| at == "P::V::b"),
        "a viewpoint is a RequirementDefinition, got {found:?}"
    );
}

#[test]
fn an_unrelated_definition_with_two_subjects_is_not_reported() {
    // The rule is stated on the four subject-bearing metaclasses. A part definition that declares
    // two `subject` lines is not one of them, and the check must not reach beyond its own rules.
    let src = r#"
package P {
    part def A;
    part def NotRuled {
        subject a : A;
        subject b : A;
    }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "a part def is outside these rules, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_calculation_is_not_a_case() {
    // A case *specializes* `CalculationDefinition` (generated.rs:1628), not the other way round,
    // so a calc is not ruled by the case statements.
    let src = r#"
package P {
    part def A;
    calc def NotACase {
        subject a : A;
        subject b : A;
    }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "a calc def is not a case definition, got {:?}",
        sem020(src)
    );
}

// -- validateCaseDefinitionSubjectParameterPosition /
//    validateCaseUsageSubjectParameterPosition -------------------------------

#[test]
fn a_case_definition_whose_subject_follows_an_input_is_rejected() {
    let src = r#"
package P {
    part def A;
    case def C {
        in p : A;
        subject a : A;
    }
}
"#;
    assert!(
        has(src, "P::C::a", "is not its first input"),
        "an input before the subject must be reported, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_case_usage_whose_subject_follows_an_input_is_rejected() {
    let src = r#"
package P {
    part def A;
    case def CD;
    case c : CD {
        in p : A;
        subject a : A;
    }
}
"#;
    assert!(
        has(src, "P::c::a", "is not its first input"),
        "an input before the subject must be reported, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_subject_written_first_is_accepted() {
    let src = r#"
package P {
    part def A;
    case def C {
        subject a : A;
        in p : A;
    }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "the subject is the first input, got {:?}",
        sem020(src)
    );
}

// -- validateRequirementDefinitionSubjectParameterPosition /
//    validateRequirementUsageSubjectParameterPosition ------------------------

#[test]
fn a_requirement_definition_whose_subject_follows_an_input_is_rejected() {
    let src = r#"
package P {
    part def A;
    requirement def R {
        in p : A;
        subject a : A;
    }
}
"#;
    assert!(
        has(src, "P::R::a", "is not its first input"),
        "an input before the subject must be reported, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_requirement_usage_whose_subject_follows_an_input_is_rejected() {
    let src = r#"
package P {
    part def A;
    requirement def RD;
    requirement r : RD {
        in p : A;
        subject a : A;
    }
}
"#;
    assert!(
        has(src, "P::r::a", "is not its first input"),
        "an input before the subject must be reported, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_requirement_subject_written_first_is_accepted() {
    let src = r#"
package P {
    part def A;
    requirement def R {
        subject a : A;
        in p : A;
    }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "the subject is the first input, got {:?}",
        sem020(src)
    );
}

#[test]
fn an_output_before_the_subject_is_not_an_input() {
    // `deriveTypeInput` selects features directed `in` or `inout` (generated.rs:8111-8115); an
    // `out` parameter is an output and does not precede the subject in that ordering.
    let src = r#"
package P {
    part def A;
    requirement def R {
        out p : A;
        subject a : A;
    }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "an output is not an input, got {:?}",
        sem020(src)
    );
}

#[test]
fn an_inout_parameter_before_the_subject_is_an_input() {
    // `deriveTypeInput` includes `inout`, so it does precede the subject.
    let src = r#"
package P {
    part def A;
    requirement def R {
        inout p : A;
        subject a : A;
    }
}
"#;
    assert!(
        has(src, "P::R::a", "is not its first input"),
        "`inout` is an input, got {:?}",
        sem020(src)
    );
}

// -- zero subjects is not a violation ---------------------------------------

#[test]
fn a_requirement_usage_with_no_subject_and_an_input_is_accepted() {
    // `input->notEmpty()` guards the ordering rather than demanding a subject. The official
    // validation corpus writes subjectless requirements
    // (`research/upstream/release/sysml/src/validation/08-Requirements/8-Requirements.sysml:88`),
    // so a diagnostic here would be a false positive.
    let src = r#"
package P {
    part def A;
    requirement def RD;
    requirement r : RD { in p : A; }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "`input->first()` is a guard, not a demand for a subject, got {:?}",
        sem020(src)
    );
}

// -- what the projection separates, and what it does not ---------------------

#[test]
fn a_frame_is_not_a_subject() {
    // `frame k : K` is a `FramedConcernMembership`, which specializes
    // `RequirementConstraintMembership` (generated.rs:1678) and is not a `ParameterMembership`.
    // `RequirementBodyItem` admits a subject and a frame side by side
    // (SysML.xtext:2039-2046), so `subject a : K; frame k : K;` is legal and must not be reported.
    // This failed while `frame` lowered to `Kind::Subject`; the two now have distinct kinds
    // (sysml-syntax/src/lower.rs:957-964).
    let src = r#"
package P {
    concern def K;
    requirement def R {
        frame k : K;
        subject a : K;
    }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "a frame is not a SubjectMembership, so one subject plus one frame is legal; got {:?}",
        sem020(src)
    );
}

#[test]
fn two_frames_are_not_two_subjects() {
    let src = r#"
package P {
    concern def K;
    requirement def R {
        frame first : K;
        frame second : K;
    }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "no rule caps the frames a requirement states; got {:?}",
        sem020(src)
    );
}

#[test]
fn an_actor_written_before_the_subject_is_not_counted_as_an_input() {
    // `actor x : T` lowers to a plain `Kind::Part` with no direction
    // (sysml-parser/src/parser.rs:985-986), so this check cannot see that it precedes the
    // subject. The metamodel makes an `ActorMembership` a `ParameterMembership`
    // (generated.rs:1609) and therefore an input, so this is a missed violation. Pinned here so
    // the gap is recorded rather than assumed away.
    let src = r#"
package P {
    part def Person;
    part def Vehicle;
    use case def UC {
        actor driver : Person;
        subject v : Vehicle;
    }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "an undirected actor is not visible as an input; got {:?}",
        sem020(src)
    );
}

// -- objectives keep their own kind, so the two objective rules can count them -------------
//
// `objective { ... }` used to lower to `Kind::RequirementUsage`, which left
// `validateCaseUsageOnlyOneObjective` and `validateCaseDefinitionOnlyOneObjective` without a
// subject: a case owning a nested requirement is legal and ordinary, so counting those would
// report every one of them. `lower.rs:1586-1602` now builds `Kind::Objective`, and these tests pin
// both halves: the kind survives the lowering, and two objectives are reported against it.

#[test]
fn an_objective_keeps_its_own_kind() {
    let src = r#"
package P {
    part def A;
    case def C {
        objective o { }
    }
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let case = loaded
        .roots
        .iter()
        .find(|e| e.name == "P")
        .and_then(|p| p.children.iter().find(|c| c.name == "C"))
        .expect("the case");
    assert!(
        case.children.iter().any(|c| c.kind == Kind::Objective),
        "`objective` must keep its own kind so the objective-count rules can see it; got {:?}",
        case.children
            .iter()
            .map(|c| c.kind.keyword())
            .collect::<Vec<_>>()
    );
}

#[test]
fn two_objectives_are_reported() {
    // validateCaseUsageOnlyOneObjective / validateCaseDefinitionOnlyOneObjective. Counting nested
    // requirement usages would report every case that owns one, so the rule waits for `objective`
    // to keep its own kind — which it now does.
    let src = r#"
package P {
    part def A;
    case def C {
        objective o1 { }
        objective o2 { }
    }
}
"#;
    let found = sem020(src);
    assert!(
        found
            .iter()
            .any(|(at, msg)| at == "P::C::o2" && msg.contains("further objective")),
        "two objectives must be reported, got {found:?}"
    );
}

#[test]
fn a_case_writing_two_objectives_reports_both() {
    // `CaseSubjectObjective_Invalid.sysml.xt:64` and `:67`: `case c: C { objective o3; objective o4; }`
    // reports *both* declarations, even though `C` already contributes one. Counting the inherited
    // set and skipping that many own objectives made `o3` legal, which is why the skip counts from
    // the declaration's own first objective.
    let src = r#"
package P {
    case def C {
        objective o1 { }
    }
    case c : C {
        objective o3 { }
        objective o4 { }
    }
}
"#;
    let found = sem020(src);
    assert!(
        found.iter().any(|(at, _)| at == "P::c::o3"),
        "`o3` is one of two the case writes, got {found:?}"
    );
    assert!(
        found.iter().any(|(at, _)| at == "P::c::o4"),
        "`o4` is one of two the case writes, got {found:?}"
    );
    assert!(
        !found.iter().any(|(at, _)| at == "P::C::o1"),
        "`C` writes exactly one objective, got {found:?}"
    );
}

#[test]
fn one_own_objective_plus_an_inherited_one_is_accepted() {
    // `9-Verification-simplified.sysml:52-58`: `verification vehicleMassTest : MassTest` writes one
    // objective while `MassTest` already declares one. That model conforms, so the rule counts what
    // a declaration writes and reports only beyond the first.
    let src = r#"
package P {
    verification def MassTest {
        objective massVerificationObjective { }
    }
    verification vehicleMassTest : MassTest {
        objective vehicleMassVerificationObjective { }
    }
}
"#;
    let found = sem020(src);
    assert!(
        found.is_empty(),
        "one own objective is legal, got {found:?}"
    );
}

#[test]
fn a_nested_requirement_is_not_an_objective() {
    // The reason the rule needed its own kind: a case may legally own a requirement, and counting
    // requirement usages as objectives would report it.
    let src = r#"
package P {
    requirement def R;
    case def C {
        objective o { }
        requirement nested : R;
    }
}
"#;
    assert!(
        sem020(src).is_empty(),
        "one objective plus a nested requirement is legal, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_case_usage_inheriting_an_out_of_order_subject_is_rejected() {
    // `CaseSubjectObjective_Invalid.sysml.xt:70-85`: `case def C1 :> C { in x; subject s5; }`
    // makes `case c1 : C1;` a case whose subject is not its first input, and the pilot states the
    // error on the *usage* line. Only the written subject list was read, so a usage — which writes
    // no subject at all — had nothing to place and the rule had nothing to say.
    let src = r#"
package P {
    case def C {
        subject s1;
    }
    case def C1 :> C {
        in x;
        subject s5;
    }
    case c1 : C1;
}
"#;
    assert!(
        has(src, "P::c1", "is not its first input"),
        "the usage inherits a subject that follows a parameter, got {:?}",
        sem020(src)
    );
}

#[test]
fn a_case_usage_inheriting_a_first_subject_is_accepted() {
    let src = r#"
package P {
    case def C1 {
        subject s5;
        in x;
    }
    case c1 : C1;
}
"#;
    assert!(sem020(src).is_empty(), "got {:?}", sem020(src));
}
