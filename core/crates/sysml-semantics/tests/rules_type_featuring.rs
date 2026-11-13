//! Failing-model tests for `validateFeatureMultiplicityDomain` (`SEM119`) and
//! `validateClassifierMultiplicityDomain` (`SEM120`), and for the `featuring of … by …;` statement
//! that both of them read.
//!
//! The reference rules are `KerMLValidator.xtend:576-580` and `:611-615`; the specification's OCL is
//! `multiplicity <> null implies multiplicity.featuringType->isEmpty()` for a classifier and
//! `multiplicity <> null implies multiplicity.featuringType = featuringType` for a feature.

use sysml_semantics::check;

/// The `SEM119` and `SEM120` findings in a model, as `(code, element)`.
fn domain_findings(src: &str) -> Vec<(String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.code == "SEM119" || issue.code == "SEM120")
        .map(|issue| (issue.code.to_string(), issue.at))
        .collect()
}

/// The syntax errors a model produces.
fn syntax_errors(src: &str) -> Vec<String> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .into_iter()
        .filter(|issue| issue.code == "SYN001")
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn a_classifiers_multiplicity_may_not_have_a_featuring_type() {
    let found = domain_findings(
        "package P {\n\tfeature X;\n\tclass C {\n\t\tmultiplicity M [0..1] { featuring of M by X; }\n\t}\n}\n",
    );
    assert!(
        found.iter().any(|(code, _)| code == "SEM120"),
        "a classifier multiplicity with a featuring type must be reported: {found:?}"
    );
}

#[test]
fn a_feature_multiplicity_with_a_further_featuring_type_is_reported_on_the_feature() {
    // `M` is featured by `C` through `f`, and by `X` through its own statement, so it has {C, X},
    // while `f` has {C}. The report names `f`.
    let found = domain_findings(
        "package P {\n\tfeature X;\n\tclass C {\n\t\tfeature f {\n\t\t\tmultiplicity M [0..1] { featuring of M by X; }\n\t\t}\n\t}\n}\n",
    );
    assert!(
        found
            .iter()
            .any(|(code, at)| code == "SEM119" && at.ends_with("::f")),
        "`f` must be reported under SEM119: {found:?}"
    );
}

#[test]
fn a_feature_multiplicity_takes_the_featuring_types_of_its_feature() {
    // `M` has no statement of its own, so it takes `{C}` from `f`, the same as `f` does.
    let found = domain_findings(
        "package P {\n\tclass C {\n\t\tfeature f {\n\t\t\tmultiplicity M [0..1];\n\t\t}\n\t}\n}\n",
    );
    assert!(found.is_empty(), "M and f both take {{C}}: {found:?}");
}

#[test]
fn a_classifier_multiplicity_with_no_featuring_is_not_reported() {
    let found = domain_findings("package P {\n\tclass C {\n\t\tmultiplicity M [0..1];\n\t}\n}\n");
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_standalone_featuring_can_name_a_classifier_multiplicity() {
    let found = domain_findings(
        "package P {\n\tfeature X;\n\tclass C {\n\t\tmultiplicity M [0..1];\n\t}\n\tfeaturing of C::M by X;\n}\n",
    );
    assert!(
        found.iter().any(|(code, _)| code == "SEM120"),
        "a standalone featuring of a classifier multiplicity must be reported: {found:?}"
    );
}

#[test]
fn a_featuring_with_an_identification_is_a_legal_statement() {
    let errors = syntax_errors(
        "package P {\n\tfeature X;\n\tclass C {\n\t\tmultiplicity M [0..1] { featuring f1 of M by X; }\n\t}\n}\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
}
