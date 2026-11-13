//! `typed-by-definition` — a usage names the kind of definition it is a usage of.
//!
//! The authority is the OMG pilot implementation's own test models, which mark each of these as
//! an error. The cases below mirror `PortUsage_Invalid.sysml.xt`, `StateUsage_invalid.sysml.xt`,
//! `ActionUsage_invalid.sysml.xt` and their siblings in `research/pilot`.

use sysml_semantics::{check, Severity};

fn sem090(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM090")
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

#[test]
fn a_port_typed_by_a_part_definition_is_rejected() {
    // `port p1 : B;` in PortUsage_Invalid.sysml.xt.
    let src = r#"
package P {
	part def B;
	part def FA { port p1 : B; }
}
"#;
    assert_eq!(sem090(src).len(), 1, "got {:?}", sem090(src));
}

#[test]
fn a_state_typed_by_a_part_definition_is_rejected() {
    let src = r#"
package P {
	part def A;
	part def H { state s1 : A; }
}
"#;
    assert_eq!(sem090(src).len(), 1, "got {:?}", sem090(src));
}

#[test]
fn an_action_typed_by_a_part_definition_is_rejected() {
    let src = r#"
package P {
	part def Block;
	part def H { action a : Block; }
}
"#;
    assert_eq!(sem090(src).len(), 1, "got {:?}", sem090(src));
}

#[test]
fn an_attribute_typed_by_a_part_definition_is_rejected() {
    let src = r#"
package P {
	part def A;
	part def H { attribute a : A; }
}
"#;
    assert_eq!(sem090(src).len(), 1, "got {:?}", sem090(src));
}

#[test]
fn each_usage_typed_by_its_own_definition_is_accepted() {
    let src = r#"
package P {
	port def PortD;
	state def StateD;
	action def ActionD;
	attribute def AttrD;
	part def H {
		port p : PortD;
		state s : StateD;
		action a : ActionD;
		attribute x : AttrD;
	}
}
"#;
    assert!(sem090(src).is_empty(), "got {:?}", sem090(src));
}

#[test]
fn an_untyped_usage_is_accepted() {
    // The pilot implementation leaves `part aPart;` alone in the same file where it marks the
    // typed-but-wrong ones.
    let src = r#"
package P {
	part def H { port p; state s; action a; }
}
"#;
    assert!(sem090(src).is_empty(), "got {:?}", sem090(src));
}

#[test]
fn a_reference_usage_is_not_judged() {
    // `ref hitchBall : HitchBall` names something that already exists; the kind of the thing
    // referenced is its own business.
    let src = r#"
package P {
	part def HitchBall;
	part def H { ref hitchBall : HitchBall; }
}
"#;
    assert!(sem090(src).is_empty(), "got {:?}", sem090(src));
}

#[test]
fn a_connection_end_is_not_judged() {
    // `end hitch : HitchBall` names one side of a connector, typed by whatever it relates.
    let src = r#"
package P {
	part def HitchBall;
	connection def C { end hitch : HitchBall; }
}
"#;
    assert!(sem090(src).is_empty(), "got {:?}", sem090(src));
}

#[test]
fn a_connection_typed_by_an_association_structure_definition_is_accepted() {
    let src = r#"
package P {
    part def Node;
    assoc struct Network {
        end source : Node;
        end target : Node;
    }
    part holder {
        part left : Node;
        part right : Node;
        connection link : Network connect left to right;
    }
}
"#;
    assert!(sem090(src).is_empty(), "got {:?}", sem090(src));
}

#[test]
fn a_metadata_definitions_members_are_not_judged() {
    // A metadata definition's members name metaclasses by design.
    let src = r#"
package P {
	part def NotAMetaclass;
	metadata def M { attribute annotatedElement : NotAMetaclass; }
}
"#;
    assert!(sem090(src).is_empty(), "got {:?}", sem090(src));
}

#[test]
fn an_allocation_typed_by_a_part_definition_is_rejected() {
    // `AllocationUsage_Invalid.sysml.xt:53` — `allocation :P allocate b to c;` with `P` a part
    // definition. The keyword lowers to `AllocationUsage`, not `AllocationDef`, so the row can tell
    // the two apart; before the split every allocation was a definition and nothing could be
    // rejected.
    let src = r#"
package P {
	part def P0;
	part p {
		allocation :P0 allocate a to b;
	}
}
"#;
    let found = sem090(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].2.contains("must be a definition of the same kind"),
        "got {found:?}"
    );
}

