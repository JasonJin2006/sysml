//! Syntax front end.
//!
//! Wraps `sysml-parser` — this project's own lexer and recursive-descent parser — and lowers its
//! tree into the language-neutral element tree from `sysml-model`. Everything upstream of this
//! crate is parser-version specific; nothing downstream should see a `sysml_parser` type.
//!
//! # Why the parser is not a dependency
//!
//! It was `sysml-v2-parser` until that crate's grammar turned out to be the limit rather than the
//! floor: four standard-library files used `step entry[1];`, `feature redefines x = y;`,
//! `return attribute [0..*] = ...` and `in x default y`, and none of those is reachable from
//! outside. Those four files define state, transition and function-return semantics, so a library
//! that cannot load them resolves less than it appears to. The parser in this workspace reads all
//! 151 corpus files with no errors.
//!
//! # Layers
//!
//! [`lower`] turns the syntax tree into elements. [`library`] reads the standard library from
//! disk. This module re-exports the lowering entry points so callers do not depend on the
//! internal module split.

mod lower;

pub mod library;
pub mod project;

pub use lower::{
    load, load_project, load_project_at, load_project_with_extra, load_project_with_extra_at,
    lower_document, lower_expr, render, render_model, LibrarySource, Loaded,
};

// Re-exported so a caller that already has a `sysml_parser` document can lower it without
// depending on the parser crate directly.
pub use sysml_parser::ParsedDocument;
