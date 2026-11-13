//! `member-duplicates` — two members one name.
//!
//! Both findings are warnings: the model is legal, and a reference to the name resolves to two
//! things. The tests pin both directions and the one exemption — a `:>>` redefinition says which
//! inherited member it replaces, so it is not ambiguous.

use sysml_semantics::{check, Severity};

fn sem091(src: &str) -> Vec<(String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.code == "SEM091")
        .map(|i| {
            let level = match i.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            (level.to_string(), i.message.clone())
        })
        .collect()
}

#[test]
fn two_owned_members_with_one_name_warn_at_both() {
    // Both declarations are reported, not only the later one: the pilot model that pins this
    // expects a warning per declaration, because each of them is a name that resolves to two
    // things.
    let src = r#"
package P {
	part def Wheel;
	part def Cart {
		part w : Wheel;
		part w : Wheel;
	}
}
"#;
    let found = sem091(src);
    assert_eq!(found.len(), 2, "got {found:?}");
    assert!(found.iter().all(|(level, _)| level == "warning"));
    assert!(
        found
            .iter()
            .all(|(_, m)| m.contains("duplicate of other owned member name")),
        "got {found:?}"
    );
}

#[test]
fn two_members_with_different_names_do_not_warn() {
    let src = r#"
package P {
	part def Wheel;
	part def Cart {
		part left : Wheel;
		part right : Wheel;
	}
}
"#;
    assert!(sem091(src).is_empty(), "got {:?}", sem091(src));
}

#[test]
fn a_member_repeating_an_inherited_name_warns() {
    let src = r#"
package P {
	classifier C {
		classifier D {}
	}
	classifier D2 :> C {
		classifier D {}
	}
}
"#;
    let found = sem091(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].0, "warning");
    assert!(
        found[0].1.contains("duplicate of inherited member name"),
        "got {found:?}"
    );
}

#[test]
fn a_redefinition_of_the_inherited_member_does_not_warn() {
    // `:>>` says "this is the inherited one", so the name resolves to one element.
    let src = r#"
package P {
	part def Wheel;
	part def Base {
		part wheel : Wheel;
	}
	part def Derived :> Base {
		part :>> wheel : Wheel;
	}
}
"#;
    assert!(sem091(src).is_empty(), "got {:?}", sem091(src));
}

#[test]
fn a_feature_repeating_a_name_through_a_specialization_is_a_redefinition() {
    // `:>` is a `Subsetting`, which is what a redefinition is made of, so a feature that repeats
    // the name a supertype provides redefines it. `Redefinition_Diamond0_valid.kerml.xt` states
    // this shape as clean: `feature B :> A1, A2 { feature p; }`.
    let src = r#"
package P {
	class A {
		feature p;
	}
	class B :> A {
		feature p;
	}
}
"#;
    assert!(sem091(src).is_empty(), "got {:?}", sem091(src));
}

#[test]
fn a_feature_repeating_a_name_through_a_type_clause_warns() {
    // `:` is a `FeatureTyping`, not a `Subsetting`: nothing redefines the inherited member, so the
    // name resolves to two things. The pilot states the warning for exactly this shape
    // (`Membership_distingushability_invalid.sysml.xt:43`).
    let src = r#"
package P {
	part def B;
	part def A {
		part b : B;
	}
	part a : A {
		part b : B;
	}
}
"#;
    let found = sem091(src);
    assert!(
        found
            .iter()
            .any(|(level, m)| level == "warning" && m.contains("repeats the name `b`")),
        "got {found:?}"
    );
}

#[test]
fn a_parameter_repeating_the_types_parameter_does_not_warn() {
    // A directed member is a parameter, and repeating the type's parameter by name is how the
    // notation re-specifies one: `valid_ActionUsage.sysml.xt:60` writes `action a: A { in p; }`
    // against `action def A { in p : P; }` and the pilot reports that model clean.
    let src = r#"
package P {
	part def P;
	action def A { in p : P; }
	action a : A { in p; }
}
"#;
    assert!(sem091(src).is_empty(), "got {:?}", sem091(src));
}

