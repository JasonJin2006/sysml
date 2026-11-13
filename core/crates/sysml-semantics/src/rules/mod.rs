//! The specification's well-formedness rules, one module per group of metaclasses.
//!
//! Each module decides the rules it lists and returns a [`Violation`] for every model element that
//! breaks one. A module may ship fewer rules than it was handed: a rule whose OCL reaches for
//! something the element tree does not carry cannot be decided here, and saying so is the correct
//! outcome — see `conformance::implemented()` for the list that is actually enforced.
//!
//! Modules are independent: each takes the element tree and the resolver index and returns
//! violations. That shape is what lets them be written and reviewed separately.

use sysml_model::Element;
use sysml_resolve::Index;

use crate::Severity;

pub mod annotation_ownership;
pub mod association_end;
pub mod association_ends;
pub mod binary_specialization;
pub mod binding_connector;
pub mod case_membership;
pub mod conjugation;
pub mod constraint_membership;
pub mod control_flow;
pub mod control_node_degree;
pub mod cross_feature_specialization;
pub mod cross_feature_type;
pub mod cross_subsetting;
pub mod declaration_shape;
pub mod element_filter;
pub mod feature_chain_conformance;
pub mod feature_chain_length;
pub mod feature_is_variable;
pub mod feature_modifiers;
pub mod feature_multiplicity;
pub mod feature_reference;
pub mod include_target;
pub mod individual_definition;
pub mod invocation;
pub mod invocation_parameters;
pub mod library_package;
pub mod member_duplicates;
pub mod membership_owners;
pub mod metadata;
pub mod metadata_abstract;
pub mod metadata_annotated_element;
pub mod metadata_body;
pub mod modifier_conformance;
pub mod multiplicity_domain;
pub mod owned_subsetting;
pub mod parallel_state;
pub mod part_type;
pub mod port_attribute;
pub mod port_direction;
pub mod port_nested_composite;
pub mod property_type;
pub mod redefinition_direction;
pub mod redefinition_owner;
pub mod redefinition_type;
pub mod reference_usage;
pub mod referential;
pub mod requirement_participants;
pub mod result_membership;
pub mod return_membership;
pub mod send_parameters;
pub mod specialization_kinds;
pub mod specialization_target;
pub mod state_subaction;
pub mod state_transition;
pub mod subset_uniqueness;
pub mod subsetting;
pub mod subsetting_multiplicity;
pub mod transition_trigger;
pub mod type_multiplicity;
pub mod type_relationships;
pub mod typed_by_definition;
pub mod variable_modifiers;
pub mod variant_membership;
pub mod variation_members;
pub mod variation_modifiers;
pub mod variation_rules;
pub mod variation_specialization;
pub mod verification_membership;
pub mod view_expose_import;
pub mod view_rendering;

/// One element that breaks one rule.
#[derive(Debug, Clone)]
pub struct Violation {
    /// The `SEM` code this kernel reports the rule under.
    pub code: &'static str,
    /// Qualified name of the element the violation is about.
    pub element: String,
    pub line: u32,
    pub message: String,
    /// Whether the finding is an error or a warning.
    ///
    /// A rule reports a warning when the model is legal but almost certainly wrong — the pilot
    /// implementation's "duplicate of inherited member name" is the model: a redeclaration that
    /// redefines what it shadows is legal, and one that does not is a name that resolves to two
    /// things. The distinction has to reach the report, because a caller that treats a legal model
    /// as broken will stop reading.
    pub severity: Severity,
}

impl Violation {
    pub fn new(
        code: &'static str,
        el: &Element,
        qualified: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            element: qualified.into(),
            line: el.line,
            message: message.into(),
            severity: Severity::Error,
        }
    }

    /// The same finding, reported as a warning because the model is legal.
    pub fn warning(
        code: &'static str,
        el: &Element,
        qualified: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            element: qualified.into(),
            line: el.line,
            message: message.into(),
            severity: Severity::Warning,
        }
    }
}

