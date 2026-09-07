//! Beacon block scanning and delayed-dequeue request matching across slots.

use super::client::BeaconClient;
use super::types::BeaconBlockResponse;
use crate::constants::MAX_SCAN_SLOTS;
use crate::utils::timestamp_to_slot;
use std::collections::HashMap;

/// Fetches and caches Beacon blocks for the estimated slots corresponding to EL timestamps,
/// scanning forward up to `max_scan` subsequent slots.
pub async fn fetch_beacon_blocks_for_timestamps(
    client: &BeaconClient,
    timestamps: impl IntoIterator<Item = u64>,
    genesis_time: u64,
    max_scan: u64,
) -> HashMap<u64, BeaconBlockResponse> {
    let mut blocks = HashMap::new();

    for ts in timestamps {
        let estimated_slot = timestamp_to_slot(ts, genesis_time);
        let max_slot = estimated_slot + max_scan.min(MAX_SCAN_SLOTS);

        for slot in estimated_slot..=max_slot {
            if blocks.contains_key(&slot) {
                continue;
            }

            match client.get_beacon_block(&slot.to_string()).await {
                Ok(Some(block)) => {
                    blocks.insert(slot, block);
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }
    }

    blocks
}

/// Resolves the state root of a block's parent by querying the parent block first.
pub async fn resolve_parent_state_root(client: &BeaconClient, parent_block_root: &str) -> String {
    if let Ok(Some(parent_block)) = client.get_beacon_block(parent_block_root).await {
        parent_block.data.message.state_root
    } else {
        parent_block_root.to_string()
    }
}
