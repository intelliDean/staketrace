//! Orchestration engine and deterministic evaluation rules for cross-layer verification.

pub mod evidence;
pub mod pipeline;
pub mod rules;

pub use evidence::build_raw_evidence;
pub use pipeline::VerificationEngine;
pub use rules::{build_pair_verification_result, evaluate_pair_status};
