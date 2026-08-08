//! bucketlang — a toy expression language.
//!
//! A program is a graph of small typed functions ("buckets"), each with a
//! sequential address, a label, an English description, a contract, and a
//! one-expression body. The pipeline is [`mod@compile`] to an in-memory
//! [`registry`] IR, then [`eval`].
//!
//! Beyond the usual front end, two things here are worth knowing about:
//!
//! - [`error`] — diagnostics as structured fields (`stage`, `code`, `bucket`,
//!   `expected`, `found`, `hint`) rather than formatted prose, resolved to a
//!   line and column only at render time.
//! - [`edit`] — replacing one bucket's body by splicing at its recorded span,
//!   then checking that nothing else moved by comparing every bucket's content
//!   hash before and after.
//!
//! See `README.md` for the language, and its "known rough edges" section before
//! trusting any of this with something that matters.

pub mod ast;
pub mod canonical;
pub mod compile;
pub mod complexity;
pub mod error;
pub mod eval;
pub mod graph;
pub mod edit;
pub mod lexer;
pub mod lint;
pub mod parser;
pub mod registry;
pub mod render;
pub mod value;

pub use compile::{compile, compile_file, compile_with_base, BuildProfile, CompileOptions, CompileResult};
pub use error::{Error, Result};
pub use lint::unused_warnings;
pub use value::Value;