#[test]
fn an_end_repeating_an_inherited_end_name_does_not_warn() {
    // An end is not a member a reference can name. `abstract connector r : R { end a; end b; }`
    // repeats the names `assoc R` gives its ends, and that is the ordinary spelling
    // (`Redefinition_End_Invalid.kerml.xt`).
    let src = r#"
package P {
	part def A;
	part def B;
	assoc R { end a : A; end b : B; }
	connector r : R { end a; end b; }
}
"#;
    let found = sem091(src);
    assert!(
        !found.iter().any(|(_, m)| m.contains("repeats the name")),
        "the ends repeat names by design: {found:?}"
    );
}

#[test]
fn a_relationship_is_not_a_named_member() {
    // Two `bind`s and two bare successions in one body are stored under the keyword that
    // introduced them; they are not two members called `bind` or `then`.
    let src = r#"
package P {
	attribute def Real;
	part def Holder {
		attribute a : Real;
		attribute b : Real;
		bind a = b;
		bind b = a;
	}
}
"#;
    assert!(sem091(src).is_empty(), "got {:?}", sem091(src));
}

#[test]
fn an_end_does_not_collide_with_an_inherited_name() {
    // An `end` is not a member a reference can name, so a name it shares with something inherited
    // is not ambiguous. `InterfaceUsage_Invalid.sysml.xt:53` writes `end part notPort : Fuel;`
    // inside an interface definition and asserts only the two interface-end errors on that line.
    let src = r#"
package P {
	part def Fuel;
	interface def F3 {
		end part notPort : Fuel;
	}
}
"#;
    let found = sem091(src);
    assert!(
        !found.iter().any(|(_, m)| m.contains("notPort")),
        "an end must not be reported as a duplicate name, got {found:?}"
    );
}

#[test]
fn two_members_sharing_a_short_name_warn_at_both() {
    // `ShortNameTests_Distinguishibility1.kerml.xt`: `<one> two` and `<one> three` share the
    // short name `one`, which is an identifier a reference may use.
    let src = r#"
package P {
	classifier <one> two;
	classifier <one> three;
}
"#;
    let found = sem091(src);
    assert_eq!(found.len(), 2, "got {found:?}");
    assert!(
        found
            .iter()
            .all(|(_, m)| m.contains("duplicate of other owned member name")),
        "got {found:?}"
    );
}

#[test]
fn a_name_matching_another_members_short_name_warns() {
    // `ShortNameTests_Distinguishibility2.kerml.xt`: `<one> two` and `<two> three` — the first
    // member's name is the second one's short name. Both are identifiers a reference may use, so
    // they collide.
    let src = r#"
package P {
	classifier <one> two;
	classifier <two> three;
}
"#;
    let found = sem091(src);
    assert_eq!(found.len(), 2, "got {found:?}");
}

#[test]
fn distinct_short_names_do_not_warn() {
    let src = r#"
package P {
	classifier <one> two;
	classifier <three> four;
}
"#;
    assert!(sem091(src).is_empty(), "got {:?}", sem091(src));
}

