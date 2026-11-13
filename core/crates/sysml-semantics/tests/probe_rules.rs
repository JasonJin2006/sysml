//! Where a check spends its time, per rule module.
//!
//! This is the instrument behind the distribution in docs/performance.md. Run it against a large
//! model and it prints one line per rule, the whole check, and the price of the lookup every
//! inheritance rule leans on.
//!
//!
//! Generate a model, then point the probe at it:
//!
//!     python -c "import sys; sys.path.insert(0, 'core/tools'); import bench, pathlib;
//!                pathlib.Path('big.sysml').write_text(bench.model_of(10000))"
//!     SYSML_PROBE_MODEL=big.sysml cargo test --release -p sysml-semantics \
//!         --test probe_rules -- --ignored --nocapture
//!
//! Ignored rather than an ordinary test: it needs a model it cannot invent and half a minute to run,
//! and a gate that measured time would fail on a loaded machine rather than on a defect. It lives
//! here so the next question about cost starts from a measurement instead of a guess.

use std::time::Instant;

use sysml_model::Element;
use sysml_resolve::Index;

fn time(f: impl FnOnce()) -> f64 {
    let start = Instant::now();
    f();
    start.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "profile probe: set SYSML_PROBE_MODEL and run with --ignored --nocapture"]
fn profile_rules() {
    let path = std::env::var("SYSML_PROBE_MODEL").expect("SYSML_PROBE_MODEL names a .sysml file");
    let text = std::fs::read_to_string(&path).expect("read model");
    let loaded = sysml_syntax::load_project(&[&text], true).expect("parses");

    let start = Instant::now();
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    eprintln!(
        "{:>8.1}ms  Index::build",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let start = Instant::now();
    let checked = sysml_semantics::check(&loaded.roots, &loaded.library);
    eprintln!(
        "{:>8.1}ms  check total ({} issues)",
        start.elapsed().as_secs_f64() * 1000.0,
        checked.issues.len()
    );

    // The lookup every inheritance rule leans on, priced on its own. A name that does not exist is
    // the worst case, and every rule that resolves a written clause pays it.
    let ids: Vec<usize> = (0..index.len()).collect();
    let lookup = (0..3)
        .map(|_| {
            time(|| {
                for &id in &ids {
                    std::hint::black_box(index.resolve(id, "Value"));
                }
            })
        })
        .fold(f64::INFINITY, f64::min);
    eprintln!(
        "{lookup:>8.1}ms  resolve, one name per element ({:.1}us each)",
        lookup * 1000.0 / ids.len().max(1) as f64
    );

    // Each module three times, keeping the best: the machine is shared, and one run of one rule is
    // not a measurement.
    let modules: [(
        &str,
        fn(&[Element], &Index) -> Vec<sysml_semantics::rules::Violation>,
    ); 64] = [
        (
            "association_end",
            sysml_semantics::rules::association_end::issues,
        ),
        (
            "association_ends",
            sysml_semantics::rules::association_ends::issues,
        ),
        (
            "binary_specialization",
            sysml_semantics::rules::binary_specialization::issues,
        ),
        (
            "binding_connector",
            sysml_semantics::rules::binding_connector::issues,
        ),
        (
            "case_membership",
            sysml_semantics::rules::case_membership::issues,
        ),
        (
            "constraint_membership",
            sysml_semantics::rules::constraint_membership::issues,
        ),
        (
            "cross_feature_type",
            sysml_semantics::rules::cross_feature_type::issues,
        ),
        (
            "cross_feature_specialization",
            sysml_semantics::rules::cross_feature_specialization::issues,
        ),
        (
            "cross_subsetting",
            sysml_semantics::rules::cross_subsetting::issues,
        ),
        ("control_flow", sysml_semantics::rules::control_flow::issues),
        (
            "declaration_shape",
            sysml_semantics::rules::declaration_shape::issues,
        ),
        (
            "control_node_degree",
            sysml_semantics::rules::control_node_degree::issues,
        ),
        (
            "feature_multiplicity",
            sysml_semantics::rules::feature_multiplicity::issues,
        ),
        (
            "feature_modifiers",
            sysml_semantics::rules::feature_modifiers::issues,
        ),
        (
            "include_target",
            sysml_semantics::rules::include_target::issues,
        ),
        (
            "feature_chain_length",
            sysml_semantics::rules::feature_chain_length::issues,
        ),
        (
            "feature_chain_conformance",
            sysml_semantics::rules::feature_chain_conformance::issues,
        ),
        (
            "feature_reference",
            sysml_semantics::rules::feature_reference::issues,
        ),
        (
            "feature_is_variable",
            sysml_semantics::rules::feature_is_variable::issues,
        ),
        (
            "individual_definition",
            sysml_semantics::rules::individual_definition::issues,
        ),
        ("invocation", sysml_semantics::rules::invocation::issues),
        (
            "invocation_parameters",
            sysml_semantics::rules::invocation_parameters::issues,
        ),
        (
            "member_duplicates",
            sysml_semantics::rules::member_duplicates::issues,
        ),
        (
            "membership_owners",
            sysml_semantics::rules::membership_owners::issues,
        ),
        ("metadata", sysml_semantics::rules::metadata::issues),
        (
            "metadata_body",
            sysml_semantics::rules::metadata_body::issues,
        ),
        (
            "metadata_abstract",
            sysml_semantics::rules::metadata_abstract::issues,
        ),
        (
            "metadata_annotated_element",
            sysml_semantics::rules::metadata_annotated_element::issues,
        ),
        ("conjugation", sysml_semantics::rules::conjugation::issues),
        (
            "port_direction",
            sysml_semantics::rules::port_direction::issues,
        ),
        (
            "owned_subsetting",
            sysml_semantics::rules::owned_subsetting::issues,
        ),
        (
            "modifier_conformance",
            sysml_semantics::rules::modifier_conformance::issues,
        ),
        ("part_type", sysml_semantics::rules::part_type::issues),
        (
            "parallel_state",
            sysml_semantics::rules::parallel_state::issues,
        ),
        (
            "port_attribute",
            sysml_semantics::rules::port_attribute::issues,
        ),
        (
            "port_nested_composite",
            sysml_semantics::rules::port_nested_composite::issues,
        ),
        (
            "redefinition_direction",
            sysml_semantics::rules::redefinition_direction::issues,
        ),
        (
            "redefinition_owner",
            sysml_semantics::rules::redefinition_owner::issues,
        ),
        (
            "reference_usage",
            sysml_semantics::rules::reference_usage::issues,
        ),
        (
            "requirement_participants",
            sysml_semantics::rules::requirement_participants::issues,
        ),
        (
            "result_membership",
            sysml_semantics::rules::result_membership::issues,
        ),
        (
            "return_membership",
            sysml_semantics::rules::return_membership::issues,
        ),
        ("referential", sysml_semantics::rules::referential::issues),
        (
            "specialization_kinds",
            sysml_semantics::rules::specialization_kinds::issues,
        ),
        (
            "specialization_target",
            sysml_semantics::rules::specialization_target::issues,
        ),
        (
            "state_subaction",
            sysml_semantics::rules::state_subaction::issues,
        ),
        (
            "element_filter",
            sysml_semantics::rules::element_filter::issues,
        ),
        (
            "state_transition",
            sysml_semantics::rules::state_transition::issues,
        ),
        (
            "subset_uniqueness",
            sysml_semantics::rules::subset_uniqueness::issues,
        ),
        (
            "subsetting_multiplicity",
            sysml_semantics::rules::subsetting_multiplicity::issues,
        ),
        ("subsetting", sysml_semantics::rules::subsetting::issues),
        (
            "transition_trigger",
            sysml_semantics::rules::transition_trigger::issues,
        ),
        (
            "type_multiplicity",
            sysml_semantics::rules::type_multiplicity::issues,
        ),
        (
            "type_relationships",
            sysml_semantics::rules::type_relationships::issues,
        ),
        (
            "typed_by_definition",
            sysml_semantics::rules::typed_by_definition::issues,
        ),
        (
            "variable_modifiers",
            sysml_semantics::rules::variable_modifiers::issues,
        ),
        (
            "verification_membership",
            sysml_semantics::rules::verification_membership::issues,
        ),
        (
            "variant_membership",
            sysml_semantics::rules::variant_membership::issues,
        ),
        (
            "variation_modifiers",
            sysml_semantics::rules::variation_modifiers::issues,
        ),
        (
            "variation_members",
            sysml_semantics::rules::variation_members::issues,
        ),
        (
            "variation_specialization",
            sysml_semantics::rules::variation_specialization::issues,
        ),
        (
            "variation_rules",
            sysml_semantics::rules::variation_rules::issues,
        ),
        (
            "view_expose_import",
            sysml_semantics::rules::view_expose_import::issues,
        ),
        (
            "view_rendering",
            sysml_semantics::rules::view_rendering::issues,
        ),
    ];
    let mut rows: Vec<(&str, f64)> = Vec::new();
    for (name, run) in modules {
        let ms = (0..3)
            .map(|_| {
                time(|| {
                    let _ = run(&loaded.roots, &index);
                })
            })
            .fold(f64::INFINITY, f64::min);
        rows.push((name, ms));
    }
    rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = rows.iter().map(|(_, ms)| ms).sum();
    eprintln!("--- per module, sum {sum:.1}ms ---");
    for (name, ms) in &rows {
        eprintln!("{ms:>8.1}ms  {name}");
    }
}
