//! The specification's rules that this kernel enforces, each proved by a model that breaks it.
//!
//! A rule appears here only when a model violating it produces the diagnostic. A test that merely
//! shows a valid model passing would not distinguish "the rule runs" from "the rule is absent",
//! which is the failure mode this file guards.

use sysml_semantics::conformance;
use sysml_semantics::{check, Severity};

fn issues(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

fn codes(src: &str) -> Vec<String> {
    issues(src).into_iter().map(|(c, _, _)| c).collect()
}

// -- the ledger itself -------------------------------------------------------

#[test]
fn the_specification_states_one_hundred_and_eighty_rules() {
    assert_eq!(conformance::RULES.len(), 180);
}

#[test]
fn every_property_check_names_metaclasses_the_metamodel_has() {
    // The property checks are not the specification's 180 constraints, so no other guard looks at
    // them: a name that drifted from the metamodel would silently stop matching any element, and a
    // check that matches nothing reports nothing — which reads exactly like a clean model.
    for check in conformance::PROPERTY_CHECKS {
        assert!(
            sysml_model::metaclass::is_metaclass(check.owner),
            "{} names `{}`, which is not a metaclass",
            check.code,
            check.owner
        );
        match check.constraint {
            conformance::PropertyConstraint::Each(declared) => assert!(
                sysml_model::metaclass::is_metaclass(declared),
                "{} declares its values `{declared}`, which is not a metaclass",
                check.code
            ),
            // A count of nothing would accept every model, which is the failure a check that
            // stops running looks like.
            conformance::PropertyConstraint::AtLeast(min) => assert!(
                min >= 1,
                "{} asks for at least {min} values, which every property has",
                check.code
            ),
            // An upper bound of zero would reject every model that states the property at all.
            conformance::PropertyConstraint::AtMost(max) => assert!(
                max >= 1,
                "{} allows at most {max} values, which forbids the property",
                check.code
            ),
        }
    }
}

#[test]
fn every_implemented_rule_names_a_rule_that_exists() {
    for (code, name) in conformance::implemented() {
        assert!(
            conformance::RULES.iter().any(|r| r.name == *name),
            "{code} claims to enforce {name}, which is not in the specification's table"
        );
    }
}

#[test]
fn the_unimplemented_list_is_the_complement() {
    let done = conformance::implemented().len();
    let todo = conformance::unimplemented().len();
    // Two SEM codes can share one rule, and one rule can map to two codes; the union of names is
    // what has to account for the table.
    let named: std::collections::BTreeSet<&str> =
        conformance::implemented().iter().map(|(_, n)| *n).collect();
    assert_eq!(named.len() + todo, conformance::RULES.len());
    assert!(done >= named.len());
}

#[test]
fn coverage_reports_a_real_fraction_not_a_pass() {
    let c = conformance::coverage();
    assert_eq!(c.total, 180);
    // The count is pinned rather than bounded: growing it means a rule was listed, and listing
    // one is only correct when a test in `tests/rules_*.rs` shows a model that breaks it being
    // rejected under that rule's code. Re-check that before raising this number.
    //
    // 127 → 128 for `validateSubsettingFeaturingTypes`, whose failing-model test is
    // `tests/rules_subsetting_featuring.rs::a_subsetted_feature_in_an_unrelated_type_is_reported`.
    //
    // `validateConnectionDefinitionIsSufficient` is *not* counted, and the reason is worth recording:
    // its default is now filled in `derive_modifiers`, so the flag is right by construction, but no
    // rule module can report it — there is no model that violates it, so a check would always pass.
    // `every_enforced_rule_has_code_that_reports_it` caught that when it was briefly listed here.
    // It stays not-applicable with a reason that says the default is filled and verified
    // (`tests/rules_connection_sufficient.rs`), which is a stronger statement than the old one.
    //
    // 128 → 131, and 51 → 48 not applicable: `validateExpressionResultParameterMembership`,
    // `validateFunctionResultParameterMembership` and `validateParameterMembershipParameterDirection`
    // were listed as unviolatable and are not — two `return`s and `return in r` are both writable.
    // Their failing-model tests are in `tests/rules_result_parameters.rs`.
    //
    // Then 48 → 41 not applicable, 1 → 2 in conflict, 0 → 6 blocked. `validateAcceptActionUsageParameters`
    // Type featuring (`SEM119`, `SEM120`) enforced: 131 → 133 enforced, 6 → 4 blocked.
    // Annotation ownership (`SEM121`) enforced: 133 → 134 enforced, 4 → 3 blocked.
    // Instantiated type (`SEM122`) enforced, reported beside the name SEM003 reports: 134 → 135, 3 → 2.
    // Send parameters (`SEM123`) enforced: a send owns three inputs only when it writes `to`. 135 → 136, 2 → 1.
    // moved to conflict: the literal count is violable by `accept s : Signal;`, and the reference only
    // checks that the list is not empty. Four of the six blocked rules need a metaclass or an implied
    // element the tree does not build (`TypeFeaturing`, `Annotation`, `inputParameters`), and two need a
    // derived property (`instantiatedType`, `result`).
    assert_eq!(
        c.enforced, 136,
        "the enforced count changed; if a rule was added, add its failing-model test first"
    );
    assert_eq!(
        c.not_applicable, 41,
        "the not-applicable count is pinned too"
    );
    assert_eq!(
        c.conflict, 2,
        "two rules where the reference and the specification disagree, and the kernel follows the reference"
    );
    assert_eq!(
        c.blocked, 1,
        "one rule is violable and waits on a derived property the tree does not build"
    );
    assert_eq!(
        c.enforced + c.not_applicable + c.conflict + c.blocked,
        c.total,
        "the four states partition the rule table"
    );
    assert!(
        c.enforced < 180,
        "the kernel does not enforce the whole specification"
    );
    // The denominator is the whole table, deliberately. It used to be `total - not_applicable`,
    // which made the reported ratio self-certifying: filing a rule as "not applicable" shrank the
    // denominator and raised the score. An audit of the 52 entries found one that was unimplemented
    // work rather than an impossible rule, so the figure that survives an adversarial read is
    // `enforced / total`. This assertion is the guard: it fails if anyone reintroduces a shrinking
    // denominator.
    assert_eq!(
        c.applicable, c.total,
        "the denominator must be the whole rule table, not the table minus what we filed away"
    );
    assert!(c.not_applicable < c.total);
}

/// A rule listed as not applicable must not also be listed as enforced, and must exist in the
/// specification's table: an entry that names nothing would shrink the denominator for free.
#[test]
fn not_applicable_names_real_rules_and_no_enforced_ones() {
    for (name, why) in conformance::NOT_APPLICABLE {
        assert!(
            conformance::RULES.iter().any(|r| r.name == *name),
            "`{name}` is not a rule the specification states"
        );
        assert!(
            !conformance::implemented().iter().any(|(_, n)| n == name),
            "`{name}` is both enforced and not applicable"
        );
        assert!(!why.is_empty(), "`{name}` gives no reason");
    }
}

/// Every rule this kernel claims to enforce must be named by a test that feeds it a violating
/// model. The count above can only be trusted if that holds, so it is checked here rather than
/// assumed: a rule listed without a test is the exact defect the ledger exists to prevent.
#[test]
fn every_enforced_rule_is_named_by_a_rule_test() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut corpus = String::new();
    for entry in std::fs::read_dir(&dir).expect("tests dir") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        // Every test file counts, not only the `rules*` ones: SEM001/SEM002 predate this ledger
        // and are covered in `checks.rs`. What matters is that some test exercises the rule, not
        // which file it lives in.
        if name.ends_with(".rs") {
            corpus.push_str(&std::fs::read_to_string(&path).expect("read"));
        }
    }
    for (code, rule) in conformance::implemented() {
        assert!(
            corpus.contains(rule),
            "{code} lists `{rule}` as enforced, but no test names it: add a model that breaks it              or drop the entry"
        );
    }
}

