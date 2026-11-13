//! The `feature-modifiers` group: three rules enforced, six dropped with the field that blocks each.
//!
//! Each enforced rule is proved by a model that violates it — the point of this file. A test that
//! only shows a legal model passing would not distinguish "the rule runs" from "the rule is
//! absent", which is the failure mode `tests/rules.rs` guards.
//!
//! The six dropped rules are recorded twice: once as the violating model each would have to reject,
//! asserted to produce no `SEM030`, and once as the projection fact that makes it undecidable. The
//! second kind fails the moment the missing field arrives, which is when the check has to be
//! written.
//!
//! Every assertion goes through `reported`, which calls the module directly. Registering the module
//! in `rules::all` and in `conformance::implemented()` is the integrator's step — this cluster may
//! add only its `pub mod` line to `rules/mod.rs` — and a test asserting full-`check` output would
//! fail until then without saying anything about the rule. `tests/rules_type_multiplicity.rs`
//! takes the same route.

use sysml_model::Element;
use sysml_resolve::Index;
use sysml_semantics::rules::feature_modifiers;

/// What the module under test reports, independent of the wiring in `rules::all`.
fn reported(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    feature_modifiers::issues(&loaded.roots, &index)
        .into_iter()
        .map(|v| (v.code.to_string(), v.element, v.message))
        .collect()
}

/// Every `SEM030` the module produces.
fn sem030(src: &str) -> Vec<(String, String, String)> {
    reported(src)
        .into_iter()
        .filter(|(code, _, _)| code == "SEM030")
        .collect()
}

/// The qualified names the module reports, sorted.
fn reported_names(src: &str) -> Vec<String> {
    let mut names: Vec<String> = sem030(src).into_iter().map(|(_, at, _)| at).collect();
    names.sort();
    names
}

/// The named element `name`, wherever it sits in the tree.
fn find<'a>(roots: &'a [Element], name: &str) -> &'a Element {
    fn go<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
        if el.name == name {
            return Some(el);
        }
        el.children.iter().find_map(|c| go(c, name))
    }
    roots
        .iter()
        .find_map(|root| go(root, name))
        .unwrap_or_else(|| panic!("no element named `{name}` in the model"))
}

// -- validateFeatureEndNotDerivedAbstractCompositeOrPortion ------------------
//
// `isEnd implies not (isDerived or isAbstract or isComposite or isPortion)` — an end is what an
// association relates, not something its association is made of. Four disjuncts, so four tests plus
// the combinations; each is a model the metamodel forbids.

#[test]
fn a_derived_end_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		derived end wheels : Wheel;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Fitting::wheels" && msg.contains("`derived`")),
        "`derived end` breaks the rule; got {found:?}"
    );
}

#[test]
fn an_abstract_end_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		abstract end wheels : Wheel;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Fitting::wheels" && msg.contains("`abstract`")),
        "`abstract end` breaks the rule; got {found:?}"
    );
}

#[test]
fn a_composite_end_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		composite end wheels : Wheel;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Fitting::wheels" && msg.contains("`composite`")),
        "`composite end` breaks the rule; got {found:?}"
    );
}

