#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Explicit, immutable topology for lumped physical studies.
//!
//! Validation is deliberately solve-independent: a solver never receives an
//! ambiguous boundary, undeclared crossing, dangling endpoint, or unordered
//! event graph.

mod model;

pub use model::*;
