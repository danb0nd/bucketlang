//! The compression harness: everything around the language, none of it inside it.
//!
//! The bet this crate exists to test is that an agent can edit a program by
//! reading a *neighbourhood* rather than a file, and that deterministic gates can
//! make that safe enough to run unattended. So the pieces here are:
//!
//! - **retrieval** ([`context`]) — smallest sufficient subgraph for one edit
//! - **the loop** ([`loop_`]) — propose, verify, feed the failure back, retry
//! - **metrics** ([`metrics`]) — what the context cost versus sending the file
//!
//! None of this is in `bucketlang` on purpose. Retrieval depth, retry policy, and
//! encoding are the variables of the experiment; the language is the thing held
//! constant. They are separate crates so that stays true — the harness can only
//! reach what the language chooses to export.

pub mod context;
pub mod loop_;
pub mod metrics;
pub mod result;

pub use context::{build_context, ContextBucket, ContextPack, ContextRef, ContextTest};
pub use loop_::{Agent, EditOutcome, EditRequest, LoopOptions, ScriptedAgent, run_edit};
pub use metrics::{estimate_tokens, Metrics};
pub use result::EditResult;
