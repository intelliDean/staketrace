//! Queries and maps parent & post state pending consolidations for Beacon blocks.

use super::client::BeaconClient;
use super::scanner::resolve_parent_state_root;
use super::types::{BeaconBlockResponse, PendingConsolidationItem};
use std::collections::HashMap;

/// Result container for parent and post state pending consolidation mappings.
#[derive(Debug, Default)]
pub struct StateDeltaMapping {
    pub parent_states_pending: HashMap<String, Vec<PendingConsolidationItem>>,
    pub post_states_pending: HashMap<String, Vec<PendingConsolidationItem>>,
}

/// Resolves parent and post state pending consolidation lists for a collection of Beacon blocks.
pub async fn resolve_state_deltas_for_blocks(
    client: &BeaconClient,
    beacon_blocks: &HashMap<u64, BeaconBlockResponse>,
) -> StateDeltaMapping {
    let mut mapping = StateDeltaMapping::default();

    for block in beacon_blocks.values() {
        let parent_block_root = &block.data.message.parent_root;
        let post_state_root = &block.data.message.state_root;
        let post_slot = &block.data.message.slot;

        // Resolve parent state root by fetching parent block first
        let parent_state_root = resolve_parent_state_root(client, parent_block_root).await;

        // Query parent state pending consolidations
        if !mapping
            .parent_states_pending
            .contains_key(&parent_state_root)
        {
            if let Ok(pending) = client.get_pending_consolidations(&parent_state_root).await {
                mapping
                    .parent_states_pending
                    .insert(parent_state_root.clone(), pending);
            } else if parent_state_root != *parent_block_root
                && let Ok(pending) = client.get_pending_consolidations(parent_block_root).await
            {
                mapping
                    .parent_states_pending
                    .insert(parent_block_root.clone(), pending);
            }
        }

        // Query post state pending consolidations (by post state root or slot)
        if !mapping.post_states_pending.contains_key(post_state_root) {
            if let Ok(pending) = client.get_pending_consolidations(post_state_root).await {
                mapping
                    .post_states_pending
                    .insert(post_state_root.clone(), pending);
            } else if let Ok(pending) = client.get_pending_consolidations(post_slot).await {
                mapping
                    .post_states_pending
                    .insert(post_slot.clone(), pending);
            }
        }
    }

    mapping
}
