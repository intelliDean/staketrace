//! Assembles raw JSON evidence artifacts from Execution and Consensus Layer responses.

use crate::cl::ClVerificationEvidence;
use crate::el::ElVerificationEvidence;
use crate::models::RawEvidenceArtifacts;
use std::collections::HashMap;

/// Builds `RawEvidenceArtifacts` from EL and CL verification outputs.
pub fn build_raw_evidence(
    el_evidence: &ElVerificationEvidence,
    cl_evidence: &ClVerificationEvidence,
) -> RawEvidenceArtifacts {
    let mut raw_beacon_blocks = HashMap::new();
    for (slot, block) in &cl_evidence.beacon_blocks {
        if let Ok(val) = serde_json::to_value(block) {
            raw_beacon_blocks.insert(slot.to_string(), val);
        }
    }

    let mut raw_parent_states = HashMap::new();
    for (id, list) in &cl_evidence.parent_states_pending {
        if let Ok(val) = serde_json::to_value(list) {
            raw_parent_states.insert(id.clone(), val);
        }
    }

    let mut raw_post_states = HashMap::new();
    for (id, list) in &cl_evidence.post_states_pending {
        if let Ok(val) = serde_json::to_value(list) {
            raw_post_states.insert(id.clone(), val);
        }
    }

    RawEvidenceArtifacts {
        el_receipts: el_evidence.raw_receipts.clone(),
        beacon_blocks: raw_beacon_blocks,
        parent_states_pending: raw_parent_states,
        post_states_pending: raw_post_states,
    }
}
