#![forbid(unsafe_code)]
//! Proof-producing enforcement of the no-energy-selection policy.
//!
//! Native selection APIs accept only [`SelectionInput`], which has no public
//! constructor. Runtime callers first pass [`SelectionRequestShape`] and then
//! call [`InfluenceAudit::prepare_runtime`]; Shape acceptance alone never
//! manufactures proof.
//!
//! ```compile_fail
//! use sim_lib_physics_influence::SelectionInput;
//! let forged = SelectionInput { sink: 1, proof_identity: 0 };
//! ```

mod analysis;

pub use analysis::*;
