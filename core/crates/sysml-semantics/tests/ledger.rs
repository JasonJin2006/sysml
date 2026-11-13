//! The ledger's states: every rule is enforced, not applicable, in conflict, or blocked by a named
//! thing.
//!
//! `conformance::status` answers for one rule. These tests answer for all 180 at once: nothing is
//! unclassified, nothing is in two states, every blocker names something the tree is genuinely
//! missing — a metaclass from the metamodel table, or a derived property the second layer builds —
//! and every not-applicable reason says what kind of evidence it rests on.

use sysml_model::metaclass;
use sysml_semantics::conformance::{
    self, Status, BLOCKED_BY, CONFLICT, DERIVED_PROPERTIES, EVIDENCE, IMPLIED_FACTS,
    NOT_APPLICABLE, RULES,
};

fn stated(name: &str) -> bool {
    RULES.iter().any(|r| r.name == name)
}

#[test]
fn every_rule_has_exactly_one_state() {
    let mut unclassified = Vec::new();
    for rule in RULES {
        match conformance::status(rule.name) {
            Status::Unclassified => unclassified.push(rule.name),
            Status::Enforced(code) => {
                assert!(!code.is_empty(), "{} has an empty code", rule.name);
                assert!(
                    !BLOCKED_BY.iter().any(|(n, _)| *n == rule.name),
                    "{} is enforced and blocked",
                    rule.name
                );
                assert!(
                    !NOT_APPLICABLE.iter().any(|(n, _)| *n == rule.name),
                    "{} is enforced and not applicable",
                    rule.name
                );
            }
            Status::NotApplicable(why) => {
                assert!(!why.is_empty(), "{} gives no reason", rule.name);
                assert!(
                    !BLOCKED_BY.iter().any(|(n, _)| *n == rule.name),
                    "{} is not applicable and blocked",
                    rule.name
                );
            }
            Status::Conflict(why) => {
                assert!(!why.is_empty(), "{} records no disagreement", rule.name);
                assert!(
                    !BLOCKED_BY.iter().any(|(n, _)| *n == rule.name),
                    "{} is a conflict and blocked",
                    rule.name
                );
            }
            Status::Blocked(what) => {
                assert!(!what.is_empty(), "{} is blocked by nothing", rule.name);
            }
        }
    }
    assert!(
        unclassified.is_empty(),
        "rules with no state: {unclassified:?}"
    );
}

#[test]
fn every_blocker_names_a_layer_one_to_three_fact() {
    for (rule, what) in BLOCKED_BY {
        assert!(
            stated(rule),
            "`{rule}` is not a rule the specification states"
        );
        assert!(
            metaclass::is_metaclass(what)
                || DERIVED_PROPERTIES.contains(what)
                || IMPLIED_FACTS.contains(what),
            "`{rule}` is blocked by `{what}`, which is not a metaclass, a derived property, \
             or an implied fact"
        );
    }
}

#[test]
fn the_states_account_for_the_whole_table() {
    // Distinct rule names: two codes may report one rule (`validateFeatureEndMultiplicity` under
    // SEM021 and SEM033, `validateConnectorRelatedFeatures` under SEM001 and SEM002), and counting
    // entries would make the partition look over-full.
    let enforced: std::collections::BTreeSet<&str> =
        conformance::implemented().iter().map(|(_, n)| *n).collect();
    let covered = enforced.len() + NOT_APPLICABLE.len() + CONFLICT.len() + BLOCKED_BY.len();
    assert_eq!(
        covered,
        RULES.len(),
        "the states must partition the rule table: {} enforced + {} not applicable + {} conflict \
         + {} blocked",
        enforced.len(),
        NOT_APPLICABLE.len(),
        CONFLICT.len(),
        BLOCKED_BY.len()
    );
}

#[test]
fn every_not_applicable_reason_says_what_it_rests_on() {
    // The class is the ledger's own statement of how much a reason is worth: text a reader can
    // open, a fact about this tree, or another program's behaviour. A reason with no class, or a
    // class with no reason, would make the count of each meaningless.
    let named: std::collections::BTreeSet<&str> = NOT_APPLICABLE.iter().map(|(n, _)| *n).collect();
    let classed: std::collections::BTreeSet<&str> = EVIDENCE.iter().map(|(n, _)| *n).collect();
    let missing: Vec<_> = named.difference(&classed).collect();
    let extra: Vec<_> = classed.difference(&named).collect();
    assert!(
        missing.is_empty(),
        "reasons with no evidence class: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "evidence classes with no reason: {extra:?}"
    );
}

#[test]
fn every_rule_metaclass_is_in_the_metamodel_table() {
    // The two generated tables share one vocabulary: a rule is asked of a metaclass, and that name
    // has to exist in the hierarchy the metaclass table carries. A drift between them — a renamed
    // class, a rule extracted from a newer metamodel — fails here rather than in a silent `false`.
    for rule in RULES {
        assert!(
            metaclass::is_metaclass(rule.metaclass),
            "`{}` is stated of `{}`, which the metaclass table does not declare",
            rule.name,
            rule.metaclass
        );
    }
}

#[test]
fn every_ocliskindof_target_is_in_the_metamodel_table() {
    // Every metaclass the OCL asks about by name must be answerable by `Index::is_kind_of`, which
    // means it has to be in the table. The names are read out of the OCL text rather than listed
    // here, so a rule added from a newer metamodel brings its own check.
    let mut checked = 0;
    for rule in RULES {
        let ocl = rule.ocl;
        for marker in ["oclIsKindOf(", "selectByKind(", "oclAsType("] {
            let mut rest = ocl;
            while let Some(at) = rest.find(marker) {
                rest = &rest[at + marker.len()..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if name.is_empty() {
                    continue;
                }
                assert!(
                    metaclass::is_metaclass(&name),
                    "`{}` tests `{name}`, which the metaclass table does not declare",
                    rule.name
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 100,
        "the scan found only {checked} metaclass tests; it stopped matching the OCL"
    );
}
