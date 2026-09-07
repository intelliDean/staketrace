use super::models::{ExitReceipt, ExitRequest, ExitSummary, ExitVerificationResult};
use crate::cl::BeaconClient;
use crate::cl::verifier::derive_address_from_credentials;
use crate::el::ElClient;
use crate::el::exit_predeploy::ExitPredeploy;
use crate::error::Result;
use crate::models::ConsolidationStatus;
use chrono::Utc;
use std::collections::HashMap;

const SECONDS_PER_SLOT: u64 = 12;
const SLOTS_PER_EPOCH: u64 = 32;
const MAX_BLOCK_SCAN_FORWARD: u64 = 64;

pub struct ExitEngine;

impl ExitEngine {
    /// Executes cross-layer verification for a batch of EIP-7002 validator exit requests.
    pub async fn run_verification(
        requests: &[ExitRequest],
        el_tx_hashes: &[String],
        el_client: &ElClient,
        beacon_client: &BeaconClient,
    ) -> Result<ExitReceipt> {
        if requests.is_empty() {
            return Ok(ExitReceipt {
                tool_version: env!("CARGO_PKG_VERSION").to_string(),
                timestamp: Utc::now(),
                el_rpc_url: el_client.rpc_url().to_string(),
                cl_beacon_url: beacon_client.base_url().to_string(),
                summary: ExitSummary::default(),
                exits: Vec::new(),
            });
        }

        // Step 1: Collect validator data from Consensus Layer
        let mut unique_pubkeys = Vec::with_capacity(requests.len());
        for req in requests {
            let norm = req.normalized_pubkey();
            if !unique_pubkeys.contains(&norm) {
                unique_pubkeys.push(norm);
            }
        }

        let full_validators = beacon_client
            .get_validators_full_data(&unique_pubkeys)
            .await
            .unwrap_or_default();

        // Step 2: Fetch Genesis & Finality Checkpoints
        let genesis_res = beacon_client.get_genesis().await.ok();
        let genesis_time = genesis_res
            .and_then(|g| g.data.genesis_time.parse::<u64>().ok())
            .unwrap_or(1606824023);

        let finality_res = beacon_client.get_finality_checkpoints("head").await.ok();
        let finalized_epoch = finality_res
            .and_then(|f| f.data.finalized.epoch.parse::<u64>().ok())
            .unwrap_or(0);

        // Step 3: Fetch EL Transaction Receipts & Blocks
        let mut el_tx_evidence: HashMap<String, (bool, u64, u64, Vec<u8>)> = HashMap::new();
        let mut estimated_start_slot = None;

        for tx_hash in el_tx_hashes {
            if let Ok(Some(receipt)) = el_client.get_transaction_receipt(tx_hash).await {
                let success = receipt.status;
                let block_num = receipt.block_number;
                let mut timestamp = 0;
                let mut calldata = Vec::new();

                if let Ok(Some(tx)) = el_client.get_transaction_by_hash(tx_hash).await {
                    let clean = tx.input.trim_start_matches("0x").trim_start_matches("0X");
                    calldata = hex::decode(clean).unwrap_or_default();
                }

                if let Ok(Some(block)) = el_client.get_block_by_number(block_num).await {
                    timestamp = block.timestamp;
                    if timestamp >= genesis_time {
                        let slot = (timestamp - genesis_time) / SECONDS_PER_SLOT;
                        estimated_start_slot = Some(
                            estimated_start_slot
                                .map(|s: u64| s.min(slot))
                                .unwrap_or(slot),
                        );
                    }
                }

                el_tx_evidence.insert(
                    tx_hash.to_lowercase(),
                    (success, block_num, timestamp, calldata),
                );
            }
        }

        // Step 4: Scan Beacon Blocks for EIP-7002 withdrawal requests
        let mut found_beacon_requests: HashMap<String, (u64, String, bool)> = HashMap::new();

        if let Some(start_slot) = estimated_start_slot {
            let max_slot = start_slot + MAX_BLOCK_SCAN_FORWARD;
            for slot in start_slot..=max_slot {
                if let Ok(Some(block_resp)) =
                    beacon_client.get_beacon_block(&slot.to_string()).await
                {
                    let block_msg = &block_resp.data.message;
                    let block_root = block_msg.state_root.clone();
                    let block_epoch = slot / SLOTS_PER_EPOCH;
                    let is_finalized = block_epoch < finalized_epoch;

                    if let Some(ref exec_reqs) = block_msg.body.execution_requests {
                        let withdrawals = exec_reqs.get_withdrawals();
                        for item in withdrawals {
                            if let Some(ref pubkey) = item.validator_pubkey {
                                let norm_pubkey = pubkey.to_lowercase();
                                found_beacon_requests
                                    .insert(norm_pubkey, (slot, block_root.clone(), is_finalized));
                            }
                        }
                    }
                }
            }
        }

        // Step 5: Evaluate each exit request
        let mut exit_results = Vec::with_capacity(requests.len());
        let mut summary = ExitSummary {
            total_exits: requests.len(),
            ..Default::default()
        };

        for req in requests {
            let norm_pubkey = req.normalized_pubkey();
            let val_data = full_validators.get(&norm_pubkey);
            let val_idx = val_data.and_then(|d| d.index.parse::<u64>().ok());
            let creds = val_data.map(|d| d.validator.withdrawal_credentials.clone());
            let derived_addr = creds.as_deref().and_then(derive_address_from_credentials);

            // Find associated EL transaction
            let mut matched_tx_hash = None;
            let mut el_success = false;
            let mut el_block = None;

            for (tx_hash, (success, block_num, _ts, calldata)) in &el_tx_evidence {
                let matched =
                    ExitPredeploy::match_exits_in_calldata(calldata, std::slice::from_ref(req));
                if !matched.is_empty() {
                    matched_tx_hash = Some(tx_hash.clone());
                    el_success = *success;
                    el_block = Some(*block_num);
                    break;
                }
            }

            let beacon_match = found_beacon_requests.get(&norm_pubkey);
            let (beacon_slot, beacon_block_root, finalized) = match beacon_match {
                Some((s, root, fin)) => (Some(*s), Some(root.clone()), *fin),
                None => (None, None, false),
            };

            let mut rejection_reason = None;
            let status = if !el_success && matched_tx_hash.is_some() {
                rejection_reason = Some("Execution layer transaction reverted.".to_string());
                ConsolidationStatus::NotAccepted
            } else if let Some(true) = val_data.map(|d| d.validator.slashed) {
                rejection_reason =
                    Some("Validator is slashed and exit cannot be safely verified.".to_string());
                ConsolidationStatus::NotAccepted
            } else if beacon_match.is_some() {
                if finalized {
                    ConsolidationStatus::Accepted
                } else {
                    ConsolidationStatus::Queued
                }
            } else if matched_tx_hash.is_some() && el_success {
                // Included on EL, awaiting consensus block inclusion
                ConsolidationStatus::Queued
            } else if val_data.is_none() {
                rejection_reason =
                    Some("Validator public key not found on Consensus Layer.".to_string());
                ConsolidationStatus::Indeterminate
            } else {
                ConsolidationStatus::Indeterminate
            };

            match status {
                ConsolidationStatus::Accepted => summary.accepted += 1,
                ConsolidationStatus::Queued => summary.queued += 1,
                ConsolidationStatus::NotAccepted => summary.not_accepted += 1,
                ConsolidationStatus::Indeterminate => summary.indeterminate += 1,
            }

            exit_results.push(ExitVerificationResult {
                pubkey: req.pubkey.clone(),
                validator_index: val_idx,
                amount_gwei: req.amount_gwei,
                is_full_exit: req.amount_gwei == 0,
                withdrawal_credentials: creds,
                derived_source_address: derived_addr,
                status,
                el_tx_hash: matched_tx_hash,
                el_block_number: el_block,
                beacon_slot,
                beacon_block_root,
                finalized,
                rejection_reason,
            });
        }

        Ok(ExitReceipt {
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: Utc::now(),
            el_rpc_url: el_client.rpc_url().to_string(),
            cl_beacon_url: beacon_client.base_url().to_string(),
            summary,
            exits: exit_results,
        })
    }
}
