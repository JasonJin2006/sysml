//! Guards for the `EXTENDED` ledger — the rules whose authority is the reference, not the 180.
//!
//! The ledger exists because a kernel that only counted the specification's own rules would report a
//! completeness it does not have: the reference rejects models under rules it never cites to an OCL
//! constraint, and a tool that ignored them accepts models the reference refuses.
//!
//! The guards here are mostly about *honesty*, not correctness. A ledger that drifts from the code
//! (a rule renamed, a report code changed, an entry deleted) still compiles and still prints a
//! confident number. Each test below closes one way that can happen.

use sysml_semantics::conformance::{extended_coverage, EXTENDED};

#[test]
fn every_entry_has_a_basis() {
    // Without a basis the entry is an assertion of authority with nothing behind it — exactly the
    // thing the ledger exists to prevent being said about a rule.
    for rule in EXTENDED {
        assert!(
            !rule.basis.trim().is_empty(),
            "`{}` records no basis",
            rule.name
        );
        assert!(
            rule.basis.len() > 30,
            "`{}` has a basis too short to check: {:?}",
            rule.name,
            rule.basis
        );
    }
}

#[test]
fn every_entry_says_what_goes_wrong_if_it_is_wrong() {
    // The field is what lets a reader decide whether to trust a rule they disagree with. An entry
    // that omits it asks for trust without offering a way to withhold it.
    for rule in EXTENDED {
        assert!(
            !rule.if_wrong.trim().is_empty(),
            "`{}` does not say what breaks if it is wrong",
            rule.name
        );
    }
}

#[test]
fn the_names_are_unique() {
    // A duplicate hides one entry behind another in any map built from this table, and the counts
    // would still add up.
    let mut seen = std::collections::BTreeSet::new();
    for rule in EXTENDED {
        assert!(seen.insert(rule.name), "`{}` appears twice", rule.name);
    }
}

#[test]
fn an_incomplete_entry_says_what_is_missing() {
    // `incomplete` is a string field, so "marked incomplete but says nothing" is representable. It
    // would read as a rule that is nearly done rather than as one with an undescribed gap. The
    // guard is that a non-empty marker must be a sentence, which is what carries the information.
    for rule in EXTENDED {
        if rule.incomplete.trim().is_empty() {
            continue;
        }
        assert!(
            rule.incomplete.len() > 30,
            "`{}` is marked incomplete but does not say what is missing: {:?}",
            rule.name,
            rule.incomplete
        );
    }
}

#[test]
fn the_coverage_count_agrees_with_the_table() {
    let coverage = extended_coverage();
    assert_eq!(coverage.total, EXTENDED.len());
    assert_eq!(coverage.complete + coverage.incomplete, coverage.total);
    let counted = EXTENDED
        .iter()
        .filter(|r| r.incomplete.trim().is_empty())
        .count();
    assert_eq!(coverage.complete, counted);
}

#[test]
fn no_extended_rule_is_also_counted_as_enforced() {
    // The two ledgers answer different questions — "what the specification states" and "what the
    // reference does" — so a rule in both would be counted twice by anyone reading the two numbers
    // together, and would inflate the conformance figure with work the specification never asked
    // for. The names are compared after dropping the parenthetical qualifiers this ledger uses to
    // distinguish halves of a split rule.
    let enforced: std::collections::BTreeSet<&str> = sysml_semantics::conformance::implemented()
        .iter()
        .map(|(_, rule)| *rule)
        .collect();
    for rule in EXTENDED {
        let bare = rule
            .name
            .split_whitespace()
            .next()
            .unwrap_or(rule.name)
            .trim_end_matches('_');
        if bare.is_empty() || bare == "—" {
            continue;
        }
        assert!(
            !enforced.contains(bare),
            "`{}` is in both the enforced list and the extended ledger",
            bare
        );
    }
}

#[test]
fn the_families_that_were_measured_to_be_outside_the_180_are_present() {
    // These are the entries whose absence would be a regression in the ledger rather than in the
    // kernel: each was established by measurement, and dropping one would quietly shrink the count
    // of rules the tool admits it enforces on non-normative authority.
    let names: Vec<&str> = EXTENDED.iter().map(|r| r.name).collect();
    for wanted in [
        "validateUsageType_",
        "validateNamespaceDistinguishability",
        "validateFeatureParameterRedefinition",
        "validateBindingConnectorTypeConformance",
        "deriveUsageMayTimeVary",
    ] {
        assert!(
            names.iter().any(|name| name.starts_with(wanted)),
            "the ledger no longer records `{wanted}`; names are {names:?}"
        );
    }
}
