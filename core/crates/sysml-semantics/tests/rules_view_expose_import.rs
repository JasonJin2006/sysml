//! The `view-expose-import` rules, each proved by a model that breaks it.
//!
//! A rule appears here only when a model violating it produces the diagnostic. A test that merely
//! shows a valid model passing would not distinguish "the rule runs" from "the rule is absent".
//!
//! Two rules are decided in `rules/view_expose_import.rs` and proved below:
//! `validateExposeOwningNamespace` and `validateImportTopLevelVisibility`.
//!
//! The other five are not tested for rejection, because no model this layer reads can express
//! them: the field or the metaclass each one reads is absent from [`sysml_model::Element`]. They
//! are pinned at the bottom of this file instead, by two kinds of test:
//!
//!   * equality tests lower two declarations that differ only by the value the rule reads and
//!     assert the whole element still matches. They hold exactly while the value is discarded, and
//!     start failing the moment a field carries it — which is when the rule becomes decidable;
//!   * `*_is_not_reported` tests run a model that violates each rule and assert nothing is
//!     reported. They fail when the check is finally written, which is the intent.
//!
//! The module-level tests call `rules::view_expose_import::issues` directly rather than going
//! through `check`, so they do not silently pass if the module is never wired into `rules::all`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;
use sysml_semantics::rules::view_expose_import;
use sysml_semantics::{check, Severity};

fn issues(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    check(&loaded.roots, &loaded.library)
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| (i.code.to_string(), i.at.clone(), i.message.clone()))
        .collect()
}

/// Every `SEM025` in a model, by element and message.
fn sem025(src: &str) -> Vec<(String, String)> {
    issues(src)
        .into_iter()
        .filter(|(code, _, _)| code == "SEM025")
        .map(|(_, at, message)| (at, message))
        .collect()
}

/// What the module under test reports for a model, independent of the wiring in `rules::all`.
fn reported(src: &str) -> Vec<(String, String, String)> {
    let loaded = sysml_syntax::load_project(&[src], true).expect("model parses");
    let index = Index::build(&loaded.roots).with_library(&loaded.library);
    view_expose_import::issues(&loaded.roots, &index)
        .into_iter()
        .map(|v| (v.code.to_string(), v.element, v.message))
        .collect()
}

fn lowered(src: &str) -> Vec<Element> {
    sysml_syntax::load_project(&[src], true)
        .expect("model parses")
        .roots
}

/// The element named `name`, wherever it sits in the tree.
fn find<'a>(roots: &'a [Element], name: &str) -> &'a Element {
    fn go<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
        if el.name == name {
            return Some(el);
        }
        el.children.iter().find_map(|c| go(c, name))
    }
    roots
        .iter()
        .find_map(|r| go(r, name))
        .unwrap_or_else(|| panic!("no element named `{name}`"))
}

/// The first element of a kind, wherever it sits in the tree.
fn find_kind<'a>(roots: &'a [Element], kind: &Kind) -> &'a Element {
    fn go<'a>(el: &'a Element, kind: &Kind) -> Option<&'a Element> {
        if &el.kind == kind {
            return Some(el);
        }
        el.children.iter().find_map(|c| go(c, kind))
    }
    roots
        .iter()
        .find_map(|r| go(r, kind))
        .unwrap_or_else(|| panic!("no element of kind {kind:?}"))
}

/// Every field of an element that records where it was written, cleared.
///
/// Two spellings of the same declaration sit at different byte offsets, so the four span fields
/// are the only legitimate difference between them. Clearing them explicitly — rather than
/// filtering a rendered form — keeps the comparison exact: every other field, `line` included,
/// must still match, and a modifier field added to [`Element`] later shows up in the compared
/// rendering with no change to this test.
fn without_spans(el: &Element) -> Element {
    fn clear(el: &mut Element) {
        el.name_span = None;
        el.type_span = None;
        el.value_span = None;
        el.ref_spans.clear();
        for child in &mut el.children {
            clear(child);
        }
    }
    let mut copy = el.clone();
    clear(&mut copy);
    copy
}

