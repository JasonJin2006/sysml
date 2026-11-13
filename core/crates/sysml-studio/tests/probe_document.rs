//! Where one canvas document spends its time.
//!
//! The canvas asks for a document on every load and after every edit, so its cost is the delay a
//! person feels. This probe times the stages `Session::document` runs, one by one, and then the
//! whole call, so a slow document starts from a measurement instead of a guess.
//!
//!     SYSML_PROBE_MODEL=research/experiments/auto-patrol/AutoPatrol.sysml cargo test --release \
//!         -p sysml-studio --test probe_document -- --ignored --nocapture
//!
//! Ignored rather than an ordinary test, for the reason `sysml-semantics/tests/probe_rules.rs`
//! gives: it needs a model it cannot invent, and a test that asserted on time would fail on a
//! loaded machine rather than on a defect. The gate measures the same call through `tools/bench.py`.

use std::time::Instant;

use sysml_studio::document::Session;
use sysml_studio::translate::Lang;
use sysml_view::{Mode, View};

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "profile probe: set SYSML_PROBE_MODEL and run with --ignored --nocapture"]
fn profile_document() {
    let path = std::env::var("SYSML_PROBE_MODEL").expect("SYSML_PROBE_MODEL names a .sysml file");
    let text = std::fs::read_to_string(&path).expect("read model");

    let start = Instant::now();
    let loaded = sysml_syntax::load_project(&[&text], true).expect("parses");
    eprintln!("{:>8.1}ms  load_project (model and library)", ms(start));

    let start = Instant::now();
    let graph = sysml_view::project(&loaded.roots, View::General, Mode::Extended);
    eprintln!(
        "{:>8.1}ms  project gv ({} nodes, {} edges)",
        ms(start),
        graph.nodes.len(),
        graph.edges.len()
    );

    let start = Instant::now();
    let report = sysml_semantics::check(&loaded.roots, &loaded.library);
    eprintln!(
        "{:>8.1}ms  check ({} issues)",
        ms(start),
        report.issues.len()
    );

    let start = Instant::now();
    let units = sysml_semantics::units_of(&loaded.roots, &loaded.library);
    eprintln!("{:>8.1}ms  units_of ({} readings)", ms(start), units.len());

    let start = Instant::now();
    let computed = sysml_semantics::computed_values(&loaded.roots, &loaded.library);
    eprintln!(
        "{:>8.1}ms  computed_values ({} values)",
        ms(start),
        computed.len()
    );

    // What the document asks: all three answers from one checker.
    let start = Instant::now();
    let (report, values, readings) =
        sysml_semantics::check_with_values(&loaded.roots, &loaded.library);
    eprintln!(
        "{:>8.1}ms  check_with_values ({} issues, {} values, {} readings)",
        ms(start),
        report.issues.len(),
        values.len(),
        readings.len()
    );

    let session =
        Session::open(vec![path.clone().into()], View::General, Mode::Extended).expect("opens");
    for round in 1..=3 {
        let start = Instant::now();
        let document = session.document().expect("document");
        eprintln!(
            "{:>8.1}ms  Session::document, round {round} ({} nodes)",
            ms(start),
            document.nodes.len()
        );
    }
    let start = Instant::now();
    let _ = session.next(Lang::En).expect("next");
    eprintln!("{:>8.1}ms  Session::next", ms(start));
}
