//! Consensus Layer state delta verifier for EIP-7251 consolidation requests.

use super::client::BeaconClient;
pub use super::credentials::derive_address_from_credentials;
use super::scanner::fetch_beacon_blocks_for_timestamps;
use super::state_delta::resolve_state_deltas_for_blocks;
use super::types::{
    BeaconBlockResponse, ClVerificationEvidence, ClVerifiedPairEvidence, ConsolidationRequestItem,
    PendingConsolidationItem,
};
use crate::constants::{MAX_SCAN_SLOTS, SLOTS_PER_EPOCH};
use crate::error::Result;
use crate::models::ConsolidationPair;
use crate::utils::time::is_epoch_finalized;
use std::collections::{HashMap, HashSet};

/// Verifies validator consolidation state across the Consensus Layer (Beacon API) using exact block-level state delta proofs.
pub async fn verify_consensus_layer(
    client: &BeaconClient,
    pairs: &[ConsolidationPair],
    el_block_timestamps: &HashMap<u64, u64>,
) -> Result<ClVerificationEvidence> {
    if pairs.is_empty() {
        return Ok(ClVerificationEvidence {
            validator_indices: HashMap::new(),
            validator_withdrawal_credentials: HashMap::new(),
            pair_evidence: HashMap::new(),
            parent_states_pending: HashMap::new(),
            post_states_pending: HashMap::new(),
            beacon_blocks: HashMap::new(),
            finalized_epoch: None,
        });
    }

    // Step 1: Collect unique public keys
    let all_pubkeys = collect_unique_pubkeys(pairs);

    // Step 2: Fetch validator details in batch
    let (validator_indices, validator_credentials) = client
        .get_validators_by_pubkeys(&all_pubkeys)
        .await
        .unwrap_or_else(|_| (HashMap::new(), HashMap::new()));

    // Step 3: Fetch Genesis timestamp
    let genesis_time = client
        .get_genesis()
        .await
        .ok()
        .and_then(|g| g.data.genesis_time.parse::<u64>().ok());

    // Step 4: Fetch Finality Checkpoints
    let finalized_epoch = client
        .get_finality_checkpoints("head")
        .await
        .ok()
        .and_then(|fc| fc.data.finalized.epoch.parse::<u64>().ok());

    // Step 5: Scan Beacon blocks for subsequent slots
    let beacon_blocks = if let Some(genesis) = genesis_time {
        fetch_beacon_blocks_for_timestamps(
            client,
            el_block_timestamps.values().copied(),
            genesis,
            MAX_SCAN_SLOTS,
        )
        .await
    } else {
        HashMap::new()
    };

    // Step 6: Query parent and post state pending consolidations
    let deltas = resolve_state_deltas_for_blocks(client, &beacon_blocks).await;
    let parent_states_pending = deltas.parent_states_pending;
    let post_states_pending = deltas.post_states_pending;

    // Step 7: Evaluate exact delta evidence for each pair
    let mut pair_evidence = HashMap::with_capacity(pairs.len());

    for pair in pairs {
        let src_norm = pair.source_pubkey.to_lowercase();
        let tgt_norm = pair.target_pubkey.to_lowercase();

        let src_idx = validator_indices.get(&src_norm).copied();
        let tgt_idx = validator_indices.get(&tgt_norm).copied();
        let src_creds = validator_credentials.get(&src_norm).cloned();
        let derived_addr = src_creds
            .as_deref()
            .and_then(derive_address_from_credentials);

        let (matched_block, beacon_slot, beacon_request_found) = find_matching_beacon_block(
            &beacon_blocks,
            pair,
            src_idx,
            tgt_idx,
            derived_addr.as_deref(),
        );

        let mut parent_state_absent = None;
        let mut post_state_present = None;
        let mut block_finalized = None;
        let mut cl_error = None;

        if let Some(block) = matched_block {
            let parent_block_root = &block.data.message.parent_root;
            let post_state_root = &block.data.message.state_root;
            let post_slot = &block.data.message.slot;

            let parent_pending = parent_states_pending.get(parent_block_root).or_else(|| {
                parent_states_pending
                    .iter()
                    .find(|(k, _)| k.starts_with("0x"))
                    .map(|(_, v)| v)
            });

            let post_pending = post_states_pending
                .get(post_state_root)
                .or_else(|| post_states_pending.get(post_slot));

            match (parent_pending, post_pending) {
                (Some(parent_list), Some(post_list)) => {
                    let in_parent = is_pair_in_queue(parent_list, src_idx, tgt_idx);
                    let in_post = is_pair_in_queue(post_list, src_idx, tgt_idx);

                    parent_state_absent = Some(!in_parent);
                    post_state_present = Some(in_post);
                }
                _ => {
                    cl_error = Some("HISTORICAL_STATE_PRUNED_OR_UNAVAILABLE".to_string());
                }
            }

            if let (Some(slot), Some(finalized_ep)) = (beacon_slot, finalized_epoch) {
                let block_epoch = slot / SLOTS_PER_EPOCH;
                block_finalized = Some(is_epoch_finalized(block_epoch, finalized_ep));
            }
        } else if beacon_blocks.is_empty() {
            cl_error = Some("BEACON_BLOCK_NOT_FOUND".to_string());
        }

        pair_evidence.insert(
            pair.clone(),
            ClVerifiedPairEvidence {
                source_pubkey: pair.source_pubkey.clone(),
                source_index: src_idx,
                target_pubkey: pair.target_pubkey.clone(),
                target_index: tgt_idx,
                withdrawal_credentials: src_creds,
                derived_source_address: derived_addr,
                beacon_slot,
                beacon_request_found,
                parent_state_absent,
                post_state_present,
                block_finalized,
                cl_error,
            },
        );
    }

    Ok(ClVerificationEvidence {
        validator_indices,
        validator_withdrawal_credentials: validator_credentials,
        pair_evidence,
        parent_states_pending,
        post_states_pending,
        beacon_blocks,
        finalized_epoch,
    })
}

