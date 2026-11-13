//! A usage is printed with the keyword it was written with, not the one its definition has.
//!
//! `concern`, `case`, `analysis` and `interface` each name one kind for the definition and the
//! usage. Printing the kind's keyword wrote `concern def c : PD` for the usage `concern c : PD`.

use sysml_syntax::load;
use sysml_view::outline;

#[test]
fn a_usage_of_a_shared_kind_drops_the_word_def() {
    let loaded = load(
        r#"
package P {
    part def PD;
    concern def CD;
    concern c : PD;
    case def CaseD;
    case k : PD;
    analysis case def AnD;
    analysis a : PD;
    interface def ID;
    interface i : PD;
}
"#,
    )
    .expect("parses");
    let text = outline(&loaded.roots);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.contains(&"  concern def CD"), "{text}");
    assert!(
        lines.iter().any(|l| l.starts_with("  concern c ")),
        "{text}"
    );
    assert!(lines.contains(&"  case def CaseD"), "{text}");
    assert!(lines.iter().any(|l| l.starts_with("  case k ")), "{text}");
    assert!(lines.contains(&"  analysis def AnD"), "{text}");
    assert!(
        lines.iter().any(|l| l.starts_with("  analysis a ")),
        "{text}"
    );
    assert!(lines.contains(&"  interface def ID"), "{text}");
    assert!(
        lines.iter().any(|l| l.starts_with("  interface i ")),
        "{text}"
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.contains(" def ") && l.contains(" : PD")),
        "a usage must not read as a definition: {text}"
    );
}
