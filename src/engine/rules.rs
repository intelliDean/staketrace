//! Deterministic rules and state machine for evaluating consolidation pair statuses.

use crate::cl::{ClVerificationEvidence, ClVerifiedPairEvidence};
use crate::el::{ElVerificationEvidence, ElVerifiedTx};
use crate::models::{ConsolidationPair, ConsolidationStatus, PairVerificationResult};

/// Builds a `PairVerificationResult` by cross-referencing execution and consensus state delta evidence.
pub fn build_pair_verification_result(
    pair: &ConsolidationPair,
    el_evidence: &ElVerificationEvidence,
    cl_evidence: &ClVerificationEvidence,
) -> PairVerificationResult {
    let el_tx_hash = el_evidence.pair_to_tx_map.get(pair).cloned();
    let el_tx = el_tx_hash
        .as_ref()
        .and_then(|h| el_evidence.verified_txs.get(h));

    let cl_pair = cl_evidence.pair_evidence.get(pair);
    let source_index = cl_pair.and_then(|c| c.source_index);
    let target_index = cl_pair.and_then(|c| c.target_index);
    let withdrawal_credentials = cl_pair.and_then(|c| c.withdrawal_credentials.clone());
    let derived_source_address = cl_pair.and_then(|c| c.derived_source_address.clone());
    let beacon_slot = cl_pair.and_then(|c| c.beacon_slot);
    let beacon_request_found = cl_pair.map(|c| c.beacon_request_found).unwrap_or(false);
    let parent_state_absent = cl_pair.and_then(|c| c.parent_state_absent);
    let post_state_present = cl_pair.and_then(|c| c.post_state_present);
    let block_finalized = cl_pair.and_then(|c| c.block_finalized);

    let (status, details, indeterminate_reason) = evaluate_pair_status(el_tx, cl_pair);

    PairVerificationResult {
        source_pubkey: pair.source_pubkey.clone(),
        source_index,
        target_pubkey: pair.target_pubkey.clone(),
        target_index,
        withdrawal_credentials,
        derived_source_address,
        el_tx_hash,
        el_block_number: el_tx.map(|t| t.block_number),
        el_predeploy_found: el_tx
            .map(|t| t.predeploy_interaction_detected)
            .unwrap_or(false),
        beacon_slot,
        beacon_request_found,
        parent_state_absent,
        post_state_present,
        block_finalized,
        status,
        details,
        indeterminate_reason,
    }
}