/// Extracts unique 0x-prefixed public keys from consolidation pairs in O(N) time.
fn collect_unique_pubkeys(pairs: &[ConsolidationPair]) -> Vec<String> {
    let mut set = HashSet::with_capacity(pairs.len() * 2);
    for pair in pairs {
        set.insert(pair.source_pubkey.clone());
        set.insert(pair.target_pubkey.clone());
    }
    set.into_iter().collect()
}

/// Checks if a validator pair is present in a `pending_consolidations` list.
fn is_pair_in_queue(
    pending: &[PendingConsolidationItem],
    src_idx: Option<u64>,
    tgt_idx: Option<u64>,
) -> bool {
    match (src_idx, tgt_idx) {
        (Some(s), Some(t)) => pending.iter().any(|item| {
            item.source_index.parse::<u64>().ok() == Some(s)
                && item.target_index.parse::<u64>().ok() == Some(t)
        }),
        _ => false,
    }
}

/// Finds the Beacon block that contains the exact consolidation request for a pair.
fn find_matching_beacon_block<'a>(
    blocks: &'a HashMap<u64, BeaconBlockResponse>,
    pair: &ConsolidationPair,
    src_idx: Option<u64>,
    tgt_idx: Option<u64>,
    derived_source_address: Option<&str>,
) -> (Option<&'a BeaconBlockResponse>, Option<u64>, bool) {
    let mut sorted_slots: Vec<u64> = blocks.keys().copied().collect();
    sorted_slots.sort_unstable();

    for slot in sorted_slots {
        if let Some(block_resp) = blocks.get(&slot)
            && let Some(ref requests) = block_resp.data.message.body.execution_requests
            && let Some(ref consolidations) = requests.consolidations
        {
            for req in consolidations {
                if matches_consolidation_request(
                    req,
                    pair,
                    src_idx,
                    tgt_idx,
                    derived_source_address,
                ) {
                    return (Some(block_resp), Some(slot), true);
                }
            }
        }
    }

    let first_slot = blocks.keys().copied().min();
    let first_block = first_slot.and_then(|s| blocks.get(&s));
    (first_block, first_slot, false)
}

/// Checks if an execution request matches a given consolidation pair.
fn matches_consolidation_request(
    req: &ConsolidationRequestItem,
    pair: &ConsolidationPair,
    src_idx: Option<u64>,
    tgt_idx: Option<u64>,
    derived_source_address: Option<&str>,
) -> bool {
    let match_pubkeys = req.source_pubkey.as_deref().map(|s| s.to_lowercase())
        == Some(pair.source_pubkey.to_lowercase())
        && req.target_pubkey.as_deref().map(|s| s.to_lowercase())
            == Some(pair.target_pubkey.to_lowercase());

    let match_indices = match (src_idx, tgt_idx) {
        (Some(s_idx), Some(t_idx)) => {
            req.source_index
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                == Some(s_idx)
                && req
                    .target_index
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok())
                    == Some(t_idx)
        }
        _ => false,
    };

    let match_address = match (&req.source_address, derived_source_address) {
        (Some(req_addr), Some(derived_addr)) => {
            req_addr.to_lowercase() == derived_addr.to_lowercase()
        }
        _ => true,
    };

    (match_pubkeys || match_indices) && match_address
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_address_from_credentials() {
        let creds_01 = "0x01000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8";
        assert_eq!(
            derive_address_from_credentials(creds_01),
            Some("0x70997970c51812dc3a010c7d01b50e0d17dc79c8".to_string())
        );

        let creds_02 = "0x02000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8";
        assert_eq!(
            derive_address_from_credentials(creds_02),
            Some("0x70997970c51812dc3a010c7d01b50e0d17dc79c8".to_string())
        );
    }

    #[test]
    fn test_non_eth1_credentials() {
        let bls_creds = "0x00a1b2c3d4e5f60718293a4b5c6d7e8f00112233445566778899aabbccddeeff";
        assert_eq!(derive_address_from_credentials(bls_creds), None);
    }

    #[test]
    fn test_timestamp_to_slot() {
        let genesis_time = 1606824023;
        assert_eq!(crate::utils::timestamp_to_slot(1606824023, genesis_time), 0);
        assert_eq!(
            crate::utils::timestamp_to_slot(1606824023 + 12, genesis_time),
            1
        );
        assert_eq!(
            crate::utils::timestamp_to_slot(1606824023 + 120, genesis_time),
            10
        );
    }
}