#[test]
fn an_allocation_typed_by_an_allocation_definition_is_accepted() {
    let src = r#"
package P {
	allocation def PowerRail;
	part p {
		allocation bc : PowerRail allocate a to b;
	}
}
"#;
    assert!(sem090(src).is_empty(), "got {:?}", sem090(src));
}
#[test]
fn an_enumeration_value_typed_by_a_part_definition_is_rejected() {
    // `EnumerationUsage_invalid.sysml.xt:48`: `enum e2 : A;` where `A` is an attribute
    // definition. A value needs an enumeration definition, and the message is the same one the
    // enumeration as a whole states.
    let src = r#"
package P {
	attribute def A;
	enum e2 : A;
}
"#;
    let found = sem090(src);
    assert_eq!(found.len(), 1, "got {found:?}");
}

#[test]
fn an_enumeration_value_with_two_types_is_rejected() {
    // `EnumerationUsage_invalid.sysml.xt:50`: `enum e3 : E, EE;` — at most one is allowed.
    let src = r#"
package P {
	enum def E;
	enum def EE;
	enum e3 : E, EE;
}
"#;
    let found = sem090(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].2.contains("at most one"), "got {found:?}");
}

#[test]
fn an_enumeration_attribute_with_two_types_is_rejected() {
    // `EnumerationUsage_invalid.sysml.xt:41` and `:43`: a value of an enumeration is one value, so
    // an attribute whose types include an enumeration may name only that one.
    let src = r#"
package P {
	attribute def A;
	enum def E;
	enum def EE;
	attribute a2 : E, A;
	attribute a3 : E, EE;
}
"#;
    let found = sem090(src);
    assert_eq!(found.len(), 2, "got {found:?}");
    assert!(
        found
            .iter()
            .all(|x| x.2.contains("cannot have more than one type")),
        "got {found:?}"
    );
}

#[test]
fn an_attribute_with_two_ordinary_types_is_accepted() {
    // The enumeration rule is about enumerations: `attribute a2 : A, B;` names two ordinary
    // definitions, and nothing forbids that.
    let src = r#"
package P {
	attribute def A;
	attribute def B;
	attribute a2 : A, B;
}
"#;
    assert!(sem090(src).is_empty(), "got {:?}", sem090(src));
}

#[test]
fn an_assertion_typed_by_a_part_definition_is_rejected() {
    // `ConstraintUsage_Invalid.sysml.xt:42`: `assert constraint aConstraint0 : ABlock;`
    let src = r#"
package P {
	part def ABlock;
	part p {
		assert constraint aConstraint0 : ABlock;
	}
}
"#;
    assert_eq!(sem090(src).len(), 1, "got {:?}", sem090(src));
}

#[test]
fn an_assertion_naming_two_constraint_definitions_is_rejected() {
    // `ConstraintUsage_Invalid.sysml.xt:44`: the rule says *one* constraint definition.
    let src = r#"
package P {
	constraint def AConstraint;
	constraint def AConstraint1;
	part p {
		assert constraint aConstraint1 : AConstraint, AConstraint1;
	}
}
"#;
    let found = sem090(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].2.contains("at most one"), "got {found:?}");
}

#[test]
fn an_assertion_keeps_the_name_the_model_wrote() {
    // `assert constraint named : A;` ends at `;` after its clause, and the parser read every
    // `assert` ending at `;` as the reference form — moving the name into the target end and
    // clearing it. The declaration then had no name of its own, and the pilot asserts on the
    // declaration.
    let src = "package P {\n\tpart def A;\n\tpart p {\n\t\tassert constraint named : A;\n\t}\n}\n";
    let loaded = sysml_syntax::load_project(&[src], true).expect("parses");
    fn find<'a>(roots: &'a [sysml_model::Element], name: &str) -> Option<&'a sysml_model::Element> {
        for el in roots {
            if el.name == name {
                return Some(el);
            }
            if let Some(found) = find(&el.children, name) {
                return Some(found);
            }
        }
        None
    }
    assert!(
        find(&loaded.roots, "named").is_some(),
        "the assertion must keep the name the model gave it"
    );
}

