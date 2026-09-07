//! Orchestrator pipeline for cross-layer validator consolidation verification.

use super::evidence::build_raw_evidence;
use super::rules::build_pair_verification_result;
use crate::cl::{BeaconClient, verify_consensus_layer};
use crate::el::{ElClient, ElVerificationEvidence, verify_execution_layer};
use crate::error::Result;
use crate::lido::LidoRoleInspector;
use crate::models::{ConsolidationPair, VerificationReceipt, VerificationSummary};
use chrono::Utc;
use std::collections::HashMap;

/// Main verification engine for EIP-7251 validator consolidations.
pub struct VerificationEngine;

impl VerificationEngine {
    /// Executes full cross-layer verification across Execution and Consensus layers.
    pub async fn run_verification(
        manifest_pairs: &[ConsolidationPair],
        tx_hashes: &[String],
        el_client: &ElClient,
        beacon_client: &BeaconClient,
        st_vault_dashboard: Option<&str>,
    ) -> Result<VerificationReceipt> {
        // Step 1: Execute Execution Layer verification
        let el_evidence = verify_execution_layer(el_client, tx_hashes, manifest_pairs).await?;
        let el_block_timestamps = extract_el_block_timestamps(&el_evidence);

        // Step 2: Execute Consensus Layer state delta verification
        let cl_evidence =
            verify_consensus_layer(beacon_client, manifest_pairs, &el_block_timestamps).await?;

        // Step 3: Extract withdrawal credentials for every source validator
        let mut source_credentials = HashMap::with_capacity(manifest_pairs.len());
        for pair in manifest_pairs {
            let src_norm = pair.source_pubkey.to_lowercase();
            let creds = cl_evidence
                .validator_withdrawal_credentials
                .get(&src_norm)
                .cloned();
            source_credentials.insert(src_norm, creds);
        }

        let receipts: Vec<_> = el_evidence
            .verified_txs
            .values()
            .map(|v| v.receipt.clone())
            .collect();

        let tx_inputs: Vec<String> = el_evidence
            .verified_txs
            .values()
            .filter_map(|v| v.details.as_ref().map(|d| d.input.clone()))
            .collect();

        // Step 4: Audit Lido fee exemption role for derived accounts
        let fee_exemption = LidoRoleInspector::check_fee_exempt_roles(
            el_client,
            st_vault_dashboard,
            &source_credentials,
            &receipts,
            &tx_inputs,
        )
        .await?;

        // Step 5: Deterministically evaluate each consolidation pair
        let mut results = Vec::with_capacity(manifest_pairs.len());
        let mut summary = VerificationSummary {
            total_pairs: manifest_pairs.len(),
            ..Default::default()
        };

        for pair in manifest_pairs {
            let pair_result = build_pair_verification_result(pair, &el_evidence, &cl_evidence);
            summary.record_status(pair_result.status);
            results.push(pair_result);
        }

        // Step 6: Assemble raw evidence artifacts
        let raw_evidence = build_raw_evidence(&el_evidence, &cl_evidence);

        Ok(VerificationReceipt {
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: Utc::now(),
            el_rpc_url: el_client.rpc_url().to_string(),
            cl_beacon_url: beacon_client.base_url().to_string(),
            summary,
            fee_exemption,
            pairs: results,
            raw_evidence: Some(raw_evidence),
        })
    }
}

/// Extracts execution block timestamps for correlating with consensus slots.
fn extract_el_block_timestamps(el_evidence: &ElVerificationEvidence) -> HashMap<u64, u64> {
    let mut map = HashMap::with_capacity(el_evidence.verified_txs.len());
    for tx in el_evidence.verified_txs.values() {
        map.insert(tx.block_number, tx.block_timestamp);
    }
    map
}