#[test]
fn the_warning_reaches_the_report_as_a_warning() {
    // The severity has to survive `Violation` → `Issue`: every rule reported as an error until this
    // batch, and a caller that treats a legal model as broken stops reading.
    let src = r#"
package P {
	part def Wheel;
	part def Cart {
		part w : Wheel;
		part w : Wheel;
	}
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let report = check(&loaded.roots, &loaded.library);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.code == "SEM091" && i.severity == Severity::Warning),
        "got {:?}",
        report
            .issues
            .iter()
            .map(|i| (i.code, i.severity))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_named_assertion_is_walked_like_any_other_named_member() {
    // `assert constraint aConstraint0 : ABlock;` writes its name and the pilot warns on it:
    // "Duplicate of inherited member name 'self' from ConstraintCheck, Part"
    // (`ConstraintUsage_Invalid.sysml.xt:42`). The walk skips a member the author did not name,
    // and the assertion lowering used to record no `name_span`, so the warning never fired.
    let src = r#"
package P {
	part def ABlock;
	assert constraint aConstraint0 : ABlock;
}
"#;
    let found = sem091(src);
    assert!(
        found
            .iter()
            .any(|(level, m)| level == "warning" && m.contains("aConstraint0")),
        "got {found:?}"
    );
}

#[test]
fn attribute_usages_choose_defaults_from_direct_feature_typing() {
    // Xpect writes block comments as `//*`; normalize without changing line numbers.
    let src = include_str!("../../../../research/pilot/models/org.omg.sysml.xpect.tests_src_org_omg_sysml_xpect_tests_validation_invalid_AttributeUsage_invalid.sysml.xt")
        .replace("//*", "/*");
    let loaded = sysml_syntax::load_project(&[src.as_str()], true).expect("model parses");
    let report = check(&loaded.roots, &loaded.library);
    let warnings: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.code == "SEM091")
        .collect();
    assert_eq!(warnings.len(), 2, "got {warnings:?}");
    for (issue, line, name, providers) in [
        (warnings[0], 48, "b", vec!["Base::DataValue", "Parts::Part"]),
        (
            warnings[1],
            53,
            "un",
            vec!["Base::DataValue", "Ports::Port"],
        ),
    ] {
        assert_eq!(issue.severity, Severity::Warning);
        assert_eq!(issue.line, line);
        assert!(
            issue.at.ends_with(&format!("Vehicle::{name}")),
            "got {issue:?}"
        );
        assert!(issue.message.contains("inherits `self`"), "got {issue:?}");
        let mut actual: Vec<_> = issue
            .message
            .split_once(" from ")
            .unwrap()
            .1
            .split_once("; a reference")
            .unwrap()
            .0
            .split(", ")
            .collect();
        actual.sort_unstable();
        assert_eq!(actual, providers, "got {issue:?}");
    }
    for line in [45, 48, 50, 53, 55] {
        assert!(
            report.issues.iter().any(|i| i.line == line
                && i.severity == Severity::Error
                && i.message.contains("attribute definitions")),
            "typing error at {line}: {:?}",
            report.issues
        );
    }
    for line in [45, 50, 55] {
        assert!(
            !warnings.iter().any(|i| i.line == line),
            "unexpected SEM091 at {line}: {warnings:?}"
        );
    }
}

#[test]
fn an_anonymous_interface_end_warns_for_its_type_and_port_bases_only() {
    let src = include_str!("../../../../research/pilot/models/org.omg.sysml.xpect.tests_src_org_omg_sysml_xpect_tests_validation_invalid_InterfaceUsage_Invalid.sysml.xt")
        .replace("//*", "/*");
    let loaded = sysml_syntax::load_project(&[src.as_str()], true).expect("model parses");
    let report = check(&loaded.roots, &loaded.library);
    let warnings: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.code == "SEM091")
        .collect();
    assert_eq!(warnings.len(), 1, "got {warnings:?}");
    let issue = warnings[0];
    assert_eq!(issue.severity, Severity::Warning);
    assert_eq!(issue.line, 79);
    assert!(!issue.at.is_empty(), "got {issue:?}");
    assert!(issue.message.contains("inherits `self`"), "got {issue:?}");
    let mut providers: Vec<_> = issue
        .message
        .split_once(" from ")
        .unwrap()
        .1
        .split_once("; a reference")
        .unwrap()
        .0
        .split(", ")
        .collect();
    providers.sort_unstable();
    assert_eq!(providers, vec!["Parts::Part", "Ports::Port"]);
    for (line, text) in [
        (
            53,
            "`InterfaceDefinition::interfaceEnd` is declared `PortUsage`",
        ),
        (53, "an interface joins two things"),
        (
            79,
            "`InterfaceDefinition::interfaceEnd` is declared `PortUsage`",
        ),
    ] {
        assert_eq!(
            report
                .issues
                .iter()
                .filter(|i| i.line == line
                    && i.code == "SEM096"
                    && i.severity == Severity::Error
                    && i.message.contains(text))
                .count(),
            1,
            "missing unique {text} at {line}: {:?}",
            report.issues
        );
    }
    assert!(
        !report
            .issues
            .iter()
            .any(|i| i.code == "SEM096" && (67..=69).contains(&i.line)),
        "the valid multityped binary interface retains exactly two effective ends: {:?}",
        report.issues
    );
}

