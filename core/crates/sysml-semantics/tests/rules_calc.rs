//! `calc` invocation: an argument list becomes parameters, and the body is evaluated.
//!
//! The syntax always parsed; the evaluator answered "call to `X` is not evaluated", so no parametric
//! model could produce a number. These tests pin what a call means.

use sysml_semantics::{value_of, Value};

fn value_of_src(src: &str, name: &str) -> Option<Value> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    value_of(&loaded.roots, &loaded.library, name)
}

#[test]
fn a_call_computes_its_result() {
    let src = r#"
package P {
    private import ScalarValues::*;

    calc def Area {
        in w : Real;
        in h : Real;
        return a : Real = w * h;
    }

    attribute x : Real = Area(2.0, 3.0);
}
"#;
    assert_eq!(value_of_src(src, "P::x"), Some(Value::Real(6.0)));
}

#[test]
fn a_parameter_shadows_a_name_from_outside() {
    // `w` exists at package level and is also a parameter. The body must read the argument; reading
    // the package member would give 10 * 4 = 40 instead of 2 * 4 = 8.
    let src = r#"
package P {
    private import ScalarValues::*;

    attribute w : Real = 10.0;

    calc def Area {
        in w : Real;
        in h : Real;
        return a : Real = w * h;
    }

    attribute x : Real = Area(2.0, 4.0);
}
"#;
    assert_eq!(value_of_src(src, "P::x"), Some(Value::Real(8.0)));
}

#[test]
fn one_calculation_calls_another() {
    let src = r#"
package P {
    private import ScalarValues::*;

    calc def Double {
        in n : Real;
        return a : Real = n * 2.0;
    }

    calc def Twice {
        in n : Real;
        return a : Real = Double(Double(n));
    }

    attribute x : Real = Twice(3.0);
}
"#;
    assert_eq!(value_of_src(src, "P::x"), Some(Value::Real(12.0)));
}

#[test]
fn an_argument_may_be_an_expression() {
    let src = r#"
package P {
    private import ScalarValues::*;

    calc def Area {
        in w : Real;
        in h : Real;
        return a : Real = w * h;
    }

    attribute x : Real = Area(1.0 + 1.0, 6.0 / 2.0);
}
"#;
    assert_eq!(value_of_src(src, "P::x"), Some(Value::Real(6.0)));
}

#[test]
fn an_argument_nobody_can_compute_leaves_the_call_undecided() {
    // A wrong number is worse than no number: `outer` names nothing, so the call has no result
    // rather than a zero.
    let src = r#"
package P {
    private import ScalarValues::*;

    calc def Area {
        in w : Real;
        in h : Real;
        return a : Real = w * h;
    }

    attribute x : Real = Area(outer, 3.0);
}
"#;
    assert_eq!(value_of_src(src, "P::x"), None);
}

#[test]
fn a_call_to_something_that_is_not_a_calculation_is_undecided() {
    let src = r#"
package P {
    private import ScalarValues::*;

    part def Wheel;

    attribute x : Real = Wheel(1.0);
}
"#;
    assert_eq!(value_of_src(src, "P::x"), None);
}

#[test]
fn a_recursion_stops_instead_of_overflowing() {
    // A calculation that calls itself with no base case. The depth guard has to answer, and the
    // answer is "undecided" — not a stack overflow, and not a number.
    let src = r#"
package P {
    private import ScalarValues::*;

    calc def Forever {
        in n : Real;
        return a : Real = Forever(n);
    }

    attribute x : Real = Forever(1.0);
}
"#;
    assert_eq!(value_of_src(src, "P::x"), None);
}

#[test]
fn the_report_computes_every_value_it_can() {
    // `sysml units` asks about every value at once, so the batch is one `Checker` for the lot
    // rather than one per value.
    let src = r#"
package Q {
    private import ScalarValues::*;
    private import ISQ::*;

    calc def Power {
        in force : ForceValue;
        in speed : SpeedValue;
        return p : PowerValue = force * speed;
    }

    attribute f : ForceValue = 10.0 [N];
    attribute v : SpeedValue = 2.0 [m/s];
    attribute p : PowerValue = Power(f, v);
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let values = sysml_semantics::computed_values(&loaded.roots, &loaded.library);
    assert_eq!(values.get("Q::f"), Some(&Value::Real(10.0)));
    assert_eq!(values.get("Q::p"), Some(&Value::Real(20.0)));
}

#[test]
fn a_calculation_carries_units_through_its_parameters() {
    // The number is only half the answer: `10 N * 2 m/s` is 20 W, and a report that said "20" would
    // leave the reader to work out whether that is watts, joules or nothing at all.
    use sysml_semantics::unit_of;
    let src = r#"
package Q {
    private import ScalarValues::*;
    private import ISQ::*;

    calc def Power {
        in force : ForceValue;
        in speed : SpeedValue;
        return p : PowerValue = force * speed;
    }

    attribute f : ForceValue = 10.0 [N];
    attribute v : SpeedValue = 2.0 [m/s];
    attribute p : PowerValue = Power(f, v);
    attribute w : PowerValue = 3.0 [W];
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let computed = unit_of(&loaded.roots, &loaded.library, "Q::p").expect("a reading");
    let written = unit_of(&loaded.roots, &loaded.library, "Q::w").expect("a reading");
    assert!(computed.known, "the computed unit is known: {computed:?}");
    assert_eq!(
        computed.dimension, written.dimension,
        "a computed power has the dimension of a power"
    );
}

#[test]
fn a_calculation_that_adds_mismatched_units_is_undecided() {
    // The unit algebra is not decoration: it is what stops `force + speed` from producing a number.
    use sysml_semantics::unit_of;
    let src = r#"
package Q {
    private import ScalarValues::*;
    private import ISQ::*;

    calc def Wrong {
        in force : ForceValue;
        in speed : SpeedValue;
        return p : Real = force + speed;
    }

    attribute f : ForceValue = 10.0 [N];
    attribute v : SpeedValue = 2.0 [m/s];
    attribute x : Real = Wrong(f, v);
}
"#;
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    assert_eq!(value_of(&loaded.roots, &loaded.library, "Q::x"), None);
    let reading = unit_of(&loaded.roots, &loaded.library, "Q::x").expect("a reading");
    assert!(!reading.known, "nothing was computed: {reading:?}");
}
