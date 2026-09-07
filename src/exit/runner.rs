//! Pipeline and state evaluator for EIP-7002 validator exits and partial withdrawals.

use super::models::{ExitReceipt, ExitRequest, ExitSummary, ExitVerificationResult};
use crate::cl::BeaconClient;
use crate::cl::credentials::derive_address_from_credentials;
use crate::cl::types::ValidatorData;
use crate::constants::MAX_BLOCK_SCAN_FORWARD;
use crate::el::ElClient;
use crate::el::exit_predeploy::ExitPredeploy;
use crate::error::Result;
use crate::models::ConsolidationStatus;
use crate::utils::strip_0x;
use crate::utils::time::{is_epoch_finalized, slot_to_epoch, timestamp_to_slot};
use chrono::Utc;
use std::collections::HashMap;

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

        // Step 1: Collect unique public keys & query CL validator states
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
        let genesis_time = beacon_client
            .get_genesis()
            .await
            .ok()
            .and_then(|g| g.data.genesis_time.parse::<u64>().ok())
            .unwrap_or(1606824023);

        let finalized_epoch = beacon_client
            .get_finality_checkpoints("head")
            .await
            .ok()
            .and_then(|f| f.data.finalized.epoch.parse::<u64>().ok())
            .unwrap_or(0);

        // Step 3: Fetch EL Transaction Receipts & Blocks
        let (el_tx_evidence, estimated_start_slot) =
            fetch_el_tx_evidence(el_tx_hashes, el_client, genesis_time).await;

        // Step 4: Scan Beacon Blocks for EIP-7002 withdrawal requests
        let found_beacon_requests =
            scan_beacon_withdrawals(beacon_client, estimated_start_slot, finalized_epoch).await;

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

            // Match EL transaction
            let (matched_tx_hash, el_success, el_block) =
                match_el_transaction(req, &el_tx_evidence);

            let beacon_match = found_beacon_requests.get(&norm_pubkey);
            let (beacon_slot, beacon_block_root, finalized) = match beacon_match {
                Some((s, root, fin)) => (Some(*s), Some(root.clone()), *fin),
                None => (None, None, false),
            };

            let (status, rejection_reason) = evaluate_exit_status(
                val_data,
                matched_tx_hash.is_some(),
                el_success,
                beacon_match.is_some(),
                finalized,
            );

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

// -----------------------------------------------------------------------------
// Helper Functions
// -----------------------------------------------------------------------------

type ElTxEvidence = HashMap<String, (bool, u64, u64, Vec<u8>)>;

async fn fetch_el_tx_evidence(
    el_tx_hashes: &[String],
    el_client: &ElClient,
    genesis_time: u64,
) -> (ElTxEvidence, Option<u64>) {
    let mut el_tx_evidence = HashMap::new();
    let mut estimated_start_slot = None;

    for tx_hash in el_tx_hashes {
        if let Ok(Some(receipt)) = el_client.get_transaction_receipt(tx_hash).await {
            let success = receipt.status;
            let block_num = receipt.block_number;
            let mut timestamp = 0;
            let mut calldata = Vec::new();

            if let Ok(Some(tx)) = el_client.get_transaction_by_hash(tx_hash).await {
                let clean = strip_0x(&tx.input);
                calldata = hex::decode(clean).unwrap_or_default();
            }

            if let Ok(Some(block)) = el_client.get_block_by_number(block_num).await {
                timestamp = block.timestamp;
                if timestamp >= genesis_time {
                    let slot = timestamp_to_slot(timestamp, genesis_time);
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

    (el_tx_evidence, estimated_start_slot)
}

async fn scan_beacon_withdrawals(
    beacon_client: &BeaconClient,
    estimated_start_slot: Option<u64>,
    finalized_epoch: u64,
) -> HashMap<String, (u64, String, bool)> {
    let mut found_beacon_requests = HashMap::new();

    if let Some(start_slot) = estimated_start_slot {
        let max_slot = start_slot + MAX_BLOCK_SCAN_FORWARD;
        for slot in start_slot..=max_slot {
            if let Ok(Some(block_resp)) = beacon_client.get_beacon_block(&slot.to_string()).await {
                let block_msg = &block_resp.data.message;
                let block_root = block_msg.state_root.clone();
                let block_epoch = slot_to_epoch(slot);
                let is_finalized = is_epoch_finalized(block_epoch, finalized_epoch);

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

    found_beacon_requests
}

fn match_el_transaction(
    req: &ExitRequest,
    el_tx_evidence: &ElTxEvidence,
) -> (Option<String>, bool, Option<u64>) {
    for (tx_hash, (success, block_num, _ts, calldata)) in el_tx_evidence {
        let matched = ExitPredeploy::match_exits_in_calldata(calldata, std::slice::from_ref(req));
        if !matched.is_empty() {
            return (Some(tx_hash.clone()), *success, Some(*block_num));
        }
    }
    (None, false, None)
}

fn evaluate_exit_status(
    val_data: Option<&ValidatorData>,
    has_el_tx: bool,
    el_success: bool,
    in_beacon_block: bool,
    finalized: bool,
) -> (ConsolidationStatus, Option<String>) {
    if has_el_tx && !el_success {
        (
            ConsolidationStatus::NotAccepted,
            Some("Execution layer transaction reverted.".to_string()),
        )
    } else if let Some(true) = val_data.map(|d| d.validator.slashed) {
        (
            ConsolidationStatus::NotAccepted,
            Some("Validator is slashed and exit cannot be safely verified.".to_string()),
        )
    } else if in_beacon_block {
        if finalized {
            (ConsolidationStatus::Accepted, None)
        } else {
            (ConsolidationStatus::Queued, None)
        }
    } else if has_el_tx && el_success {
        (ConsolidationStatus::Queued, None)
    } else if val_data.is_none() {
        (
            ConsolidationStatus::Indeterminate,
            Some("Validator public key not found on Consensus Layer.".to_string()),
        )
    } else {
        (ConsolidationStatus::Indeterminate, None)
    }
}