// -- validateMultiplicityRangeBounds -----------------------------------------

#[test]
fn a_lower_bound_above_its_upper_bound_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	part def Car { part wheels : Wheel[4..2]; }
}
"#;
    let found = issues(src);
    assert!(
        found.iter().any(|(code, at, msg)| code == "SEM013"
            && at == "wheels"
            && msg.contains("lower bound above its upper bound")),
        "a bound of [4..2] must be reported, got {found:?}"
    );
}

#[test]
fn a_valid_bound_is_accepted() {
    let src = r#"
package P {
	part def Wheel;
	part def Car { part wheels : Wheel[0..4]; }
}
"#;
    assert!(codes(src).is_empty(), "a valid bound must not be reported");
}

// -- validateMultiplicityRangeBoundResultTypes -------------------------------

#[test]
fn a_fractional_bound_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	part def Car { part odd : Wheel[2.5]; }
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, _, msg)| code == "SEM013" && msg.contains("fractional bound")),
        "a bound of [2.5] must be reported, got {found:?}"
    );
}

#[test]
fn an_unbounded_upper_limit_is_accepted() {
    let src = r#"
package P {
	part def Wheel;
	part def Car { part wheels : Wheel[1..*]; }
}
"#;
    assert!(codes(src).is_empty(), "`*` is a legal upper bound");
}

