//! The typing checks judge a type by its metaclass, as the reference does (`checkAllTypes` and
//! `checkOneType` in `SysMLValidator.xtend`). An action may be typed by any `Behavior`, an item by
//! any `Class`, a constraint by any `Predicate`. Each case below was measured against the
//! reference's condition, and each disagreed with the table of definition words this replaced.

use sysml_model::metaclass;
use sysml_semantics::check;
use sysml_semantics::rules::typed_by_definition;

/// How many `SEM090` findings a package holding `defs` and `usage` draws.
fn sem090(defs: &str, usage: &str) -> usize {
    let src = format!("package P {{\n\t{defs}\n\t{usage}\n}}\n");
    let loaded = sysml_syntax::load_project(&[src.as_str()], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|issue| issue.code == "SEM090")
        .count()
}

#[test]
fn every_required_metaclass_exists() {
    // `metaclass::is_a` answers `false` for a name the metamodel does not declare, so a misspelt
    // row would reject every type silently.
    for name in typed_by_definition::required_metaclasses() {
        assert!(metaclass::is_metaclass(name), "`{name}` is not a metaclass");
    }
}

#[test]
fn typings_the_reference_accepts_are_not_reported() {
    for (defs, usage) in [
        ("state def SD;", "action a : SD;"),
        ("constraint def CD;", "action a : CD;"),
        ("action def AD;", "state s : AD;"),
        ("requirement def RD;", "constraint c : RD;"),
        ("case def CsD;", "calc c : CsD;"),
        ("action def AD;", "item i : AD;"),
        ("allocation def AlD;", "connection c : AlD;"),
        ("concern def CoD;", "requirement r : CoD;"),
    ] {
        assert_eq!(
            sem090(defs, usage),
            0,
            "`{usage}` with `{defs}` is accepted by the reference"
        );
    }
}

#[test]
fn typings_the_reference_rejects_are_reported() {
    for (defs, usage) in [
        ("interface def ID;", "port p : ID;"),
        ("connection def CnD;", "interface i : CnD;"),
        ("part def PD;", "view v : PD;"),
        ("part def PD;", "viewpoint vp : PD;"),
        ("part def PD;", "rendering r : PD;"),
        ("part def PD;", "concern c : PD;"),
    ] {
        assert_eq!(
            sem090(defs, usage),
            1,
            "`{usage}` with `{defs}` is rejected by the reference"
        );
    }
}

#[test]
fn a_parameter_typed_by_a_usage_is_reported() {
    // `checkUsage`: a usage that is neither an attribute nor an occurrence is typed by classifiers,
    // and a parameter is a reference usage.
    assert_eq!(
        sem090("attribute someAttr;", "action def A { in x : someAttr; }"),
        1,
        "a parameter typed by an attribute usage"
    );
    assert_eq!(
        sem090(
            "attribute someAttr;",
            "requirement def R { subject s : someAttr; }"
        ),
        1,
        "a subject is a reference usage too"
    );
    assert_eq!(
        sem090("part def V;", "requirement def R { subject s : V; }"),
        0,
        "a subject typed by a part definition"
    );
    for (defs, usage) in [
        ("attribute def AD;", "action def A { in x : AD; }"),
        (
            "part def V;",
            "action def A { in x : ScalarValues::Real; out z : V; }",
        ),
    ] {
        assert_eq!(sem090(defs, usage), 0, "`{usage}` with `{defs}`");
    }
}

#[test]
fn the_subclasses_of_a_checked_usage_are_checked_too() {
    // The reference applies a check to every subclass of its usage: a performed action and an
    // entry action are actions, an exhibited state is a state, and an objective, a framed concern
    // and a verified requirement are requirements. A return parameter is a reference usage.
    let defs = "part def PD;\n\tattribute someAttr;";
    for usage in [
        "case def C { objective o : PD; }",
        "requirement def R { frame f : PD; }",
        "verification def V { objective { verify v : PD; } }",
        "part y { perform action p : PD; }",
        "part y { exhibit state s : PD; }",
        "state def S { entry action e : PD; }",
        "calc def F { return r : someAttr; }",
    ] {
        assert_eq!(
            sem090(defs, usage),
            1,
            "`{usage}` is rejected by the reference"
        );
    }
}