/// The element's rendering, spans removed. `Element` has no `PartialEq`, and `Debug` shows every
/// field, so this is the strictest comparison the model crate allows.
fn shape(el: &Element) -> String {
    format!("{:#?}", without_spans(el))
}

/// Assert that a model written to break `rule` produces no diagnostic at all.
///
/// `why` names the field the rule needs and where it is lost, so a later reader can tell an
/// intentional drop from a check that quietly stopped working.
fn assert_nothing_reported(src: &str, rule: &str, why: &str) {
    let found = reported(src);
    assert!(
        found.is_empty(),
        "`{rule}` is not decided by this module, so this model must not be reported. If this \
         fires, the field the rule needs has been added and the check must be written properly \
         rather than left as a heuristic ({why}). Got {found:#?}"
    );
    let through_check = sem025(src);
    assert!(
        through_check.is_empty(),
        "and nothing may reach `check` under SEM025 for the same reason ({why}). \
         Got {through_check:#?}"
    );
}

// -- validateExposeOwningNamespace -------------------------------------------
//
// `importOwningNamespace.oclIsType(ViewUsage)`. The owning namespace is `index.parent` of the
// expose element (`lower.rs:611-616`), and a view lowers to `Kind::ViewDef` (`lower.rs:235`).

#[test]
fn an_expose_owned_by_a_package_is_rejected() {
    let src = r#"
package P {
	part def Q;
	expose Q;
}
"#;
    let found = sem025(src);
    assert_eq!(
        found.len(),
        1,
        "one expose in a package must be reported, got {found:?}"
    );
    assert!(
        found[0].1.contains("expose Q") && found[0].1.contains("ViewUsage"),
        "the report must name the expose and the metaclass it needs, got {found:?}"
    );
}

#[test]
fn an_expose_owned_by_a_requirement_is_rejected() {
    // Nothing about the rule is view-specific in the other direction: any namespace that is not a
    // view fails `oclIsType(ViewUsage)`. A requirement is a namespace the grammar does accept
    // members in, so it is the nearest ordinary case.
    let src = r#"
package P {
	part def Q;
	requirement def R { expose Q; }
}
"#;
    let found = sem025(src);
    assert_eq!(
        found.len(),
        1,
        "an expose under a requirement is not under a ViewUsage, got {found:?}"
    );
}

#[test]
fn an_expose_owned_by_a_viewpoint_usage_is_rejected() {
    // A `ViewpointUsage` specializes `RequirementUsage` (generated.rs:1779), not `ViewUsage`, so
    // the owning namespace fails `oclIsType(ViewUsage)`.
    let src = r#"
package P {
	part def Q;
	viewpoint def VP;
	viewpoint vp : VP { expose Q; }
}
"#;
    let found = sem025(src);
    assert_eq!(
        found.len(),
        1,
        "an expose under a viewpoint is not under a ViewUsage, got {found:?}"
    );
}

#[test]
fn an_expose_owned_by_a_viewpoint_definition_is_rejected() {
    // `ViewpointDefinition specializes RequirementDefinition` (generated.rs:1778).
    let src = r#"
package P {
	part def Q;
	viewpoint def VP { expose Q; }
}
"#;
    let found = sem025(src);
    assert_eq!(
        found.len(),
        1,
        "an expose under a viewpoint definition is not under a ViewUsage, got {found:?}"
    );
}

#[test]
fn an_expose_outside_every_namespace_is_rejected() {
    let src = r#"
part def Q;
expose Q;
"#;
    let found = sem025(src);
    assert_eq!(
        found.len(),
        1,
        "an expose with no owning namespace must be reported, got {found:?}"
    );
    assert!(
        found[0].1.contains("no namespace"),
        "the report must say the expose has no owner, got {found:?}"
    );
}

#[test]
fn an_expose_owned_by_a_view_definition_is_accepted() {
    // The tree lowers a view definition and a view usage to one kind, so this check can confirm
    // that a view owns the expose but not which of the two it is. See the module note: the
    // reference grammar does not admit an expose in a view definition body at all
    // (SysML.xtext:2329-2333 vs :2359-2364), so the model is coarser than the language here.
    let src = r#"
package P {
	part def Q;
	view def V { expose Q; }
}
"#;
    assert!(
        sem025(src).is_empty(),
        "an expose inside a view must not be reported"
    );
}