/// Evaluates status from combined EL and CL evidence.
pub fn evaluate_pair_status(
    el_tx: Option<&ElVerifiedTx>,
    cl_evidence: Option<&ClVerifiedPairEvidence>,
) -> (ConsolidationStatus, String, Option<String>) {
    // 1. EL transaction verification
    let tx = match el_tx {
        Some(t) => t,
        None => {
            return (
                ConsolidationStatus::Indeterminate,
                "No Execution Layer transaction hash provided or matching predeploy calldata found."
                    .to_string(),
                Some("MISSING_EL_TRANSACTION".to_string()),
            );
        }
    };

    if !tx.status_success {
        return (
            ConsolidationStatus::NotAccepted,
            format!(
                "Execution Layer transaction '{}' reverted on-chain (status = 0x0).",
                tx.tx_hash
            ),
            None,
        );
    }

    if !tx.predeploy_interaction_detected {
        return (
            ConsolidationStatus::NotAccepted,
            format!(
                "Execution Layer transaction '{}' did not call the consolidation predeploy.",
                tx.tx_hash
            ),
            None,
        );
    }

    // 2. Consensus Layer state verification
    let cl = match cl_evidence {
        Some(c) => c,
        None => {
            return (
                ConsolidationStatus::Indeterminate,
                "Consensus Layer verification data is missing.".to_string(),
                Some("MISSING_CL_EVIDENCE".to_string()),
            );
        }
    };

    if let Some(ref err) = cl.cl_error {
        return (
            ConsolidationStatus::Indeterminate,
            format!("Consensus layer verification could not complete: {err}."),
            Some(err.clone()),
        );
    }

    if cl.source_index.is_none() || cl.target_index.is_none() {
        return (
            ConsolidationStatus::Indeterminate,
            format!(
                "Source validator (idx: {:?}) or target validator (idx: {:?}) could not be resolved from Beacon API.",
                cl.source_index, cl.target_index
            ),
            Some("UNRESOLVED_VALIDATOR_INDICES".to_string()),
        );
    }

    // Exact state delta verification logic
    match (
        cl.beacon_request_found,
        cl.parent_state_absent,
        cl.post_state_present,
        cl.block_finalized,
    ) {
        // Case 1: Exact proof of inclusion, delta transition (absent before, present after), and finalized block -> ACCEPTED
        (true, Some(true), Some(true), Some(true)) => (
            ConsolidationStatus::Accepted,
            format!(
                "Verified: Request included in finalized Beacon block (slot {}) and proven newly transitioned (absent in parent state, present in post state).",
                cl.beacon_slot.unwrap_or_default()
            ),
            None,
        ),

        // Case 2: In block, newly transitioned, but not yet finalized -> QUEUED
        (true, Some(true), Some(true), Some(false)) => (
            ConsolidationStatus::Queued,
            format!(
                "Request included in Beacon block (slot {}) and queued in post state, awaiting block finalization.",
                cl.beacon_slot.unwrap_or_default()
            ),
            None,
        ),

        // Case 3: In block, but state was already pending in parent state (pre-existing pair, not newly accepted by this block)
        (true, Some(false), Some(true), _) => (
            ConsolidationStatus::Queued,
            format!(
                "Consolidation pair was already present in parent state prior to Beacon slot {}; request is queued.",
                cl.beacon_slot.unwrap_or_default()
            ),
            None,
        ),

        // Case 4: Request was in block execution requests, but absent in post state -> CL REJECTION (failed validation rules during block execution)
        (true, Some(true), Some(false), _) => (
            ConsolidationStatus::NotAccepted,
            format!(
                "Request was included in Beacon block (slot {}), but was rejected during block state transition (absent in post state).",
                cl.beacon_slot.unwrap_or_default()
            ),
            None,
        ),

        // Case 5: Request not found in the estimated beacon block body
        (false, _, _, _) => (
            ConsolidationStatus::Queued,
            format!(
                "EL tx '{}' succeeded, but request has not yet appeared in subsequent Beacon block bodies; awaiting block proposer inclusion.",
                tx.tx_hash
            ),
            None,
        ),

        // Default fail-closed
        _ => (
            ConsolidationStatus::Indeterminate,
            "State delta proof could not be definitively evaluated from Consensus Layer data."
                .to_string(),
            Some("INCONCLUSIVE_STATE_DELTA".to_string()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::el::TxReceipt;

    fn mock_el_tx(success: bool) -> ElVerifiedTx {
        ElVerifiedTx {
            tx_hash: "0x111".to_string(),
            status_success: success,
            block_number: 100,
            block_hash: "0xabc".to_string(),
            block_timestamp: 1700000000,
            from: "0xoperator".to_string(),
            to: Some("0x0000bbddc7ce488642fb579f8b00f3a590007251".to_string()),
            predeploy_interaction_detected: true,
            matched_manifest_pairs: vec![],
            receipt: TxReceipt {
                transaction_hash: "0x111".to_string(),
                block_number: 100,
                block_hash: "0xabc".to_string(),
                status: success,
                gas_used: 21000,
                from: "0xoperator".to_string(),
                to: Some("0x0000bbddc7ce488642fb579f8b00f3a590007251".to_string()),
                logs: vec![],
                raw: serde_json::Value::Null,
            },
            details: None,
        }
    }

    fn mock_cl_evidence(
        beacon_req: bool,
        parent_absent: Option<bool>,
        post_present: Option<bool>,
        finalized: Option<bool>,
    ) -> ClVerifiedPairEvidence {
        ClVerifiedPairEvidence {
            source_pubkey: "0x01".to_string(),
            source_index: Some(10),
            target_pubkey: "0x02".to_string(),
            target_index: Some(20),
            withdrawal_credentials: Some(
                "0x0100000000000000000000001111111111111111111111111111111111111111".to_string(),
            ),
            derived_source_address: Some("0x1111111111111111111111111111111111111111".to_string()),
            beacon_slot: Some(500),
            beacon_request_found: beacon_req,
            parent_state_absent: parent_absent,
            post_state_present: post_present,
            block_finalized: finalized,
            cl_error: None,
        }
    }

    #[test]
    fn test_status_accepted_with_exact_state_delta_and_finality() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(true, Some(true), Some(true), Some(true));
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Accepted);
    }

    #[test]
    fn test_status_queued_when_not_yet_finalized() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(true, Some(true), Some(true), Some(false));
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Queued);
    }

    #[test]
    fn test_status_not_accepted_when_rejected_in_post_state() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(true, Some(true), Some(false), Some(true));
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::NotAccepted);
    }

    #[test]
    fn test_status_queued_when_pre_existing_in_parent_state() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(true, Some(false), Some(true), Some(true));
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Queued);
    }

    #[test]
    fn test_status_indeterminate_when_state_pruned() {
        let el_tx = mock_el_tx(true);
        let mut cl = mock_cl_evidence(true, None, None, None);
        cl.cl_error = Some("HISTORICAL_STATE_PRUNED_OR_UNAVAILABLE".to_string());
        let (status, _, reason) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Indeterminate);
        assert_eq!(
            reason.as_deref(),
            Some("HISTORICAL_STATE_PRUNED_OR_UNAVAILABLE")
        );
    }
}