#[test]
fn the_table_covers_the_grammar_layer() {
    // Twenty-one rows, one per usage kind. The pilot's message strings are not one-to-one with
    // them: it states nineteen, and one of those covers three kinds ("occurrence, item or part"),
    // so the count is the table's, not the message list's. `occurrence` is the rule for the kind the
    // projection gained when a bare `snapshot` stopped lowering to an attribute; `enumeration`
    // also covers a single enumeration value (`enum e2 : A;`), and `constraint` also covers the
    // assertion itself (`assert constraint a : A;`). `event occurrence e : T;` states the same
    // requirement as `occurrence e : T;` and is its own row because the notation reaches it by its
    // own keyword.
    //
    // Several rows were keyed on a definition kind because the notation did not distinguish the
    // two: `allocation`, `use case` and `connection` are usages, and each moved once the tree
    // could tell them apart.
    //
    // Then 21 → 33: the reference applies each typing check to every subclass of its usage, and
    // twelve kinds the tree types had no row, so a wrong type on them was accepted: a view, a
    // viewpoint and a rendering (`checkViewUsage`, `checkViewpointUsage`, `checkRenderingUsage`); a
    // concern, an objective, a framed concern and a verified requirement (the requirement rule); a
    // declared `perform` and an `entry`/`do`/`exit` action (the action rule); a declared `exhibit`
    // (the state rule); and a parameter and a subject (the generic usage rule, `checkUsage`).
    // `tests/rules_usage_type_metaclass.rs` pins each.
    assert_eq!(
        sysml_semantics::rules::typed_by_definition::rule_count(),
        33,
        "one row per usage kind"
    );
}

#[test]
fn a_subsetting_supplies_the_type_when_none_is_written() {
    // `occurrence avalue :> aValue;` with `attribute aValue : Real` — the occurrence takes the
    // type of what it subsets, and that type is a data type. The rule read only the typing clause,
    // so the occurrence passed (OccurrenceUsage_invalid.sysml.xt:45).
    let src = r#"
package P {
	private import ScalarValues::*;
	occurrence def A {
		attribute aValue : Real;
		occurrence avalue :> aValue;
	}
}
"#;
    let found = sem090(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].1.ends_with("avalue"), "{found:?}");
}

#[test]
fn a_subsetting_that_supplies_an_occurrence_definition_is_accepted() {
    // The same clause with a type the row allows: a part is an occurrence, so nothing is reported.
    let src = r#"
package P {
	part def PartDef;
	occurrence def A {
		part aPart : PartDef;
		occurrence avalue :> aPart;
	}
}
"#;
    assert!(
        sem090(src).is_empty(),
        "a part definition types an occurrence, got {:?}",
        sem090(src)
    );
}

#[test]
fn an_event_denoting_an_occurrence_typed_by_a_data_value_is_rejected() {
    // `event a.areal;` denotes `A::areal`, which `Real` types, so the event is an occurrence of
    // a data value. The pilot states the message it uses for `occurrence areal : Real` again,
    // at the event (`OccurrenceUsage_invalid.sysml.xt:58`): it is the same defect one hop away.
    let src = r#"
package P {
	public import ScalarValues::*;
	occurrence def A {
		occurrence areal : Real;
	}
	ref a : A;
	event a.areal;
}
"#;
    let found = sem090(src);
    assert!(
        found
            .iter()
            .any(|(_, at, m)| at == "a.areal" && m.contains("an occurrence must be typed by")),
        "got {found:?}"
    );
}