#[test]
fn an_expose_owned_by_a_view_usage_is_accepted() {
    let src = r#"
package P {
	part def Q;
	view def V;
	view v : V { expose Q::*; }
}
"#;
    assert!(
        sem025(src).is_empty(),
        "an expose inside a view usage must not be reported"
    );
}

// -- validateImportTopLevelVisibility ----------------------------------------
//
// `importOwningNamespace.owner = null implies visibility = VisibilityKind::private`. An import
// with no owner in this tree has `index.parent == None`; an absent prefix is the private default
// (`sysml-model/src/lib.rs:790-792`).

#[test]
fn a_public_top_level_membership_import_is_rejected() {
    let src = r#"
public import ScalarValues::Real;
package P {
	part def Q;
}
"#;
    let found = sem025(src);
    assert_eq!(
        found.len(),
        1,
        "a public membership import at the root must be reported, got {found:?}"
    );
    assert!(
        found[0].1.contains("ScalarValues::Real") && found[0].1.contains("private"),
        "the report must name the import and the required visibility, got {found:?}"
    );
}

#[test]
fn a_public_top_level_namespace_import_is_rejected() {
    // The rule reads visibility, not shape, so both `import A::B` and `import A::*` are covered.
    let src = r#"
public import ScalarValues::*;
package P {
	part def Q;
}
"#;
    let found = sem025(src);
    assert_eq!(
        found.len(),
        1,
        "a public namespace import at the root must be reported, got {found:?}"
    );
}

#[test]
fn a_protected_top_level_import_is_rejected() {
    let src = r#"
protected import ScalarValues::*;
package P {
	part def Q;
}
"#;
    let found = sem025(src);
    assert_eq!(
        found.len(),
        1,
        "a protected import at the root must be reported, got {found:?}"
    );
    assert!(
        found[0].1.contains("protected"),
        "the report must quote the visibility that was written, got {found:?}"
    );
}

#[test]
fn a_private_top_level_import_is_accepted() {
    let src = r#"
private import ScalarValues::*;
package P {
	part def Q;
}
"#;
    assert!(
        sem025(src).is_empty(),
        "a private import at the root is what the rule requires"
    );
}

#[test]
fn an_import_without_a_prefix_is_a_syntax_error_not_a_default() {
    // `ImportPrefix` is `visibility = VisibilityIndicator 'import'` (`SysML.xtext:239-241`,
    // `KerML.xtext:168-170`): there is no spelling without the keyword. This test used to assert
    // the opposite — "absent visibility is the language default" — which read the metamodel's
    // `visibility` default as if the grammar made the prefix optional. The reference states the
    // error (`Import_Visibility_Invalid.sysml.xt:23`), and the parser now does too, so the model
    // never reaches the rule: it is unreadable, not importable.
    let src = r#"
import ScalarValues::*;
package P {
	part def Q;
}
"#;
    let found = issues(src);
    assert!(
        found.iter().any(|(code, _, _)| code == "SYN001"),
        "a bare import is a syntax error, got {found:?}"
    );
    assert!(
        sem025(src).is_empty(),
        "and it is not a visibility violation, got {:?}",
        sem025(src)
    );
}

#[test]
fn a_public_import_inside_a_top_level_package_is_accepted() {
    // The import's owning namespace is the package, whose own owner is the root namespace, so
    // `importOwningNamespace.owner` is not null and the rule does not apply. The standard library
    // relies on this reading: `ISQ` writes `public import ISQBase::*` inside its own body
    // (library/domain/Quantities and Units/ISQ.sysml:11-22).
    let src = r#"
package P {
	public import ScalarValues::*;
	part def Q;
}
"#;
    assert!(
        sem025(src).is_empty(),
        "an import inside a package is not top-level"
    );
}

