//! Parse every file of a corpus and report what failed.

use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect(&p, out);
        } else if matches!(
            p.extension().and_then(|e| e.to_str()),
            Some("sysml") | Some("kerml")
        ) {
            out.push(p);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = args
        .first()
        .cloned()
        .unwrap_or_else(|| "../../research/upstream".to_string());
    let mut files = Vec::new();
    collect(Path::new(&root), &mut files);
    files.sort();

    let mut hard_failures = 0usize;
    let mut with_errors = 0usize;
    let mut clean = 0usize;
    let mut total_errors = 0usize;
    for f in &files {
        let src = match std::fs::read_to_string(f) {
            Ok(s) => s,
            Err(e) => {
                println!("READ-FAIL {}: {e}", f.display());
                hard_failures += 1;
                continue;
            }
        };
        match sysml_parser::parse(&src) {
            Ok(doc) => {
                let errs = doc.document.errors();
                if errs.is_empty() {
                    clean += 1;
                } else {
                    with_errors += 1;
                    total_errors += errs.len();
                    if args.iter().any(|a| a == "-v") {
                        for (span, msg) in errs.iter().take(3) {
                            println!("{}:{}: {}", f.display(), span.line, msg);
                        }
                    }
                }
            }
            Err(e) => {
                hard_failures += 1;
                println!("HARD-FAIL {}: {e}", f.display());
            }
        }
    }
    println!(
        "files {}  clean {}  with-errors {}  hard-fail {}  errors {}",
        files.len(),
        clean,
        with_errors,
        hard_failures,
        total_errors
    );
}