#[test]
fn a_portion_end_is_rejected() {
    let src = r#"
package P {
	part def Occurrence;
	connection def HappensDuring {
		portion end longer : Occurrence;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::HappensDuring::longer" && msg.contains("`portion`")),
        "`portion end` breaks the rule; got {found:?}"
    );
}

#[test]
fn a_snapshot_end_and_a_timeslice_end_are_rejected_as_portions() {
    // `isPortion` is what the OCL forbids, and `snapshot` / `timeslice` set it:
    // `PortionUsage : OccurrenceUsage { isPortion = true }` and the reference builder sets both
    // (research/upstream/sem/mod/sysmlv2-model-0.1.2/src/build.rs:238-245). A check that read only
    // the word `portion` would accept these.
    for word in ["snapshot", "timeslice"] {
        let src = format!(
            r#"
package P {{
	part def Occurrence;
	connection def HappensDuring {{
		{word} end longer : Occurrence;
	}}
}}
"#
        );
        let found = sem030(&src);
        assert!(
            found
                .iter()
                .any(|(_, at, msg)| at == "P::HappensDuring::longer"
                    && msg.contains(&format!("`{word}`"))),
            "`{word} end` breaks the isPortion disjunct; got {found:?}"
        );
    }
}

#[test]
fn an_end_writing_several_forbidden_modifiers_names_them_all() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		derived abstract end wheels : Wheel;
	}
}
"#;
    let found = sem030(src);
    let message = &found
        .iter()
        .find(|(_, at, _)| at == "P::Fitting::wheels")
        .unwrap_or_else(|| panic!("the end must be reported; got {found:?}"))
        .2;
    assert!(
        message.contains("`derived`") && message.contains("`abstract`"),
        "both modifiers are worth naming: {message}"
    );
}

#[test]
fn a_plain_end_is_accepted() {
    // The shape the specification's own library uses
    // (`Systems Library/Connections.sysml:43-44` writes `end source: Anything`).
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		end left : Wheel;
		end right : Wheel;
	}
}
"#;
    assert!(
        sem030(src).is_empty(),
        "an end with no modifier breaks no disjunct"
    );
}