// -- validateMultiplicityRangeBoundResultTypes: values and types ----------

#[test]
fn a_boolean_literal_bound_is_rejected() {
    // `MultiplicityRange_invalid.kerml.xt:32`: `feature f [1..false];` — a Boolean is not a
    // whole number, so the rule's first clause rejects it without evaluating anything.
    let src = r#"
package P {
	feature f [1..false];
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, _, msg)| code == "SEM013" && msg.contains("not a whole number")),
        "a Boolean bound must be reported, got {found:?}"
    );
}

#[test]
fn a_string_literal_bound_is_rejected() {
    // `MultiplicityRange_invalid.kerml.xt:52`: `feature h ["x"..*];`
    let src = r#"
package P {
	feature h ["x"..*];
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, _, msg)| code == "SEM013" && msg.contains("not a whole number")),
        "a string bound must be reported, got {found:?}"
    );
}

#[test]
fn a_negative_literal_bound_is_rejected() {
    let src = r#"
package P {
	feature f [-1..2];
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, _, msg)| code == "SEM013" && msg.contains("negative bound")),
        "a negative bound must be reported, got {found:?}"
    );
}

#[test]
fn a_boolean_valued_name_bound_is_rejected() {
    // `MultiplicityRange_invalid.kerml.xt:37` and `:48`: the bound is a name whose value is a
    // comparison. The *value* may be out of reach; the *type* is not, because a comparison yields
    // a Boolean whatever its operands are.
    let src = r#"
package P {
	feature n = 0;
	feature b = n > 3;
	feature g1 [n..b];
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, _, msg)| code == "SEM013"
                && msg.contains("Boolean rather than a whole number")),
        "a Boolean-valued bound must be reported, got {found:?}"
    );
}

#[test]
fn a_name_that_evaluates_below_zero_is_rejected() {
    // `MultiplicityRange_invalid.kerml.xt:42`: `m1 = n - 1` with `n = 0` is -1, and the rule reads
    // "value <> null implies value >= 0". The upper bound `m2 = 2 - m1` is 3, legal even though it
    // arrives through arithmetic rather than as a literal.
    let src = r#"
package P {
	feature n = 0;
	feature m1 = n - 1;
	feature m2 = 2 - m1;
	feature g2 [m1..m2];
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, _, msg)| code == "SEM013" && msg.contains("negative bound `m1`")),
        "a bound that evaluates to -1 must be reported, got {found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|(code, _, msg)| code == "SEM013" && msg.contains("`m2`")),
        "`m2` is 3, a whole non-negative number, got {found:?}"
    );
}

#[test]
fn a_bound_this_layer_cannot_evaluate_is_accepted() {
    // The rule constrains a bound that evaluates. A name with no value of its own is out of reach,
    // and guessing would reject a model that may be right.
    let src = r#"
package P {
	attribute n : Integer;
	part def Wheel;
	part def Car { part wheels : Wheel[n..*]; }
}
"#;
    let found = issues(src);
    assert!(
        !found.iter().any(|(code, _, _)| code == "SEM013"),
        "a bound whose value is unknown must not be reported, got {found:?}"
    );
}

// -- validateFeatureEndNoDirection -------------------------------------------

#[test]
fn an_end_feature_with_a_direction_is_rejected() {
    let src = r#"
package Q {
	part def A;
	connection def C { end in a : A; }
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, _, msg)| code == "SEM014" && msg.contains("must not carry a direction")),
        "`end in a` must be reported, got {found:?}"
    );
}

#[test]
fn a_plain_end_feature_is_accepted() {
    let src = r#"
package Q {
	part def A;
	connection def C { end a : A; }
}
"#;
    assert!(
        codes(src).is_empty(),
        "`end a` carries no direction and is legal"
    );
}