#[test]
fn a_public_import_inside_a_nested_package_is_accepted() {
    let src = r#"
package P {
	package Inner {
		public import ScalarValues::*;
		part def Q;
	}
}
"#;
    assert!(
        sem025(src).is_empty(),
        "a nested package also has an owner, so its imports are not top-level"
    );
}

// -- the code ---------------------------------------------------------------

#[test]
fn every_finding_from_this_group_is_reported_under_sem025() {
    let src = r#"
public import ScalarValues::Real;
package P {
	part def Q;
	expose Q;
	viewpoint def VP;
	viewpoint vp : VP { expose Q; }
}
"#;
    let found = sem025(src);
    assert_eq!(
        found.len(),
        3,
        "two exposes outside a view and one public root import, got {found:?}"
    );
    // Nothing else may report these two constructs: an expose or a root import belongs to this
    // group's code, and a second code on one would mean two rules claiming the same statement.
    for (code, at, message) in &issues(src) {
        if message.contains("expose") || message.contains("top-level import") {
            assert_eq!(
                code, "SEM025",
                "`{at}` reported a view/expose/import violation under {code}, not SEM025"
            );
        }
    }
}

// -- the five rules this module cannot decide -------------------------------
//
// Each is stated by the specification, and each reads something the element tree does not carry.
// The citation for the missing field is in the module note; the tests below reduce it to a fact
// about lowering that can be re-checked here.

// `validateExposeVisibility` — `visibility = VisibilityKind::protected`.
//
// The grammar writes no visibility on an expose. `ExposePrefix` is `visibility = ExposeVisibilityKind`,
// and its only value is the keyword `expose` (`SysML.xtext:2366-2372`). The parser refuses a
// visibility keyword before `expose`, so no model reaches `Element::visibility` with a value for the
// rule to read.

#[test]
fn a_visibility_keyword_before_an_expose_is_a_syntax_error() {
    for src in [
        "package P {\n\tpart def Q;\n\tview def V { public expose Q; }\n}\n",
        "package P {\n\tpart def Q;\n\tview def V { private expose Q::*; }\n}\n",
    ] {
        assert!(
            issues(src).iter().any(|(code, _, _)| code == "SYN001"),
            "`{src}` must be a syntax error"
        );
    }
}

#[test]
fn an_expose_has_no_visibility_for_the_rule_to_read() {
    assert_nothing_reported(
        r#"
package P {
	part def Q;
	view def V { expose Q; }
}
"#,
        "validateExposeVisibility",
        "a bare expose carries no visibility, so the rule has no value to read",
    );
}

// `validateExposeIsImportAll` — `isImportAll`.
//
// No field carries it: the struct at `sysml-model/src/lib.rs:721-850` has none, and
// `import_shape` is documented and populated for `Kind::Import` only (`:787-789`,
// `lower.rs:601`). The metamodel defaults `Expose::isImportAll` to true (`generated.rs:2219`),
// and the grammar writes `all` only in `ImportPrefix` (`KerML.xtext:168-171`), never on an
// expose.

#[test]
fn an_expose_records_no_import_form_at_all() {
    // `import_shape` is the only field that could carry an import-all flag, and it is set only on
    // `Kind::Import`. The strongest spellings an expose can write leave it None.
    for src in [
        r#"
package P {
	part def Q;
	view def V { expose Q; }
}
"#,
        r#"
package P {
	part def Q;
	view def V { expose Q::*; }
}
"#,
    ] {
        let roots = lowered(src);
        let expose = find_kind(&roots, &Kind::Expose);
        assert_eq!(
            expose.import_shape, None,
            "an expose records no import shape; if one appears, validateExposeIsImportAll may \
             have become decidable"
        );
        assert_eq!(
            expose.visibility, None,
            "and no visibility either, for the same reason"
        );
    }
}

#[test]
fn an_expose_that_does_not_import_all_is_not_reported() {
    // `Expose::isImportAll` defaults to true and the notation has no way to write false, so the
    // rule's consequent is unreachable. This model writes the only spellings available.
    assert_nothing_reported(
        r#"
package P {
	part def Q;
	view def V { expose Q; }
}
"#,
        "validateExposeIsImportAll",
        "Element has no isImportAll field, and Expose defaults it to true",
    );
}

