use super::models::{
    ESTIMATED_GAS_PER_CONSOLIDATION, MAX_EFFECTIVE_BALANCE_ELECTRA_GWEI,
    MIN_ACTIVATION_BALANCE_GWEI, PairSimulationResult, SimulationReport, SimulationSummary,
};
use crate::cl::BeaconClient;
use crate::cl::verifier::derive_address_from_credentials;
use crate::error::Result;
use crate::models::ConsolidationPair;
use chrono::Utc;
use std::collections::{HashMap, HashSet};

pub struct SimulationEngine;

impl SimulationEngine {
    /// Executes pre-flight validation and EIP-7251 safety simulation for a consolidation manifest.
    pub async fn run_simulation(
        beacon_client: &BeaconClient,
        manifest_pairs: &[ConsolidationPair],
        el_rpc_url: Option<&str>,
    ) -> Result<SimulationReport> {
        if manifest_pairs.is_empty() {
            return Ok(SimulationReport {
                tool_version: env!("CARGO_PKG_VERSION").to_string(),
                timestamp: Utc::now(),
                cl_beacon_url: beacon_client.base_url().to_string(),
                el_rpc_url: el_rpc_url.map(String::from),
                summary: SimulationSummary::default(),
                pairs: Vec::new(),
            });
        }

        // Step 1: Collect unique public keys
        let mut unique_pubkeys = HashSet::with_capacity(manifest_pairs.len() * 2);
        for pair in manifest_pairs {
            unique_pubkeys.insert(pair.source_pubkey.clone());
            unique_pubkeys.insert(pair.target_pubkey.clone());
        }
        let pubkeys_vec: Vec<String> = unique_pubkeys.into_iter().collect();

        // Step 2: Fetch full validator data from Beacon Chain
        let validators_map = beacon_client
            .get_validators_full_data(&pubkeys_vec)
            .await
            .unwrap_or_default();

        // Step 3: Fetch current head pending consolidations queue
        let head_pending = beacon_client
            .get_pending_consolidations("head")
            .await
            .unwrap_or_default();

        // Step 4: Track accumulated target balances to detect MaxEB cap overflow
        let mut target_accumulated_gwei: HashMap<String, u64> = HashMap::new();
        for pair in manifest_pairs {
            let tgt_norm = pair.target_pubkey.to_lowercase();
            if !target_accumulated_gwei.contains_key(&tgt_norm) {
                let init_balance = validators_map
                    .get(&tgt_norm)
                    .and_then(|v| v.validator.effective_balance.parse::<u64>().ok())
                    .unwrap_or(0);
                target_accumulated_gwei.insert(tgt_norm.clone(), init_balance);
            }
        }

        // Step 5: Simulate each consolidation pair
        let mut pair_results = Vec::with_capacity(manifest_pairs.len());
        let mut summary = SimulationSummary {
            total_pairs: manifest_pairs.len(),
            estimated_total_gas: (manifest_pairs.len() as u64) * ESTIMATED_GAS_PER_CONSOLIDATION,
            ..Default::default()
        };

        for pair in manifest_pairs {
            let src_norm = pair.source_pubkey.to_lowercase();
            let tgt_norm = pair.target_pubkey.to_lowercase();

            let src_data = validators_map.get(&src_norm);
            let tgt_data = validators_map.get(&tgt_norm);

            let src_idx = src_data.and_then(|d| d.index.parse::<u64>().ok());
            let tgt_idx = tgt_data.and_then(|d| d.index.parse::<u64>().ok());

            let src_status = src_data.map(|d| d.status.clone());
            let tgt_status = tgt_data.map(|d| d.status.clone());

            let src_balance_gwei =
                src_data.and_then(|d| d.validator.effective_balance.parse::<u64>().ok());
            let tgt_balance_gwei =
                tgt_data.and_then(|d| d.validator.effective_balance.parse::<u64>().ok());

            let src_creds = src_data.map(|d| d.validator.withdrawal_credentials.clone());
            let tgt_creds = tgt_data.map(|d| d.validator.withdrawal_credentials.clone());

            let src_addr = src_creds
                .as_deref()
                .and_then(derive_address_from_credentials);
            let tgt_addr = tgt_creds
                .as_deref()
                .and_then(derive_address_from_credentials);

            let src_slashed = src_data.map(|d| d.validator.slashed);
            let tgt_slashed = tgt_data.map(|d| d.validator.slashed);

            let mut warnings = Vec::new();
            let mut rejection_reason = None;

            // Check existence
            if src_data.is_none() {
                rejection_reason =
                    Some("Source validator public key not found on Consensus Layer.".to_string());
            } else if tgt_data.is_none() {
                rejection_reason =
                    Some("Target validator public key not found on Consensus Layer.".to_string());
            }

            // Check slashed status
            if rejection_reason.is_none() {
                if src_slashed == Some(true) {
                    rejection_reason =
                        Some("Source validator is slashed and cannot be consolidated.".to_string());
                } else if tgt_slashed == Some(true) {
                    rejection_reason = Some(
                        "Target validator is slashed and cannot receive consolidations."
                            .to_string(),
                    );
                }
            }

            // Check withdrawal credentials compatibility
            let mut credentials_match = false;
            if rejection_reason.is_none() {
                match (src_addr.as_deref(), tgt_addr.as_deref()) {
                    (Some(s_addr), Some(t_addr)) => {
                        if s_addr.eq_ignore_ascii_case(t_addr) {
                            credentials_match = true;
                        } else {
                            rejection_reason = Some(format!(
                                "Withdrawal credential mismatch: Source ({}) does not match Target ({}).",
                                s_addr, t_addr
                            ));
                        }
                    }
                    (None, _) => {
                        rejection_reason = Some(
                            "Source validator has 0x00 BLS credentials; must upgrade to 0x01/0x02 before consolidating."
                                .to_string(),
                        );
                    }
                    (_, None) => {
                        rejection_reason = Some(
                            "Target validator has 0x00 BLS credentials; must upgrade to 0x01/0x02 before consolidating."
                                .to_string(),
                        );
                    }
                }
            }

            // Check minimum source balance (32 ETH)
            if rejection_reason.is_none() {
                if let Some(bal) = src_balance_gwei {
                    if bal < MIN_ACTIVATION_BALANCE_GWEI {
                        rejection_reason = Some(format!(
                            "Source validator effective balance ({} Gwei) is below 32 ETH minimum activation threshold.",
                            bal
                        ));
                    }
                }
            }

            // Check queue collision in head pending consolidations
            let mut already_pending = false;
            if let (Some(s_idx), Some(t_idx)) = (src_idx, tgt_idx) {
                already_pending = head_pending.iter().any(|item| {
                    item.source_index.parse::<u64>().ok() == Some(s_idx)
                        && item.target_index.parse::<u64>().ok() == Some(t_idx)
                });
                if already_pending {
                    warnings.push("Consolidation pair is already queued in pending_consolidations at chain head.".to_string());
                }
            }

            // Calculate projected balance & MaxEB cap overflow
            let mut projected_target_balance_gwei = None;
            let mut exceeds_max_eb = false;
            if rejection_reason.is_none() {
                if let (Some(src_bal), Some(current_acc)) =
                    (src_balance_gwei, target_accumulated_gwei.get_mut(&tgt_norm))
                {
                    *current_acc += src_bal;
                    projected_target_balance_gwei = Some(*current_acc);

                    if *current_acc > MAX_EFFECTIVE_BALANCE_ELECTRA_GWEI {
                        exceeds_max_eb = true;
                        warnings.push(format!(
                            "Projected target balance ({} Gwei) exceeds 2,048 ETH MaxEB cap. Excess balance will not earn additional staking yield.",
                            current_acc
                        ));
                    }
                }
            }

            let eligible = rejection_reason.is_none();
            if eligible {
                summary.eligible_pairs += 1;
                if let Some(src_bal) = src_balance_gwei {
                    summary.total_source_balance_gwei += src_bal;
                }
                if let Some(tgt_bal) = tgt_balance_gwei {
                    summary.total_target_balance_gwei =
                        summary.total_target_balance_gwei.max(tgt_bal);
                }
            } else {
                summary.ineligible_pairs += 1;
            }

            if !warnings.is_empty() {
                summary.warning_count += warnings.len();
            }

            pair_results.push(PairSimulationResult {
                source_pubkey: pair.source_pubkey.clone(),
                source_index: src_idx,
                source_status: src_status,
                source_effective_balance_gwei: src_balance_gwei,
                source_withdrawal_credentials: src_creds,
                source_derived_address: src_addr,
                source_slashed: src_slashed,

                target_pubkey: pair.target_pubkey.clone(),
                target_index: tgt_idx,
                target_status: tgt_status,
                target_effective_balance_gwei: tgt_balance_gwei,
                target_withdrawal_credentials: tgt_creds,
                target_derived_address: tgt_addr,
                target_slashed: tgt_slashed,

                credentials_match,
                already_pending,
                projected_target_balance_gwei,
                exceeds_max_eb,
                eligible,
                rejection_reason,
                warnings,
            });
        }

        summary.projected_target_balance_gwei =
            summary.total_target_balance_gwei + summary.total_source_balance_gwei;

        Ok(SimulationReport {
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: Utc::now(),
            cl_beacon_url: beacon_client.base_url().to_string(),
            el_rpc_url: el_rpc_url.map(String::from),
            summary,
            pairs: pair_results,
        })
    }
}
