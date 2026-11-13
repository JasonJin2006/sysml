//! SysML v2 and KerML textual notation.
//!
//! Own lexer and recursive-descent parser. The previous implementation wrapped
//! `sysml-v2-parser`, which is a good tokenizer but cannot be extended from outside: four
//! standard-library files failed on `step entry[1];`, `feature redefines x = y;`,
//! `return attribute [0..*] = ...` and `in x default y`, and none of those is fixable in a
//! downstream crate. Those four files define the semantics of states, transitions and function
//! returns, so a library that cannot load them resolves less than it appears to.
//!
//! # Layers
//!
//! [`lexer`] turns text into tokens. [`ast`] is the syntax tree, shaped to be easy to lower
//! rather than to mirror the OMG metamodel. [`parser`] builds it.
//!
//! # Error handling
//!
//! Parsing never panics and never silently drops input. A construct the grammar does not accept
//! produces a [`ParseError`] with a line and what was expected; a construct the grammar accepts
//! but the lowering layer does not model becomes a visible marker downstream. Those two channels
//! are deliberately separate: "this text is not valid SysML" and "this is valid SysML that the
//! tool does not understand yet" need different responses.

pub mod ast;
pub mod lexer;
pub mod parser;

pub use lexer::{LexError, Token, TokenKind};
pub use parser::{parse, ParseError, ParsedDocument};