// -- scalar value conformance (SEM015) ---------------------------------------
//
// This check has no counterpart in the specification's rule table: the metamodel assumes values
// conform and states no OCL constraint about it. It is kept out of `conformance::implemented()`
// for that reason, and tested here because a check with no failing-model test cannot be trusted.

#[test]
fn a_string_initialising_a_real_is_rejected() {
    let src = r#"
package P {
	private import ScalarValues::*;
	attribute b : Real = "text";
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, at, msg)| code == "SEM015" && at == "b" && msg.contains("a string")),
        "a string assigned to Real must be reported, got {found:?}"
    );
}

#[test]
fn a_real_initialising_an_integer_is_rejected() {
    let src = r#"
package P {
	private import ScalarValues::*;
	attribute c : Integer = 2.5;
}
"#;
    assert!(
        codes(src).iter().any(|c| c == "SEM015"),
        "2.5 assigned to Integer must be reported"
    );
}

#[test]
fn a_computed_integer_initialising_a_boolean_is_rejected() {
    // The check used to read the literal in front of it, so `1 + 2` was invisible: neither operand
    // is the value. The sum is an `Integer` because both operands are, and the type is what says
    // so — which is why this now reports where it used to pass.
    let src = r#"
package P {
	private import ScalarValues::*;
	attribute b : Boolean = 1 + 2;
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, at, msg)| code == "SEM015" && at == "b" && msg.contains("an integer")),
        "a sum of integers assigned to Boolean must be reported, got {found:?}"
    );
}

#[test]
fn a_computed_integer_initialising_a_real_is_accepted() {
    let src = r#"
package P {
	private import ScalarValues::*;
	attribute a : Real = 1 + 2;
}
"#;
    assert!(
        !codes(src).iter().any(|c| c == "SEM015"),
        "an integer sum is a Real, so it initialises one"
    );
}

#[test]
fn an_integer_initialising_a_real_is_accepted() {
    let src = r#"
package P {
	private import ScalarValues::*;
	attribute a : Real = 1;
}
"#;
    assert!(
        codes(src).is_empty(),
        "Integer specializes Real, so an integer literal conforms"
    );
}

#[test]
fn matching_scalar_literals_are_accepted() {
    let src = r#"
package P {
	private import ScalarValues::*;
	attribute d : String = "ok";
	attribute e : Boolean = true;
	attribute f : Real = 3.5;
}
"#;
    assert!(
        codes(src).is_empty(),
        "conforming literals must not be reported"
    );
}

#[test]
fn a_user_defined_type_named_like_a_scalar_is_not_judged() {
    let src = r#"
package P {
	part def Real;
	attribute def RealDef;
	attribute b : RealDef = "text";
}
"#;
    assert!(
        codes(src).is_empty(),
        "a project's own `Real` is not the library's, and its values are not judged"
    );
}

#[test]
fn an_untyped_value_is_not_judged() {
    let src = r#"
package P {
	attribute x = "text";
}
"#;
    assert!(
        codes(src).is_empty(),
        "nothing is declared, so nothing can be contradicted"
    );
}

// -- across the scalar and structural families ---------------------------------
//
// The literal check above answers "is this the right kind of number". The other half of the same
// question is "is this a number at all", and until these tests existed the answer was never asked:
// `attribute mass : Real = wheels` with `wheels` a part passed every gate, because the value is a
// reference rather than a literal and nothing compared the two types.

#[test]
fn a_part_initialising_a_scalar_attribute_is_rejected() {
    let src = r#"
package P {
	private import ScalarValues::*;
	part def Wheel;
	part wheels : Wheel;
	attribute mass : Real = wheels;
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, at, msg)| code == "SEM015" && at == "mass" && msg.contains("part def")),
        "a part assigned to a Real must be reported, got {found:?}"
    );
}

#[test]
fn a_number_initialising_a_part_is_rejected() {
    let src = r#"
package P {
	private import ScalarValues::*;
	part def Wheel;
	part spare : Wheel = 5;
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, at, _)| code == "SEM015" && at == "spare"),
        "a literal assigned to a part must be reported, got {found:?}"
    );
}

#[test]
fn a_part_initialising_a_part_is_accepted() {
    let src = r#"
package P {
	part def Wheel;
	part wheels : Wheel;
	part spare : Wheel = wheels;
}
"#;
    assert!(
        !codes(src).iter().any(|c| c == "SEM015"),
        "a part initialised with a part of the same type is legal"
    );
}

