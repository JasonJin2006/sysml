//! `property-type`: a metamodel property's declared type, proved by a model that breaks it.
//!
//! Three checks, each with a model that must be rejected and the nearest model that must stay
//! silent. The middle model — a connection definition that writes ends of its own — is the one the
//! corpus states clean and the pilot's own suite does not cover; see
//! [`sysml_semantics::conformance::PropertyCheck::requires_no_own_ends`].

use sysml_semantics::{check, Severity};

fn sem096(src: &str) -> Vec<(String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.code == "SEM096")
        .map(|i| (i.at.clone(), i.message.clone()))
        .collect()
}

fn has(src: &str, at: &str, needle: &str) -> bool {
    sem096(src)
        .iter()
        .any(|(element, message)| element == at && message.contains(needle))
}

#[test]
fn a_connection_definition_inheriting_a_kernel_association_is_rejected() {
    // `ConnectionUsage_End_Invalid.sysml.xt:33`: the ends come from `Links::BinaryLink`, which is a
    // KerML association, so the definition's `connectionEnd` holds two KerML features where the
    // property is declared `Usage`. Nothing is written on the definition's own line, so the report
    // lands there.
    let src = r#"
package P {
	abstract connection def B :> Links::BinaryLink;
}
"#;
    let found = sem096(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].1.contains("ConnectionDefinition::connectionEnd"),
        "got {found:?}"
    );
}

#[test]
fn a_flow_definition_inheriting_a_kernel_association_is_rejected() {
    // `FlowUsage_End_Invalid.sysml.xt:33`, the same shape under `flowEnd`.
    let src = r#"
package P {
	abstract flow def F :> Links::BinaryLink;
}
"#;
    let found = sem096(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].1.contains("FlowDefinition::flowEnd"),
        "got {found:?}"
    );
}

#[test]
fn an_interface_end_that_is_a_part_is_rejected() {
    // `InterfaceUsage_Invalid.sysml.xt:53`: `end part notPort : Fuel;` inside an `interface def`.
    // The end is written here, so the report goes on the end's line — the assertion's own line.
    let src = r#"
package P {
	part def Fuel;
	interface def I {
		end part notPort : Fuel;
	}
}
"#;
    let found = sem096(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        found[0].1.contains("InterfaceDefinition::interfaceEnd"),
        "got {found:?}"
    );
    assert!(
        found[0].0.contains("::I::notPort"),
        "reported at the end, got {found:?}"
    );
}

#[test]
fn an_interface_end_that_is_a_port_is_accepted() {
    let src = r#"
package P {
	port def FuelOutPort;
	interface def I {
		end supplierPort : FuelOutPort;
	}
}
"#;
    assert!(sem096(src).is_empty(), "got {:?}", sem096(src));
}

#[test]
fn a_connection_definition_that_writes_its_own_ends_is_not_judged() {
    // `14c-Language Extensions.sysml:38`, a model the corpus states clean:
    // `connection def Causation :> Occurrences::HappensBefore` inherits four KerML features the
    // same way `B :> Links::BinaryLink` does, and it writes two ends of its own. That is the line
    // between the two models, so it is the line this kernel draws.
    let src = r#"
package P {
	part def Situation;
	connection def Causation :> Occurrences::HappensBefore {
		end [*] ref cause : Situation;
		end [*] ref effect : Situation;
	}
}
"#;
    assert!(sem096(src).is_empty(), "got {:?}", sem096(src));
}

#[test]
fn a_connection_with_one_related_element_is_rejected() {
    // `Relationship_invalid_relatedElement0.sysml.xt:33`: `connection { end ::> b0; }` states one
    // related element where `Relationship::relatedElement` is `Element[2..*]`. The report lands on
    // the declaration, which is where the pilot states it.
    let src = r#"
package P {
	part b0;
	part v {
		connection { end ::> b0; }
	}
}
"#;
    let found = sem096(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].1.contains("1 related element"), "got {found:?}");
}