#[test]
fn an_anonymous_flow_still_gets_its_inherited_duplicates_reported() {
    // `message : A from b to c;` writes `message` as a keyword, not a name, so the element is
    // anonymous — and the rule skipped anonymous declarations on the reasoning that no reference
    // can reach a name they inherit. The pilot states three warnings per line anyway
    // (`FlowUsage_Invalid.sysml.xt:72`, `:79` — `self`, `start`, `done` from `Action, Part`), and
    // this kernel reported none of the six. A flow is typed by an action, so it takes `self` from
    // `Flows::Message`'s action base and from the type's chain, which is exactly two providers.
    // The type has to be a *part* for the two providers to differ — the pilot writes
    // `part def A`, not `action def A`, and with an action type the two paths meet at `Action` and
    // there is nothing to report.
    let src = r#"
package P {
	part def A;
	part b;
	part c;
	message : A from b to c;
}
"#;
    let found = sem091(src);
    let names: Vec<&str> = found
        .iter()
        .filter(|(level, _)| level == "warning")
        .map(|(_, m)| m.as_str())
        .collect();
    for wanted in ["`self`", "`start`", "`done`"] {
        assert!(
            names.iter().any(|m| m.contains(wanted)),
            "a flow inherits `{wanted}` from two providers and must say so; got {found:?}"
        );
    }
}

/// The names a warning says are inherited ambiguously, one entry per warning.
fn inherited_names(found: &[(String, String)]) -> Vec<String> {
    let mut names: Vec<String> = found
        .iter()
        .filter_map(|(_, message)| {
            let start = message.find("inherits `")? + "inherits `".len();
            let end = message[start..].find('`')? + start;
            Some(message[start..end].to_string())
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_performance_and_its_feature_chain_are_namespaces_of_their_own() {
    // `ActionUsage_invalid.sysml.xt:62-69`: `perform b.a;` reference-subsets `a`, which inherits
    // `self`, `start` and `done` from both `Parts::Part` and `Actions::Action`. The reference checks
    // every `Type` (`KerMLValidator.xtend:421-448`), so the usage and the chain `b.a` each carry the
    // three warnings — six on that line, beside the three at `a`'s own declaration.
    let src = r#"
package pkg {
	part def ABlock;
	part def B {
		action a : ABlock;
	}
	ref b : B;
	perform b.a;
}
"#;
    let found = sem091(src);
    let at_perform: Vec<(String, String)> = found
        .iter()
        .filter(|(_, message)| !message.starts_with('`'))
        .cloned()
        .collect();
    assert_eq!(
        inherited_names(&at_perform),
        ["done", "done", "self", "self", "start", "start"],
        "got {found:?}"
    );
    assert!(at_perform.iter().all(|(level, _)| level == "warning"));
    assert_eq!(
        at_perform
            .iter()
            .filter(|(_, m)| m.starts_with("the feature chain `b.a`"))
            .count(),
        3
    );
}

#[test]
fn a_performance_of_an_unambiguous_feature_and_a_plain_reference_stay_silent() {
    // Positive control, and the chain counts only when one is written: `perform b;` names no chain.
    let src = r#"
package pkg {
	part def ABlock;
	action def Act;
	part def B {
		action a : Act;
		action amb : ABlock;
	}
	ref b : B;
	perform b.a;
	ref c : B;
	part p {
		perform action amb2 :> b.amb;
	}
}
"#;
    let found = sem091(src);
    assert!(
        found.iter().all(|(_, message)| !message.contains("`b.a`")),
        "an action typed by an action definition inherits one `self`: {found:?}"
    );
}

#[test]
fn a_feature_chain_written_as_a_specialization_target_is_a_namespace_too() {
    // `ref r :> b.a;` owns the chain `b.a`, typed through `a`, which inherits `self`, `start` and
    // `done` twice. The reference checks every `Type` it builds, the chain included.
    let src = r#"
package pkg {
	part def ABlock;
	part def B {
		action a : ABlock;
	}
	part holder {
		ref b : B;
		ref r :> b.a;
	}
}
"#;
    let found = sem091(src);
    let chains: Vec<&(String, String)> = found
        .iter()
        .filter(|(_, m)| m.starts_with("the feature chain `b.a`"))
        .collect();
    assert_eq!(
        inherited_names(&chains.into_iter().cloned().collect::<Vec<_>>()),
        ["done", "self", "start"],
        "got {found:?}"
    );
}