#[test]
fn a_part_of_another_type_initialising_a_part_is_rejected() {
    // The structural half of the same question. Before this, only the family was compared, so
    // `part spare : Wheel = car` with `car : Car` passed — the corpus states nothing about it and
    // nothing else looked.
    let src = r#"
package P {
	part def Wheel;
	part def Car;
	part car : Car;
	part spare : Wheel = car;
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, at, msg)| code == "SEM015" && at == "spare" && msg.contains("P::Car")),
        "a Car assigned to a Wheel must be reported, got {found:?}"
    );
}

#[test]
fn a_narrower_part_initialising_a_part_is_accepted() {
    // Narrowing is legal in every other type question this layer asks, and it is legal here too:
    // a value whose type specializes the declared one is an instance of it.
    let src = r#"
package P {
	part def Wheel;
	part def Turbo :> Wheel;
	part turbo : Turbo;
	part spare : Wheel = turbo;
}
"#;
    assert!(
        !codes(src).iter().any(|c| c == "SEM015"),
        "a Turbo initialises a Wheel"
    );
}

#[test]
fn a_wider_declared_type_is_accepted() {
    // The other direction is left alone on purpose. `AssembledVehicle :> VehicleAssembly` and
    // `out item assembledVehicle : AssembledVehicle = ref { part engine : Engine = …; }` is in the
    // official corpus, which states the model clean: the `= ref { … }` form adds the members that
    // make up the difference, and the reference implementation asks no question about it.
    let src = r#"
package P {
	item def VehicleAssembly;
	item def AssembledVehicle :> VehicleAssembly;
	item base : VehicleAssembly;
	item assembled : AssembledVehicle = base {
		part engine : Engine;
	}
	part def Engine;
}
"#;
    assert!(
        !codes(src).iter().any(|c| c == "SEM015"),
        "a wider declared type with a body is accepted"
    );
}

#[test]
fn an_unrelated_structural_value_is_rejected_across_kinds() {
    let src = r#"
package P {
	part def Wheel;
	item def Fuel;
	item fuel : Fuel;
	part spare : Wheel = fuel;
}
"#;
    let found = issues(src);
    assert!(
        found
            .iter()
            .any(|(code, at, _)| code == "SEM015" && at == "spare"),
        "an item assigned to a part must be reported, got {found:?}"
    );
}

#[test]
fn a_quantity_value_initialised_with_a_number_is_accepted() {
    // `LengthValue` specializes `ScalarQuantityValue`, which specializes `NumericalValue` and so
    // `ScalarValue`. It is an attribute definition, not one of the eleven names the literal check
    // knows, so the family has to come from the library rather than from a list of leaf names —
    // otherwise `attribute length : LengthValue = 5 [m]` would be called a part holding a number.
    let src = r#"
package P {
	private import ScalarValues::*;
	private import ISQ::*;
	attribute length : LengthValue = 3 [m];
}
"#;
    let found = issues(src);
    assert!(
        !found.iter().any(|(c, _, _)| c == "SEM015"),
        "a quantity value is a scalar, got {found:?}"
    );
}

#[test]
fn a_value_above_the_scalar_family_is_not_judged() {
    // `DataValue` is a supertype of `ScalarValue`, not a member of it: a structure and a number
    // both specialize it, so a feature typed by it says nothing about which one holds.
    let src = r#"
package P {
	private import ScalarValues::*;
	private import Base::*;
	part def Wheel;
	part wheels : Wheel;
	part any : DataValue = wheels;
	attribute n : DataValue = 5;
}
"#;
    let found = issues(src);
    assert!(
        !found.iter().any(|(c, _, _)| c == "SEM015"),
        "DataValue is not a scalar, got {found:?}"
    );
}

#[test]
fn a_value_with_no_family_to_compare_is_not_judged() {
    // With no library loaded there is no `ScalarValue` to be a member of, and the scalar side of
    // every question would be a guess. A model written against the bare kernel must not be judged
    // by a hierarchy it never imported.
    let src = r#"
package P {
	private import ScalarValues::*;
	part def Wheel;
	part wheels : Wheel;
	attribute mass : Real = wheels;
}
"#;
    let loaded = sysml_syntax::load_project(&[src], false).expect("model parses");
    let report = check(&loaded.roots, &loaded.library);
    let found: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.code == "SEM015")
        .collect();
    assert!(
        found.is_empty(),
        "no library, no family to compare against, got {found:?}"
    );
}