// `validateViewUsageOnlyOneViewRendering` and `validateViewDefinitionOnlyOneViewRendering` —
// `featureMembership->selectByKind(ViewRenderingMembership)->size() <= 1`.
//
// Three constructs used to collapse to one kind: `render r : R` (a `ViewRenderingMember`,
// SysML.xtext:2335-2343), `rendering r : R` (an ordinary usage member, a `StructureUsageElement`
// admitted by `ViewBodyItem -> DefinitionBodyItem -> OccurrenceUsageMember`, SysML.xtext:661-680),
// and `rendering def R` (a `RenderingDefinition`, `SysML.xtext:301-331`). Only the first is a
// `ViewRenderingMembership`, and only that one the bound counts.
//
// The parser now keeps `render` as a member prefix (`Prefixes::is_render`) rather than a kind
// keyword, and `element_kind` splits `rendering` into a definition and a usage.

#[test]
fn a_render_member_is_told_apart_from_a_rendering_usage() {
    // Two bodies that differ in exactly one word. The violating one writes two view rendering
    // memberships; the legal one writes a membership and an ordinary usage.
    let violating = lowered(
        r#"
package P {
	rendering def R;
	view def V {
		render r1 : R;
		render r2 : R;
	}
}
"#,
    );
    let legal = lowered(
        r#"
package P {
	rendering def R;
	view def V {
		render r1 : R;
		rendering r2 : R;
	}
}
"#,
    );

    let a = find(&violating, "V");
    let b = find(&legal, "V");
    assert_eq!(
        a.children.iter().filter(|c| c.is_render).count(),
        2,
        "the violating body writes two rendering memberships"
    );
    assert_eq!(
        b.children.iter().filter(|c| c.is_render).count(),
        1,
        "the legal body writes one membership and one plain usage"
    );
}

#[test]
fn a_view_body_with_a_rendering_definition_states_one_rendering() {
    // A `rendering def` inside a view body is legal (`DefinitionElement`, SysML.xtext:301-331) and
    // is not a `ViewRenderingMembership`. Counting children of the rendering kind reported this
    // conforming model before the definition and the membership were separate kinds.
    let roots = lowered(
        r#"
package P {
	rendering def R;
	view def V {
		rendering def R2;
		render r : R;
	}
}
"#,
    );
    let view = find(&roots, "V");
    let memberships = view.children.iter().filter(|c| c.is_render).count();
    assert_eq!(
        memberships, 1,
        "one `render` is one membership; the `rendering def` beside it is another element kind"
    );
}

#[test]
fn two_renderings_in_a_view_are_not_reported() {
    assert_nothing_reported(
        r#"
package P {
	rendering def R;
	view def V {
		render r1 : R;
		render r2 : R;
	}
}
"#,
        "validateViewUsageOnlyOneViewRendering / validateViewDefinitionOnlyOneViewRendering",
        "ViewRenderingMembership is not on the tree and render/rendering/rendering def all \
         lower to Kind::RenderingDef",
    );
}

// `validateEnumerationDefinitionIsVariation` — `isVariation`.
//
// `Prefixes::is_variation` on an enumeration definition. The grammar has no slot for the word
// (`SysML.xtext:767-770`), and the parser refuses it as a syntax error, so no tree carries the flag.

#[test]
fn a_variation_enumeration_definition_is_a_syntax_error() {
    assert!(
        issues("package P {\n\tvariation enum def M { a; b; }\n}\n")
            .iter()
            .any(|(code, _, _)| code == "SYN001"),
        "`variation enum def` must be a syntax error"
    );
}

#[test]
fn a_non_variation_enumeration_definition_is_not_reported() {
    assert_nothing_reported(
        r#"
package P {
	enum def M { enum a; enum b; }
}
"#,
        "validateEnumerationDefinitionIsVariation",
        "Element has no isVariation field, and EnumerationDefinition defaults it to true",
    );
}
