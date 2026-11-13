//! A project manifest: which files make up a model, and which library it was written against.
//!
//! Before this, a project was whatever the caller listed on the command line. That works for one
//! file and stops working at about five: the list has to be retyped every run, a directory cannot be
//! named at all, and the order the shell happened to expand decides the order the files load in.
//!
//! The manifest is JSON rather than TOML. The reason is not taste: this workspace has no TOML
//! parser, and adding one to read six keys would be a dependency for a file format nobody else
//! reads. Every other file this project writes — the ledger, `--json`, the review page — is JSON,
//! so a project description in JSON is the shape a reader here already knows.
//!
//! What the manifest decides:
//!
//!   * which files are the model, and which are only reachable by name (a corpus next door);
//!   * which library the model is checked against, pinned rather than discovered;
//!   * where the ledger lives, so a project has one ledger and not one per file;
//!   * the order files load in, which is sorted, so two machines load one project the same way.

use std::path::{Path, PathBuf};

/// The file a project is described in.
pub const MANIFEST: &str = "sysml.project.json";

/// Which library a project is checked against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stdlib {
    /// `"discovered"`: the copy found beside the binary, as every command has done until now.
    Discovered,
    /// `"bundled"`: the stub compiled into the binary. A model checked against it can pass while
    /// its unit and quantity chain is missing, which is why pinning is worth doing.
    Bundled,
    /// A directory of library sources, relative to the manifest.
    Pinned(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub version: String,
    /// The directory the manifest sits in. Every path in it is relative to this.
    pub root: PathBuf,
    /// The model, expanded and ordered.
    pub sources: Vec<PathBuf>,
    /// Files reachable by name but not audited as part of the model.
    pub library: Vec<PathBuf>,
    pub stdlib: Stdlib,
    pub ledger: PathBuf,
}

/// Whether a path is a model source this project should read.
fn is_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("sysml" | "kerml")
    )
}

/// Every source under `dir`, sorted.
///
/// Sorted because `read_dir` is not: the platform returns entries in whatever order the filesystem
/// holds them, so the same project could load in a different order on another machine. Ids are
/// assigned in load order, so that difference shows up as a changed `--json` document for an
/// unchanged model — the kind of diff that teaches a reader to ignore diffs.
/// # Errors
///
/// Returns the operating system's message when `dir` cannot be read, prefixed with the path.
pub fn expand(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            out.extend(expand(&path)?);
        } else if is_source(&path) {
            out.push(path);
        }
    }
    Ok(out)
}

/// The manifest at or above `from`.
///
/// Walking up rather than searching the working directory only: a command run from a subdirectory
/// is describing the same project, and refusing to find it there would make the manifest a thing
/// you have to stand in front of.
pub fn discover(from: &Path) -> Option<PathBuf> {
    let start = if from.is_dir() {
        from.to_path_buf()
    } else {
        from.parent()?.to_path_buf()
    };
    let mut dir = Some(start.as_path());
    for _ in 0..8 {
        let current = dir?;
        let candidate = current.join(MANIFEST);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = current.parent();
    }
    None
}

fn string_list(
    value: Option<&serde_json::Value>,
    key: &str,
    path: &Path,
) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(format!(
            "{}: `{key}` must be a list of paths",
            path.display()
        ));
    };
    let mut out = Vec::new();
    for item in items {
        let Some(text) = item.as_str() else {
            return Err(format!(
                "{}: `{key}` holds something that is not a path: {item}",
                path.display()
            ));
        };
        out.push(text.to_string());
    }
    Ok(out)
}

fn expand_entry(root: &Path, text: &str) -> Result<Vec<PathBuf>, String> {
    let path = if Path::new(text).is_absolute() {
        PathBuf::from(text)
    } else {
        root.join(text)
    };
    if path.is_dir() {
        let files = expand(&path)?;
        if files.is_empty() {
            return Err(format!(
                "{}: no .sysml or .kerml files under it",
                path.display()
            ));
        }
        return Ok(files);
    }
    if path.is_file() {
        return Ok(vec![path]);
    }
    Err(format!(
        "no such file or directory: {} (from `{text}`)",
        path.display()
    ))
}