#[test]
fn an_event_denoting_an_occurrence_typed_by_a_definition_is_accepted() {
    let src = r#"
package P {
	occurrence def B;
	occurrence def A {
		occurrence inner : B;
	}
	ref a : A;
	event a.inner;
}
"#;
    assert!(
        sem090(src).is_empty(),
        "an occurrence definition types an occurrence, got {:?}",
        sem090(src)
    );
}

#[test]
fn an_event_occurrence_typed_by_a_data_type_is_rejected() {
    // `event occurrence e : Real;` — the declaration form, whose type is the event's own.
    let src = r#"
package P {
	attribute def Real;
	event occurrence e : Real;
}
"#;
    let found = sem090(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].2.contains("an occurrence"), "{found:?}");
}

#[test]
fn an_event_occurrence_typed_by_an_action_definition_is_accepted() {
    // A behaviour is an occurrence (`ActionDefinition :> OccurrenceDefinition`), and the standard
    // library writes exactly this: `event occurrence zeroCrossingEvents[0..*] :
    // ZeroCrossingEventDef` with an `action def` (`StateSpaceRepresentation.sysml:113`).
    let src = r#"
package P {
	action def ZeroCrossingEventDef;
	event occurrence zeroCrossingEvents : ZeroCrossingEventDef;
}
"#;
    assert!(
        sem090(src).is_empty(),
        "an action definition types an occurrence, got {:?}",
        sem090(src)
    );
}

#[test]
fn a_performance_denoting_a_verification_case_is_accepted() {
    // `9-Verification-simplified.sysml:51,79` — `verification vehicleMassTest : MassTest { … }` and,
    // inside an action, `perform vehicleMassTest { … }`. The performance has no name of its own, so
    // it is judged by the type of what it denotes, and a verification case definition is a kind of
    // action definition (`VerificationCaseDefinition :> CaseDefinition :> CalculationDefinition :>
    // ActionDefinition`). A list holding only the two literal action words rejected the corpus's own
    // valid model.
    let src = r#"
package P {
	verification def MassTest;
	verification vehicleMassTest : MassTest;
	action harness {
		perform vehicleMassTest;
	}
}
"#;
    assert!(
        sem090(src).is_empty(),
        "a verification case definition types an action, got {:?}",
        sem090(src)
    );
}

#[test]
fn an_action_may_name_every_kind_of_action_definition() {
    // The allowed set is a closed list of kinds, so it can fall behind the metamodel without any
    // test noticing. Each row is one subtype of `ActionDefinition` in the notation's own keyword and
    // the usage that names it.
    for (definition, usage) in [
        ("action def", "action"),
        ("calc def", "calc"),
        ("case def", "case"),
        ("analysis case def", "analysis case"),
        ("verification def", "verification"),
        ("use case def", "use case"),
    ] {
        let src = format!(
            "package P {{\n\t{definition} D;\n\t{usage} u : D;\n\taction harness {{\n\t\tperform u;\n\t}}\n}}\n"
        );
        let found = sem090(&src);
        assert!(
            found.is_empty(),
            "`{definition}` is a kind of action definition, got {found:?}"
        );
    }
}

#[test]
fn an_interface_typed_by_an_interface_usage_is_rejected() {
    // `InterfaceUsage_Invalid.sysml.xt:75`: `interface opposite1 : fi` where `fi` is an
    // `interface fi : FuelInterface connect …` usage. Both lower to `Kind::InterfaceDef`, so only
    // `Element::is_definition` can tell them apart.
    let src = r#"
package P {
	port def FuelOutPort;
	part def Fuel;
	interface def FuelInterface {
		end supplierPort : FuelOutPort;
		end consumerPort : FuelOutPort;
	}
	part holder {
		part a;
		part b;
		interface fi : FuelInterface connect a to b;
		interface opposite1 : fi connect a to b;
	}
}
"#;
    let found = sem090(src);
    assert!(
        found
            .iter()
            .any(|(_, at, m)| at.ends_with("::opposite1") && m.contains("must be a definition")),
        "got {found:?}"
    );
    assert!(
        !found.iter().any(|(_, at, _)| at.ends_with("::fi")),
        "`fi` itself names a definition and is legal, got {found:?}"
    );
}