/// Every violation from every module, in module order.
pub fn all(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    out.extend(association_end::issues(roots, index));
    out.extend(association_ends::issues(roots, index));
    out.extend(binary_specialization::issues(roots, index));
    out.extend(binding_connector::issues(roots, index));
    out.extend(case_membership::issues(roots, index));
    out.extend(constraint_membership::issues(roots, index));
    out.extend(cross_feature_type::issues(roots, index));
    out.extend(cross_feature_specialization::issues(roots, index));
    out.extend(cross_subsetting::issues(roots, index));
    out.extend(control_flow::issues(roots, index));
    out.extend(declaration_shape::issues(roots, index));
    out.extend(library_package::issues(roots, index));
    out.extend(property_type::issues(roots, index));
    out.extend(control_node_degree::issues(roots, index));
    out.extend(feature_multiplicity::issues(roots, index));
    out.extend(feature_modifiers::issues(roots, index));
    out.extend(include_target::issues(roots, index));
    out.extend(feature_chain_length::issues(roots, index));
    out.extend(feature_chain_conformance::issues(roots, index));
    out.extend(feature_reference::issues(roots, index));
    out.extend(feature_is_variable::issues(roots, index));
    out.extend(individual_definition::issues(roots, index));
    out.extend(invocation::issues(roots, index));
    out.extend(invocation_parameters::issues(roots, index));
    out.extend(member_duplicates::issues(roots, index));
    out.extend(membership_owners::issues(roots, index));
    out.extend(metadata::issues(roots, index));
    out.extend(metadata_body::issues(roots, index));
    out.extend(metadata_abstract::issues(roots, index));
    out.extend(metadata_annotated_element::issues(roots, index));
    out.extend(conjugation::issues(roots, index));
    out.extend(port_direction::issues(roots, index));
    out.extend(owned_subsetting::issues(roots, index));
    out.extend(modifier_conformance::issues(roots, index));
    out.extend(part_type::issues(roots, index));
    out.extend(parallel_state::issues(roots, index));
    out.extend(port_attribute::issues(roots, index));
    out.extend(port_nested_composite::issues(roots, index));
    out.extend(redefinition_direction::issues(roots, index));
    out.extend(redefinition_type::issues(roots, index));
    out.extend(redefinition_owner::issues(roots, index));
    out.extend(reference_usage::issues(roots, index));
    out.extend(requirement_participants::issues(roots, index));
    out.extend(annotation_ownership::issues(roots, index));
    out.extend(multiplicity_domain::issues(roots, index));
    out.extend(result_membership::issues(roots, index));
    out.extend(return_membership::issues(roots, index));
    out.extend(send_parameters::issues(roots, index));
    out.extend(referential::issues(roots, index));
    out.extend(specialization_kinds::issues(roots, index));
    out.extend(specialization_target::issues(roots, index));
    out.extend(state_subaction::issues(roots, index));
    out.extend(element_filter::issues(roots, index));
    out.extend(state_transition::issues(roots, index));
    out.extend(subset_uniqueness::issues(roots, index));
    out.extend(subsetting_multiplicity::issues(roots, index));
    out.extend(subsetting::issues(roots, index));
    out.extend(transition_trigger::issues(roots, index));
    out.extend(type_multiplicity::issues(roots, index));
    out.extend(type_relationships::issues(roots, index));
    out.extend(typed_by_definition::issues(roots, index));
    out.extend(variable_modifiers::issues(roots, index));
    out.extend(verification_membership::issues(roots, index));
    out.extend(variant_membership::issues(roots, index));
    out.extend(variation_modifiers::issues(roots, index));
    out.extend(variation_members::issues(roots, index));
    out.extend(variation_specialization::issues(roots, index));
    out.extend(variation_rules::issues(roots, index));
    out.extend(view_expose_import::issues(roots, index));
    out.extend(view_rendering::issues(roots, index));
    out
}

/// Walk every element of every root, depth first.
pub fn walk<'a>(roots: &'a [Element], mut visit: impl FnMut(&'a Element)) {
    fn go<'a>(el: &'a Element, visit: &mut impl FnMut(&'a Element)) {
        visit(el);
        for child in &el.children {
            // `inherit_ports` copies a definition's ports onto a usage typed by it so a view can
            // draw them. Those copies are marked `inherited`, and they are the same authored
            // element the definition already carries: visiting them again reports every port
            // twice and lets a rule judge a member the usage never wrote.
            if child.inherited {
                continue;
            }
            go(child, visit);
        }
    }
    for root in roots {
        go(root, &mut visit);
    }
}