// -- the integrated clusters, end to end -------------------------------------
//
// The modules in `rules::` are also tested on their own, which proves their logic but not their
// wiring. The tests below go through `check`, so an entry in `conformance::implemented()` is backed
// by a diagnostic a user would actually see under the code the ledger names.

// validateFeatureEndNotDerivedAbstractCompositeOrPortion — SEM030
#[test]
fn an_end_carrying_a_forbidden_modifier_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		derived end wheels : Wheel;
	}
}
"#;
    let found = issues(src);
    assert!(
        found.iter().any(|(code, at, msg)| code == "SEM030"
            && at == "P::Fitting::wheels"
            && msg.contains("`derived`")),
        "`derived end` must be reported under SEM030, got {found:?}"
    );
}

// validateOccurrenceUsageIndividualUsage — SEM030
#[test]
fn an_individual_usage_of_a_plain_definition_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	part def Holder {
		individual part w : Wheel;
	}
}
"#;
    let found = issues(src);
    assert!(
        found.iter().any(|(code, at, msg)| code == "SEM030"
            && at == "P::Holder::w"
            && msg.contains("is not an individual definition")),
        "`individual part w : Wheel` must be reported under SEM030, got {found:?}"
    );
}

// validateOccurrenceUsagePortionKind — SEM030
#[test]
fn a_portion_kind_with_no_owning_type_is_rejected() {
    let src = r#"
package P {
	part def Engine;
	snapshot frozen : Engine;
}
"#;
    let found = issues(src);
    assert!(
        found.iter().any(|(code, at, msg)| code == "SEM030"
            && at == "P::frozen"
            && msg.contains("portion kind")),
        "a package-level `snapshot` must be reported under SEM030, got {found:?}"
    );
}

// validatePortUsageIsReference — SEM031
#[test]
fn a_composite_port_under_a_part_definition_is_rejected() {
    let src = r#"
package P {
	port def PD;
	part def Holder {
		composite port p : PD;
	}
}
"#;
    let found = issues(src);
    assert!(
        found.iter().any(|(code, at, msg)| code == "SEM031"
            && at == "P::Holder::p"
            && msg.contains("must be referential")),
        "a composite port must be reported under SEM031, got {found:?}"
    );
}

// validateAttributeUsageIsReference — SEM031
#[test]
fn a_composite_attribute_usage_is_rejected() {
    let src = r#"
package P {
	attribute def AD;
	part def Holder {
		composite attribute a : AD;
	}
}
"#;
    let found = issues(src);
    assert!(
        found.iter().any(|(code, at, msg)| code == "SEM031"
            && at == "P::Holder::a"
            && msg.contains("always referential")),
        "a composite attribute must be reported under SEM031, got {found:?}"
    );
}

// validateAttributeUsageFeatures — SEM031
#[test]
fn a_composite_feature_of_an_attribute_usage_is_rejected() {
    let src = r#"
package P {
	part def A;
	attribute def AD;
	part def Holder {
		attribute a : AD {
			composite part x : A;
		}
	}
}
"#;
    let found = issues(src);
    assert!(
        found.iter().any(|(code, at, msg)| code == "SEM031"
            && at == "P::Holder::a::x"
            && msg.contains("every feature of an attribute")),
        "a composite part inside an attribute usage must be reported under SEM031, got {found:?}"
    );
}

// validateAttributeDefinitionFeatures — SEM031
#[test]
fn a_composite_feature_of_an_attribute_definition_is_rejected() {
    let src = r#"
package P {
	part def A;
	attribute def AD {
		composite part x : A;
	}
}
"#;
    let found = issues(src);
    assert!(
        found.iter().any(|(code, at, msg)| code == "SEM031"
            && at == "P::AD::x"
            && msg.contains("every feature of an attribute")),
        "a composite part inside an attribute definition must be reported under SEM031, got \
         {found:?}"
    );
}

