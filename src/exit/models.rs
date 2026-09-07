use crate::models::ConsolidationStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Request descriptor for a validator exit or partial withdrawal under EIP-7002.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExitRequest {
    /// 48-byte BLS public key of the validator (0x-prefixed hex string).
    pub pubkey: String,
    /// Requested withdrawal amount in Gwei (0 = full exit, > 0 = partial withdrawal).
    #[serde(default)]
    pub amount_gwei: u64,
    /// Optional expected source address that authorized the exit.
    #[serde(default)]
    pub source_address: Option<String>,
}

impl ExitRequest {
    pub fn new(pubkey: impl Into<String>, amount_gwei: u64) -> Self {
        Self {
            pubkey: pubkey.into(),
            amount_gwei,
            source_address: None,
        }
    }

    /// Normalized lowercase 0x-prefixed public key.
    pub fn normalized_pubkey(&self) -> String {
        let p = self.pubkey.trim();
        if p.starts_with("0x") || p.starts_with("0X") {
            p.to_lowercase()
        } else {
            format!("0x{}", p.to_lowercase())
        }
    }
}

/// Verification metrics summary for an EIP-7002 exit batch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExitSummary {
    pub total_exits: usize,
    pub accepted: usize,
    pub queued: usize,
    pub not_accepted: usize,
    pub indeterminate: usize,
}

impl ExitSummary {
    /// Returns `true` if all exits in the batch were proven `ACCEPTED`.
    pub fn is_all_accepted(&self) -> bool {
        self.accepted == self.total_exits && self.total_exits > 0
    }
}

/// Detailed cross-layer verification result for a single validator exit request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitVerificationResult {
    pub pubkey: String,
    pub validator_index: Option<u64>,
    pub amount_gwei: u64,
    pub is_full_exit: bool,
    pub withdrawal_credentials: Option<String>,
    pub derived_source_address: Option<String>,
    pub status: ConsolidationStatus,
    pub el_tx_hash: Option<String>,
    pub el_block_number: Option<u64>,
    pub beacon_slot: Option<u64>,
    pub beacon_block_root: Option<String>,
    pub finalized: bool,
    pub rejection_reason: Option<String>,
}

/// Complete machine-readable audit receipt for an EIP-7002 exit verification run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitReceipt {
    pub tool_version: String,
    pub timestamp: DateTime<Utc>,
    pub el_rpc_url: String,
    pub cl_beacon_url: String,
    pub summary: ExitSummary,
    pub exits: Vec<ExitVerificationResult>,
}