#[test]
fn the_subclasses_accept_the_types_the_reference_accepts() {
    let defs = "action def AD;\n\tstate def SD;\n\trequirement def RD;\n\tconcern def CD;";
    for usage in [
        "case def C { objective o : RD; }",
        "requirement def R { frame f : CD; }",
        "part y { perform action p : AD; exhibit state s : SD; }",
        "state def S { entry action e : AD; do action d : SD; }",
        "calc def F { return : ScalarValues::Real; }",
    ] {
        assert_eq!(
            sem090(defs, usage),
            0,
            "`{usage}` is accepted by the reference"
        );
    }
}

#[test]
fn a_value_binding_or_an_aliased_subsetting_is_not_a_typing() {
    // `subject vehicle = vehicle1;` binds the subject to a part and does not type it
    // (`10c-Fuel Economy Analysis.sysml:148`); the parser keeps the bound reference where a type
    // would go. `in attribute deltaT :> ISQ::time;` subsets the feature behind an alias, and that
    // feature's type is a definition (`10d-Dynamics Analysis.sysml:59`).
    assert_eq!(
        sem090(
            "part def V;\n\tpart vehicle1 : V;",
            "requirement def R { subject vehicle = vehicle1; }"
        ),
        0,
        "a subject bound to a value"
    );
    assert_eq!(
        sem090(
            "attribute def DurationValue;\n\tattribute duration : DurationValue;\n\talias time for duration;",
            "action def A { in attribute deltaT :> time; }"
        ),
        0,
        "a parameter subsetting an aliased feature"
    );
}

#[test]
fn a_redefined_member_does_not_hide_the_definition_of_the_same_name() {
    // `CalculationUsage_Invalid2.sysml.xt:38-53`: `return r1 : T` in `calc c2 : C` redefines `C`'s
    // result parameter, which is also named `T`. The redefined member is hidden, so the type `T`
    // is the attribute definition outside, and the reference reports no typing error.
    assert_eq!(
        sem090(
            "attribute def T;\n\tcalc def C { return T; }",
            "calc c2 : C { return r1 : T; }"
        ),
        0,
        "the result parameter `T` is redefined, so `T` names the attribute definition"
    );
}

#[test]
fn a_bare_feature_is_a_reference_so_any_classifier_may_type_it() {
    // `out image : Payload;` in a port definition is a `DefaultReferenceUsage`
    // (`SysML.xtext:630-633`), a reference. Its type must be a classifier, which an item
    // definition is. The lowering keeps it an attribute usage, so the typing rule reads
    // `Element::is_bare` to judge it as the reference does. Four experiment models drew 31
    // `SEM090` errors from this shape before the flag existed.
    for (defs, usage) in [
        ("item def I;", "port def Out { out image : I; }"),
        ("item def I;", "part def Q { x : I; }"),
        ("attribute def AD;", "part def Q { x : AD; }"),
        ("part def PD;", "port def Out { out p : PD; }"),
    ] {
        assert_eq!(sem090(defs, usage), 0, "`{usage}` with `{defs}`");
    }
}

#[test]
fn an_attribute_the_author_wrote_as_one_is_still_typed_by_data_types() {
    assert_eq!(
        sem090("item def I;", "part def Q { attribute x : I; }"),
        1,
        "an explicit attribute in a part"
    );
    assert_eq!(
        sem090("item def I;", "port def Out { in attribute y : I; }"),
        1,
        "an explicit attribute in a port"
    );
}

#[test]
fn a_bare_feature_typed_by_a_usage_is_reported_as_a_usage() {
    assert_eq!(
        sem090("attribute someAttr;", "part def Q { x : someAttr; }"),
        1,
        "a usage typed by a usage"
    );
}

#[test]
fn a_view_viewpoint_or_rendering_of_its_own_kind_is_accepted() {
    for (defs, usage) in [
        ("view def VD;", "view v : VD;"),
        ("viewpoint def VpD;", "viewpoint vp : VpD;"),
        ("rendering def RnD;", "rendering r : RnD;"),
        ("concern def CoD;", "concern c : CoD;"),
    ] {
        assert_eq!(sem090(defs, usage), 0, "`{usage}` with `{defs}`");
    }
}