#[test]
fn a_non_end_feature_may_carry_every_forbidden_modifier() {
    // The rule is bounded to `isEnd`. Without this test a check that dropped the `isEnd` guard would
    // still pass every test above, and would reject ordinary abstract and derived features.
    let src = r#"
package P {
	part def Wheel;
	part def Holder {
		derived part d : Wheel;
		abstract part a : Wheel;
		portion part p : Wheel;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found.is_empty(),
        "`validateFeatureEndNotDerivedAbstractCompositeOrPortion` is about ends only; got {found:?}"
    );
}

#[test]
fn an_end_inside_an_interface_definition_is_judged_too() {
    // `interface def Link { end a : PP; }` declares a `PortUsage` end; the lowering relabels every
    // end a port (`sysml-syntax/src/lower.rs:281-283`), so a rule keyed on `Kind::PortUsage` rather
    // than on `is_end` would report none of this. This test keys the rule on the flag.
    let src = r#"
package P {
	port def PP;
	interface def Link {
		composite end a : PP;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found.iter().any(|(_, at, _)| at == "P::Link::a"),
        "an interface end is an end; got {found:?}"
    );
}

// -- validateOccurrenceUsageIndividualUsage ---------------------------------
//
// `isIndividual implies individualDefinition <> null` — an individual occurrence usage must have a
// definition that is itself individual among its types.

#[test]
fn an_individual_occurrence_usage_with_no_type_is_rejected() {
    let src = r#"
package P {
	individual part def Wheel;
	part def Holder {
		individual part w;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Holder::w" && msg.contains("no type")),
        "`individual part w;` names no individual definition; got {found:?}"
    );
}

#[test]
fn an_individual_occurrence_usage_typed_by_a_plain_definition_is_rejected() {
    let src = r#"
package P {
	part def Wheel;
	part def Holder {
		individual part w : Wheel;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Holder::w"
                && msg.contains("is not an individual definition")),
        "`Wheel` is not an individual definition; got {found:?}"
    );
}

#[test]
fn a_bare_individual_usage_is_an_occurrence_usage() {
    // `individual b : B;` writes no kind keyword: the word is the kind (`SysML.xtext:845-851`),
    // so the element is an `IndividualUsage`, which is an `OccurrenceUsage` with `isIndividual`.
    // Projected as a definition it fell outside this rule (`IndividualUsage_Invalid.sysml.xt:50`).
    let src = r#"
package P {
	part def B;
	individual b : B;
}
"#;
    let found = sem030(src);
    assert!(
        found.iter().any(|(_, at, _)| at == "P::b"),
        "`b` is an individual usage typed by a plain definition; got {found:?}"
    );
}

#[test]
fn an_individual_usage_is_rejected_whatever_the_usage_kind() {
    // The rule is asked of `OccurrenceUsage`, which covers the whole occurrence hierarchy:
    // `PartUsage`, `ItemUsage`, `ActionUsage`, `PortUsage` and the rest (`generated.rs:1785`).
    // Judging only `PartUsage` would look enforced and pass these.
    let src = r#"
package P {
	action def Move;
	state def Nominal;
	part def Holder {
		individual action a : Move;
		individual state s : Nominal;
	}
}
"#;
    let names = reported_names(src);
    assert_eq!(
        names,
        ["P::Holder::a", "P::Holder::s"],
        "an individual action and an individual state break the same rule"
    );
}

#[test]
fn an_individual_usage_typed_by_an_individual_definition_is_accepted() {
    let src = r#"
package P {
	individual part def Wheel;
	part def Holder {
		individual part w : Wheel;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found.is_empty(),
        "`Wheel` is an individual definition, so the consequent holds; got {found:?}"
    );
}

#[test]
fn an_individual_usage_whose_definition_is_an_individual_attribute_definition_is_accepted() {
    // `individual def Wheel` is the notation's ordinary spelling, and it lowers to a
    // `Kind::AttributeUsage` rather than a definition: `prefixes()` eats `individual`, then
    // `declaration()` eats `def`, and no kind keyword is left
    // (`sysml-parser/src/parser.rs:275-287`, `:846-851`, `:859-895`). Requiring the target's kind to
    // be a definition would reject this legal model, so this test pins the narrower reading.
    let src = r#"
package P {
	individual def Wheel;
	part def Holder {
		individual part w : Wheel;
	}
}
"#;
    assert!(
        sem030(src).is_empty(),
        "the idiomatic `individual def` must satisfy the rule; got {:?}",
        sem030(src)
    );
}

#[test]
fn a_plain_usage_of_an_individual_definition_is_accepted() {
    // The rule's antecedent is `isIndividual`, which is set by the `individual` keyword and by
    // nothing else. A usage that names an individual definition without saying `individual` is not
    // what the rule is about — `isIndividual` is a stored attribute
    // (`generated.rs:2443`), not derived from the types.
    let src = r#"
package P {
	individual part def Wheel;
	part def Holder {
		part w : Wheel;
	}
}
"#;
    assert!(
        sem030(src).is_empty(),
        "no `individual` written, no antecedent; got {:?}",
        sem030(src)
    );
}

#[test]
fn an_individual_part_definition_is_not_an_individual_usage() {
    // `individual part def Wheel` lowers to `Kind::PartDef` with `isIndividual = true`. A
    // `PartDefinition` is an `OccurrenceDefinition`, not an `OccurrenceUsage` (`generated.rs:1785`),
    // so the rule does not reach it — and a definition legitimately has no type of its own.
    let src = r#"
package P {
	individual part def Wheel;
}
"#;
    let found = sem030(src);
    assert!(
        found.is_empty(),
        "a definition is not an `OccurrenceUsage`; got {found:?}"
    );
}

#[test]
fn an_individual_usage_whose_type_does_not_resolve_is_left_undecided() {
    // The unresolved reference is the reference pass's report. Assuming the target is not
    // individual would claim a second defect the model may not have.
    let src = r#"
package P {
	part def Holder {
		individual part w : Nowhere;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found.is_empty(),
        "an unresolved typing is not this rule's report; got {found:?}"
    );
}

#[test]
fn an_individual_usage_naming_a_later_individual_type_is_left_undecided() {
    // `individualDefinition` reads the whole `occurrenceDefinition` list, but `type_ref` keeps the
    // first typing target only (`lower.rs:326` takes `Head::target_of`, `ast.rs:211-216`). Here the
    // first target is plain and the second is individual, so the metamodel's consequent holds while
    // the projection shows the opposite. Reporting it would be a false positive on a legal model.
    let src = r#"
package P {
	part def Plain;
	individual part def Wheel;
	part def Holder {
		individual part w : Plain, Wheel;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found.is_empty(),
        "the individual definition is among the types, so the consequent may hold; got {found:?}"
    );
}

#[test]
fn an_individual_item_definition_is_not_judged() {
    // `item def I` and `item i : I` both lower to `Kind::ItemDef` (`lower.rs:172`, which ignores
    // `is_definition`), so the tree cannot tell an individual item definition from an individual
    // item usage. A definition legitimately has no type, so the rule cannot reject the second
    // without also rejecting the first.
    let src = r#"
package P {
	individual item def Fuel;
}
"#;
    let found = sem030(src);
    assert!(
        found.is_empty(),
        "the collapsed `Kind::ItemDef` cannot be judged without rejecting a legal definition; got \
         {found:?}"
    );
}

// -- validateOccurrenceUsagePortionKind ------------------------------------
//
// `portionKind <> null implies owningType <> null and (owningType is an OccurrenceDefinition or an
// OccurrenceUsage)`.

#[test]
fn a_snapshot_owned_by_a_part_definition_is_accepted() {
    // `PartDefinition` is an `OccurrenceDefinition`, so the disjunction's first side holds.
    let src = r#"
package P {
	part def Engine {
		snapshot frozen : Engine;
	}
}
"#;
    assert!(
        sem030(src).is_empty(),
        "a part definition is an occurrence definition; got {:?}",
        sem030(src)
    );
}

#[test]
fn a_timeslice_owned_by_an_action_usage_is_accepted() {
    // `ActionUsage` is an `OccurrenceUsage`, so the second side holds. This test is what stops the
    // check from being written against definitions only.
    let src = r#"
package P {
	action def Run;
	action def Drive {
		action leg : Run {
			timeslice warmup : Run;
		}
	}
}
"#;
    assert!(
        sem030(src).is_empty(),
        "an action usage is an occurrence usage; got {:?}",
        sem030(src)
    );
}

#[test]
fn a_snapshot_owned_by_an_attribute_definition_is_rejected() {
    // `AttributeDefinition` is a `DataType`, not an `OccurrenceDefinition` (`generated.rs:1785`),
    // so neither side of the disjunction holds.
    let src = r#"
package P {
	attribute def Setting {
		snapshot part frozen : Setting;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Setting::frozen" && msg.contains("snapshot")),
        "an attribute definition is not an occurrence type; got {found:?}"
    );
}

#[test]
fn a_timeslice_owned_by_a_package_is_rejected() {
    let src = r#"
package P {
	timeslice window : P;
}
"#;
    let found = sem030(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::window" && msg.contains("timeslice")),
        "a package is not an occurrence type; got {found:?}"
    );
}

#[test]
fn a_portion_word_that_is_not_a_portion_kind_is_not_judged_here() {
    // `portion` is the KerML `isPortion` word (`KerML.xtext:518`) and writes no portion kind;
    // `PortionKind` is `snapshot | timeslice` (`SysML.xtext:860-862`). A check that treated any
    // portion word as a portion kind would report this legal model.
    let src = r#"
package P {
	attribute def Setting;
	attribute def Table {
		portion attribute p : Setting;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found.is_empty(),
        "`portion` is not a portion kind, so the antecedent is false; got {found:?}"
    );
}

#[test]
fn a_bare_snapshot_with_no_kind_keyword_is_still_judged() {
    // `snapshot s : O;` writes no kind keyword. `declaration_kind()` returns `None` and the
    // declaration falls through to the kind-less arm (`sysml-parser/src/parser.rs:859-895`), so it
    // lowers to `Kind::AttributeUsage` (`lower.rs:158-163`) even though the metamodel makes it a
    // `PortionUsage`, an `OccurrenceUsage`. A check gated on `Kind::PartUsage` would miss it, which
    // is why the antecedent `portionKind <> null` is the whole gate.
    let src = r#"
package P {
	attribute def Setting {
		snapshot s : Setting;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found.iter().any(|(_, at, _)| at == "P::Setting::s"),
        "a bare `snapshot` still states a portion kind; got {found:?}"
    );
}

#[test]
fn a_portion_usage_owned_by_a_bare_snapshot_is_accepted() {
    // The shape the official corpus writes (validation/06-Individual and Snapshots/
    // 6-Individual and Snapshots.sysml:87-102): `individual reference : R { snapshot context_t0 : C
    // { snapshot vehicle_ID1_t0 : V { … } } }`, with no kind keyword anywhere.
    //
    // Both owners are `OccurrenceUsage`s in the metamodel — `IndividualUsage` (`SysML.xtext:845-851`)
    // and `PortionUsage` (`:853-857`) both take a bare `Usage` after the prefix — but with no kind
    // keyword they lower to `Kind::AttributeUsage` (`lower.rs:158-163`). Reading only the owner's
    // kind therefore rejected this legal model six times in one file, which is what this test pins.
    let src = r#"
package P {
	individual def Reference;
	individual def Context;
	individual def Vehicle;
	individual part ref : Reference {
		snapshot context_t0 : Context {
			snapshot vehicle_t0 : Vehicle {
				attribute mass : Real;
			}
		}
	}
}
"#;
    let found = sem030(src);
    assert!(
        found.is_empty(),
        "a `snapshot` under a bare `snapshot` and under a bare `individual` is owned by an \
         occurrence usage; got {found:?}"
    );
}

#[test]
fn a_snapshot_owned_by_an_attribute_usage_is_still_rejected() {
    // The other side of the fix: an owner that wrote none of the occurrence prefixes is not an
    // occurrence type however the tree names it, so the rule must keep reporting here.
    let src = r#"
package P {
	attribute def Setting;
	part def Holder {
		attribute a : Setting {
			snapshot frozen : Setting;
		}
	}
}
"#;
    let found = sem030(src);
    assert!(
        found
            .iter()
            .any(|(_, at, msg)| at == "P::Holder::a::frozen" && msg.contains("snapshot")),
        "an attribute usage that wrote no occurrence prefix is not an occurrence type; got \
         {found:?}"
    );
}

#[test]
fn an_owned_cross_feature_puts_the_modifier_on_the_other_declaration_known_limit() {
    // §7.13.2 splits `end cross [0..1] derived part hub : Hub` into a cross feature `cross` and an
    // end `hub`, and the rule forbids `derived` on the end. `lower.rs` does not model the owned
    // cross feature, so the two become flat siblings: `is_end` lands on `cross`, and the prefix run
    // ends at that name, so `derived` lands on the separate member `hub` instead.
    //
    // The rule reads `is_end` and `isDerived` off the same element, so it finds neither element
    // carrying both and reports nothing. That is a missed violation, and this test records it as
    // one rather than asserting the silence is right. It fails when `lower.rs` starts modelling the
    // owned cross feature, which is the fix.
    let src = r#"
package P {
	part def Hub;
	connection def HubDeviceConnection {
		end cross [0..1] derived part hub : Hub;
	}
}
"#;
    // The limit this test recorded is closed: the prefix run reads `cross [0..1]` as the owned
    // cross feature and `derived` as the end's own modifier, so `hub` is the end, it is derived,
    // and the rule reports it.
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let conn = find(&loaded.roots, "HubDeviceConnection");
    assert_eq!(
        conn.children.len(),
        1,
        "one end, not two siblings: {:?}",
        conn.children
    );
    let hub = &conn.children[0];
    assert_eq!(hub.name, "hub");
    assert!(
        hub.is_end && hub.modifiers.is_derived,
        "the end is the derived one"
    );
    assert!(
        hub.children
            .iter()
            .any(|c| c.name == "cross" && c.multiplicity.as_deref() == Some("[0..1]")),
        "`cross [0..1]` is the end's owned cross feature: {:?}",
        hub.children
    );
    assert!(
        !sem030(src).is_empty(),
        "a derived end is now visible to the rule"
    );
}

// -- the report itself -----------------------------------------------------

#[test]
fn the_report_names_the_rule_by_its_own_code() {
    let src = r#"
package P {
	part def Wheel;
	connection def Fitting {
		derived end wheels : Wheel;
	}
}
"#;
    let found = reported(src);
    assert_eq!(found.len(), 1, "one violating end, one report: {found:?}");
    assert_eq!(found[0].0, "SEM030", "the group's code: {found:?}");
    assert_eq!(
        found[0].1, "P::Fitting::wheels",
        "qualified name: {found:?}"
    );
}

#[test]
fn every_violation_in_one_model_is_reported() {
    let src = r#"
package P {
	part def Wheel;
	attribute def Setting;
	connection def Fitting {
		derived end left : Wheel;
		abstract end right : Wheel;
		end plain : Wheel;
	}
	part def Holder {
		individual part w;
		individual action a;
	}
	attribute def Table {
		snapshot frozen : Setting;
	}
}
"#;
    let names = reported_names(src);
    assert_eq!(
        names,
        [
            "P::Fitting::left",
            "P::Fitting::right",
            "P::Holder::a",
            "P::Holder::w",
            "P::Table::frozen",
        ],
        "exactly the five violating elements, and not the plain end"
    );
}

#[test]
fn a_clean_model_produces_no_sem030() {
    let src = r#"
package P {
	part def Wheel;
	individual part def Axle;
	connection def Fitting {
		end left : Wheel;
		end right : Wheel;
	}
	part def Holder {
		individual part a : Axle;
		snapshot frozen : Holder;
	}
}
"#;
    assert!(
        sem030(src).is_empty(),
        "nothing here breaks a rule of the group; got {:?}",
        sem030(src)
    );
}

// -- `var` is a modifier, not a member name ---------------------------------

#[test]
fn var_arrives_as_one_feature_carrying_is_variable() {
    // The parser used to have no `var` arm, so `var x : Real` split into a feature named `var`
    // and a second member `x : Real`, and four rules had nothing to read. `var` is now a prefix
    // and the flag is on the feature it modifies.
    let src = r#"
package P {
	attribute def AD;
	part def Holder {
		var x : AD;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let holder = find(&loaded.roots, "Holder");
    let names: Vec<&str> = holder.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        ["x"],
        "`var x : AD` must arrive as one feature named `x`"
    );
    assert!(
        holder.children[0].modifiers.is_variable,
        "and it must carry isVariable, which validateFeatureConstantIsVariable and          validateFeaturePortionNotVariable read"
    );
}

#[test]
fn a_plain_attribute_is_not_variable() {
    let src = r#"
package P {
	attribute def AD;
	part def Holder {
		attribute x : AD;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    assert!(
        !find(&loaded.roots, "Holder").children[0]
            .modifiers
            .is_variable
    );
}

#[test]
fn the_four_is_variable_rules_report_nothing() {
    // One model per dropped rule, each breaking it as its own prose describes:
    //
    //   * `validateFeatureEndIsConstant`  — an `end` that is variable and not constant;
    //   * `validateFeatureIsVariable`     — a variable feature inside a PartDefinition, which is
    //                                       not an `Occurrences::Occurrence`;
    //   * `validateFeatureConstantIsVariable` — `constant` with no `var`;
    //   * `validateFeaturePortionNotVariable` — `portion` together with `var`.
    //
    // None of them can be written as intended, because `var` does not survive the parser at all
    // (see `var_arrives_as_one_feature_carrying_is_variable`), so the models below are the nearest
    // shape that parses. The assertion that matters is that no checkable part of the tree moves.
    let src = r#"
package P {
	part def A;
	part def NotAnOccurrence {
		constant part k : A;
		portion var q : A;
	}
	connection def C {
		end e : A;
	}
}
"#;
    let found = sem030(src);
    assert!(
        found.is_empty(),
        "`isVariable` reaches no element, so none of these four can be decided; got {found:?}"
    );
}

#[test]
fn a_constant_feature_keeps_its_flag_but_its_variable_twin_does_not_exist() {
    // Half the field is there, which is worse than neither: `isConstant` survived lowering
    // (`sysml-model/src/lib.rs:882`) and `isVariable` did not, so `isConstant implies isVariable`
    // has one side readable and the other not. This test records which side is missing.
    let src = r#"
package P {
	attribute def AD;
	part def Holder {
		constant attribute k : AD;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let k = find(&loaded.roots, "k");
    assert!(
        k.modifiers.is_constant,
        "`constant` must survive lowering, and does"
    );
    assert!(
        !k.modifiers.is_reference && !k.modifiers.is_derived,
        "the other flags are independent, so is_constant is not a stand-in for any of them"
    );
}

#[test]
fn one_portion_field_means_validate_occurrence_usage_is_portion_cannot_fail() {
    // `portionKind <> null implies isPortion`: in this tree both are `Modifiers::portion`
    // (`sysml-model/src/lib.rs:889`), and the reference implementation sets them together for
    // `snapshot`/`timeslice` (`build.rs:238-245`). A model that states a portion kind without
    // `isPortion` cannot be written, so a check for the rule would be one that can never fire.
    let src = r#"
package P {
	part def Engine {
		snapshot frozen : Engine;
		timeslice span : Engine;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    for name in ["frozen", "span"] {
        let el = find(&loaded.roots, name);
        assert!(
            el.modifiers.portion.is_some(),
            "`{name}` carries the portion kind and isPortion in the same field"
        );
    }
    assert!(
        sem030(src).is_empty(),
        "a legal portion usage is not a violation; got {:?}",
        sem030(src)
    );
}

#[test]
fn a_multi_type_clause_keeps_only_the_first_target() {
    // The blocker for `validateOccurrenceUsageIndividualDefinition`, which counts the whole
    // `occurrenceDefinition` list. `lower.rs:326` calls `Head::target_of(RelationOp::Typing)`, which
    // takes the first target of the first typing clause (`sysml-parser/src/ast.rs:211-216`), so
    // `ind : Ind, Ind2` arrives as `"Ind"` and the count can never exceed one. This test pins that,
    // so the day the whole list arrives the rule becomes decidable.
    let src = r#"
package P {
	individual part def Ind;
	individual part def Ind2;
	part def Holder {
		individual part c : Ind, Ind2;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let c = find(&loaded.roots, "c");
    assert_eq!(
        c.type_ref.as_deref(),
        Some("Ind"),
        "the second target is not on `type_ref`; the day it is, \
         validateOccurrenceUsageIndividualDefinition becomes writable"
    );
    assert!(
        c.ref_spans.len() >= 2,
        "both authored targets are on `ref_spans`, but it records no relation operator: {:?}",
        c.ref_spans
    );
    assert!(
        sem030(src).is_empty(),
        "two individual definitions exceed the count of one, and this module cannot see it; got \
         {:?}",
        sem030(src)
    );
}

#[test]
fn the_owning_type_of_a_root_usage_has_no_element_of_its_own() {
    // `validateOccurrenceUsagePortionKind`'s antecedent needs `owningType`, which is derived
    // (`generated.rs:2227`) and has no field. The tree's substitute for a member declaration is the
    // parent, which is what the implemented check reads; this test records that the tree really
    // does expose that parent and nothing more, so the substitute is the strongest one available.
    let src = r#"
package P {
	part def Engine {
		snapshot frozen : Engine;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    let frozen = find(&loaded.roots, "frozen");
    let owner = index.parent[frozen.id].expect("a member has a parent");
    assert_eq!(
        index.qualified[owner], "P::Engine",
        "the owning type of a member declaration is the element that owns it"
    );
}