impl Project {
    /// Read the manifest at `path`.
    pub fn read(path: &Path) -> Result<Project, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let root = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let doc: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("{}: not JSON: {e}", path.display()))?;

        let name = doc["name"].as_str().unwrap_or("project").to_string();
        let version = doc["version"].as_str().unwrap_or("0.0.0").to_string();

        let listed = string_list(doc.get("sources"), "sources", path)?;
        if listed.is_empty() {
            return Err(format!(
                "{}: `sources` is empty; a project has to say which files it is",
                path.display()
            ));
        }
        let mut sources: Vec<PathBuf> = Vec::new();
        for entry in &listed {
            for file in expand_entry(&root, entry)? {
                // The same file named twice is one file: a manifest that lists a directory and one
                // of its members would otherwise lower it twice and report every name in it twice.
                if !sources.iter().any(|p| same_file(p, &file)) {
                    sources.push(file);
                }
            }
        }

        let mut library: Vec<PathBuf> = Vec::new();
        for entry in string_list(doc.get("library"), "library", path)? {
            for file in expand_entry(&root, &entry)? {
                if !library.iter().any(|p| same_file(p, &file))
                    && !sources.iter().any(|p| same_file(p, &file))
                {
                    library.push(file);
                }
            }
        }

        let stdlib = match doc.get("stdlib").and_then(|v| v.as_str()) {
            None | Some("discovered") => Stdlib::Discovered,
            Some("bundled") => Stdlib::Bundled,
            Some(other) => {
                let dir = if Path::new(other).is_absolute() {
                    PathBuf::from(other)
                } else {
                    root.join(other)
                };
                if !dir.is_dir() {
                    return Err(format!(
                        "{}: `stdlib` names no directory: {}",
                        path.display(),
                        dir.display()
                    ));
                }
                Stdlib::Pinned(dir)
            }
        };

        let ledger = match doc.get("ledger").and_then(|v| v.as_str()) {
            Some(name) => root.join(name),
            None => root.join(format!("{name}.ledger.json")),
        };

        Ok(Project {
            name,
            version,
            root,
            sources,
            library,
            stdlib,
            ledger,
        })
    }

    /// Load the project: every source as a root, every library file reachable but not audited.
    pub fn load(&self) -> Result<crate::Loaded, String> {
        let texts: Vec<String> = self
            .sources
            .iter()
            .map(|p| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display())))
            .collect::<Result<_, _>>()?;
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let extra_texts: Vec<String> = self
            .library
            .iter()
            .map(|p| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display())))
            .collect::<Result<_, _>>()?;
        let extra: Vec<&str> = extra_texts.iter().map(String::as_str).collect();
        crate::load_project_with_extra_at(
            &refs,
            &extra,
            self.stdlib.pinned(),
            self.stdlib.bundled(),
        )
    }
}

impl Stdlib {
    /// The directory to read, when the project names one.
    pub fn pinned(&self) -> Option<&Path> {
        match self {
            Stdlib::Pinned(path) => Some(path),
            _ => None,
        }
    }

    /// Whether the stub compiled into the binary is what should be used.
    pub fn bundled(&self) -> bool {
        matches!(self, Stdlib::Bundled)
    }
}

/// Whether two paths name the same file. Falls back to the written path when either cannot be
/// canonicalized, which is what a missing file does.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

// -- annotation filters ------------------------------------------------------

/// `@Safety` or `@Safety{level >= 2}` — the annotation a view is narrowed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub name: String,
    pub constraint: Option<String>,
}

/// Read a filter as it is written on a command line or in a box in the canvas.
///
/// `@//safety/`, `@FMEA::Safety` and `@Safety` name one annotation as far as a view can tell: what
/// the model writes after `@` is the name a filter can match, and the last segment is that name.
/// # Errors
///
/// Returns a message that names the accepted forms when `text` does not start with `@`, and the
/// constraint's own parse error when the part inside braces is not an expression.
pub fn parse_filter(text: &str) -> Result<Filter, String> {
    let Some(rest) = text.trim().strip_prefix('@') else {
        return Err(format!(
            "a filter names an annotation: `@Safety` or `@Safety{{level >= 2}}` (got `{text}`)"
        ));
    };
    let (name, constraint) = match rest.find('{') {
        Some(open) => {
            let close = rest
                .rfind('}')
                .ok_or_else(|| format!("`{text}`: the constraint has no closing `}}`"))?;
            (
                &rest[..open],
                Some(rest[open + 1..close].trim().to_string()),
            )
        }
        None => (rest, None),
    };
    let name = name
        .split([':', '/'])
        .filter(|segment| !segment.trim().is_empty())
        .next_back()
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return Err(format!("`{text}` names no annotation"));
    }
    Ok(Filter { name, constraint })
}

/// An expression written outside the model, parsed by the model parser.
///
/// A second expression grammar would disagree with the first, and the disagreement would show up as
/// a filter that matches nothing on a model the checker reads fine.
/// # Errors
///
/// Returns the parser's message, prefixed with the text, when `text` is not an expression the
/// model parser accepts. The wrapper package it is parsed in adds no names, so a reference in
/// `text` resolves later, or not at all.
pub fn constraint_expr(text: &str) -> Result<sysml_model::Expr, String> {
    let src = format!("package __filter {{\n    attribute value = {text};\n}}\n");
    let loaded = crate::load_project(&[&src], false)
        .map_err(|e| format!("`{text}` is not an expression: {e}"))?;
    loaded
        .roots
        .first()
        .and_then(|package| package.children.first())
        .and_then(|attribute| attribute.value_expr.clone())
        .ok_or_else(|| format!("`{text}` is not an expression"))
}