// validateFeatureEndMultiplicity — one rule, two codes
//
// `rules::feature_multiplicity` and `rules::type_multiplicity` both enforce this rule, under
// SEM021 and SEM033. Both are listed in `conformance::implemented()`, so a violating end carries
// two diagnostics. That duplication is a defect of the split, not of either check, and it is
// pinned here so it stays visible and is retired deliberately rather than discovered in a report.
#[test]
fn an_end_with_a_bound_other_than_one_one_is_reported_under_both_codes() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end wheels : Wheel[0..1];
	}
}
"#;
    let found = issues(src);
    let seen: Vec<&str> = found
        .iter()
        .filter(|(_, at, _)| at == "P::Fitting::wheels")
        .map(|(code, _, _)| code.as_str())
        .collect();
    assert!(
        seen.contains(&"SEM021") && seen.contains(&"SEM033"),
        "both modules enforce the rule today, got {found:?}"
    );
}

/// Every registered rule's code must appear in the source that enforces it.
///
/// The first two guards check that a registered rule has a *test*. They cannot tell a
/// registration from an implementation: a code listed in `implemented()` with no check anywhere
/// would satisfy both. This reads the crate's own source and requires each code to appear outside
/// `conformance.rs`, which is the file that lists it.
#[test]
fn every_enforced_rule_has_code_that_reports_it() {
    let mut source = String::new();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![dir];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "rs")
                && p.file_name() != Some("conformance.rs".as_ref())
            {
                source.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
            }
        }
    }

    // The code alone is too weak a check: several rules share one code (`SEM031` covers four),
    // so a registration whose rule is never mentioned anywhere still passes if any sibling code
    // appears. A `("SEM080", "validateAttributeDefinitionFeatures")` registered twice — once
    // implemented, once not — went unnoticed that way. The rule name is what must appear.
    let mut missing: Vec<(&str, &str)> = Vec::new();
    for (code, rule) in conformance::implemented() {
        if !source.contains(&format!("\"{code}\"")) || !source.contains(rule) {
            missing.push((code, rule));
        }
    }
    assert!(
        missing.is_empty(),
        "these rules are registered but no source in the crate names them, so nothing can enforce \
         them: {missing:?}"
    );
}

/// A numeric multiplicity bound is not a Boolean one.
///
/// The multiplicity rule asks whether a bound's type is Boolean. It called `.is_some()` on
/// `boolean_result`, so "known not to be a Boolean" counted as yes — invisible until a literal
/// could answer the question, at which point every `[n..b]` over a numeric bound was reported.
#[test]
fn a_numeric_bound_is_not_reported_as_a_boolean() {
    let src = r#"
package P {
    private import ScalarValues::*;
    part def S {
        attribute n : Integer = 1;
        attribute b : Integer = 2;
        part p [n..b];
    }
}
"#;
    assert!(
        !codes(src).iter().any(|c| c == "SEM013"),
        "an integer bound is a whole number, got {:?}",
        codes(src)
    );
}

// -- redefinition type conformance (SEM096) ----------------------------------
//
// The rule is not one of the 180: the metamodel assumes values conform and never says what would
// make them. It is checked here for the same reason SEM015 is — the ledger cannot hold it.

fn warnings(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_redefinition_typed_by_an_unrelated_definition_is_reported() {
    let src = r#"
package P {
	part def Engine;
	part def Wheel;
	part def Base {
		part engine : Engine;
	}
	part def Car :> Base {
		part :>> engine : Wheel;
	}
}
"#;
    let found = warnings(src);
    assert!(
        found.iter().any(|(code, at, msg)| code == "SEM096"
            && at == "P::Car::engine"
            && msg.contains("Wheel")),
        "a wheel is not an engine, got {found:?}"
    );
}

#[test]
fn a_redefinition_typed_by_a_subtype_is_accepted() {
    let src = r#"
package P {
	part def Engine;
	part def Turbo :> Engine;
	part def Base {
		part engine : Engine;
	}
	part def Car :> Base {
		part :>> engine : Turbo;
	}
}
"#;
    assert!(
        !warnings(src).iter().any(|(c, _, _)| c == "SEM096"),
        "narrowing to a subtype is what a redefinition is for"
    );
}

#[test]
fn a_redefinition_that_writes_no_type_is_accepted() {
    let src = r#"
package P {
	part def Engine;
	part def Base {
		part engine : Engine;
	}
	part def Car :> Base {
		part :>> engine;
	}
}
"#;
    assert!(
        !warnings(src).iter().any(|(c, _, _)| c == "SEM096"),
        "a redefinition with no type takes the one it replaces"
    );
}
