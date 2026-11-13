//! Loading the SysML/KerML standard library from disk.
//!
//! The library is 94 files and about 1.3 MB of source. Embedding it with `include_str!` would
//! put all of it in every binary for the sake of a constant that most runs never touch, and the
//! upstream library is versioned separately from this crate anyway. It is read at run time
//! instead, and the bundled `stdlib.sysml` stub stays as the fallback for environments that have
//! no library on disk (the test suite, a copied binary).
//!
//! # Why the library is not a set of roots
//!
//! `Loaded::library` is kept apart from `Loaded::roots` on purpose: a view draws the project, and
//! a standard library drawn into every diagram would bury the model in `Real` and `ScalarValue`
//! boxes. The resolver attaches it through `Index::with_library`, where it is reachable only by
//! name — which is exactly how the language treats it.

use std::path::{Path, PathBuf};

/// Files of the standard library, paired with the path they came from.
///
/// Sorted by path so two runs over the same directory lower elements in the same order and
/// therefore assign the same ids. Ids are compared by tests and by the ledger, so an
/// order that depends on directory iteration would make the whole thing flaky.
pub struct LibrarySources {
    pub files: Vec<(PathBuf, String)>,
    /// The directory that was read, for diagnostics and for the report.
    pub root: PathBuf,
}

impl LibrarySources {
    /// Read every `.kerml` and `.sysml` file under `root`.
    ///
    /// A missing or unreadable file is an error rather than a skip: a library that silently
    /// loses the file defining `Real` produces a model where every quantity type is unresolved,
    /// and the cause is much harder to see than an error at load time.
    pub fn read(root: &Path) -> Result<Self, String> {
        let mut paths: Vec<PathBuf> = Vec::new();
        collect(root, &mut paths)?;
        paths.sort();
        if paths.is_empty() {
            return Err(format!(
                "{}: no .kerml or .sysml files found; is this the library root?",
                root.display()
            ));
        }
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            files.push((path, text));
        }
        Ok(Self {
            files,
            root: root.to_path_buf(),
        })
    }

    pub fn texts(&self) -> Vec<&str> {
        self.files.iter().map(|(_, t)| t.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("kerml") | Some("sysml")
        ) {
            out.push(path);
        }
    }
    Ok(())
}

/// Where the library lives, in the order the lookup tries.
///
/// `SYSML_LIBRARY` wins so a test or a deployment can point at a pinned copy. After that the
/// shipped copy in `core/stdlib` is the answer — it is in the repository, so a clone has it and
/// nobody silently falls back to the stub. The research corpus is the last resort, for a layout
/// that has the upstream checkout but not the vendored copy.
pub fn discover(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    if let Some(from_env) = std::env::var_os("SYSML_LIBRARY") {
        let path = PathBuf::from(from_env);
        if path.is_dir() {
            return Some(path);
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        // Walk up a few levels: the binary sits in `target/release`, and the library ships
        // beside the project root in most layouts. `stdlib` comes first because it is the copy
        // that is versioned with this repository.
        for _ in 0..6 {
            let Some(current) = dir else { break };
            candidates.push(current.join("stdlib"));
            candidates.push(current.join("core/stdlib"));
            candidates.push(current.join("../core/stdlib"));
            candidates.push(current.join("sysml.library"));
            candidates.push(current.join("library"));
            // No candidate for the extraction inside the syside wheel. It held a later edition of
            // the library (`fixes` branch), and falling back to it would silently change what the
            // kernel is reading: same model, different semantics. A machine that wants that
            // edition can still point `SYSML_LIBRARY` at it and say so.
            dir = current.parent().map(Path::to_path_buf);
        }
    }
    candidates.into_iter().find(|p| p.is_dir())
}