#[test]
fn an_abstract_connection_may_be_incomplete() {
    // The same model's line 35: `abstract connection { end ::> b0; }` is written beside the one
    // above and the pilot states no error for it.
    let src = r#"
package P {
	part b0;
	part v {
		abstract connection { end ::> b0; }
	}
}
"#;
    assert!(sem096(src).is_empty(), "got {:?}", sem096(src));
}

#[test]
fn a_connector_whose_ends_are_written_after_connect_is_accepted() {
    // `connection bc : BC connect b to c;` — the ends are in the `connect` clause, not in `end`
    // members, and counting only the members read zero for every such connector.
    let src = r#"
package P {
	connection def BC;
	part def B;
	part def C;
	part p {
		part b : B;
		part c : C;
		connection bc : BC connect b to c;
	}
}
"#;
    assert!(sem096(src).is_empty(), "got {:?}", sem096(src));
}

#[test]
fn a_connection_shorthand_with_one_endpoint_states_one_related_element() {
    // `connection c : C connect b;` projects a single end child. Counting that child and the
    // authored endpoint text beside it would read two and accept the connector, which is the double
    // count this regression is about: the projected ends are the whole count, so the model states
    // one related element and `Connector::relatedFeature` wants two.
    let src = r#"
package P {
	part def B;
	connection def C;
	part v {
		part b : B;
		connection c : C connect b;
	}
}
"#;
    let found = sem096(src);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(found[0].1.contains("1 related element"), "got {found:?}");
}

#[test]
fn a_third_interface_end_is_rejected() {
    // `InterfaceUsage_Invalid.sysml.xt:45-53`: an interface joins two things, and the third `end`
    // is reported on its own line — "Cannot have more than two ends".
    let src = r#"
package P {
	port def FuelOutPort;
	part def Fuel;
	interface def I {
		end supplierPort : FuelOutPort;
		end consumerPort : FuelOutPort;
		end part notPort : Fuel;
	}
}
"#;
    let found = sem096(src);
    // Two rules meet on that line — the third end is also a `part` where the property asks for a
    // `PortUsage` — and the pilot asserts both there, so both are expected.
    assert!(
        found
            .iter()
            .any(|(at, m)| at.ends_with("::notPort") && m.contains("further interface end")),
        "got {found:?}"
    );
}

#[test]
fn interface_shorthand_ends_are_counted_once() {
    let src = r#"
package P {
    port def Port;
    interface def Base {
        end source : Port;
        end target : Port;
    }
    part def Assembly {
        part left;
        part right;
        interface joined : Base connect left to right;
    }
}
"#;
    assert!(
        sem096(src).is_empty(),
        "projected shorthand ends count once: {:?}",
        sem096(src)
    );
}

#[test]
fn interface_shorthand_that_leaves_a_third_inherited_end_is_rejected() {
    // The malformed half of the same count: the shorthand positionally replaces the first two ends
    // of the definition, the third inherited end survives, and the usage holds three effective ends.
    // `AtMost(2)` then reports the one past the limit, and that end is inherited, so the report lands
    // on the usage's declaration. If `from` / `to` were added beside the projected children the count
    // would read four and blame the usage for a count it never wrote.
    let src = r#"
package P {
    port def Port;
    interface def ThreeEnds {
        end first : Port;
        end second : Port;
        end third : Port;
    }
    part def Assembly {
        part left;
        part right;
        interface joined : ThreeEnds connect left to right;
    }
}
"#;
    let found = sem096(src);
    assert!(
        found
            .iter()
            .any(|(at, m)| at.ends_with("::joined") && m.contains("further interface end")),
        "got {found:?}"
    );
}

#[test]
fn an_interface_with_two_ends_is_accepted() {
    let src = r#"
package P {
	port def FuelOutPort;
	interface def I {
		end supplierPort : FuelOutPort;
		end consumerPort : FuelOutPort;
	}
}
"#;
    assert!(sem096(src).is_empty(), "got {:?}", sem096(src));
}
