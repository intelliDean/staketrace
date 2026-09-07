use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Minimum effective balance required for an active source validator (32 ETH in Gwei).
pub const MIN_ACTIVATION_BALANCE_GWEI: u64 = 32_000_000_000;

/// Maximum effective balance allowed under EIP-7251 MaxEB (2,048 ETH in Gwei).
pub const MAX_EFFECTIVE_BALANCE_ELECTRA_GWEI: u64 = 2_048_000_000_000;

/// Estimated execution layer gas required for a single consolidation predeploy call.
pub const ESTIMATED_GAS_PER_CONSOLIDATION: u64 = 65_000;

/// Summary metrics for the batch simulation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimulationSummary {
    pub total_pairs: usize,
    pub eligible_pairs: usize,
    pub ineligible_pairs: usize,
    pub warning_count: usize,
    pub total_source_balance_gwei: u64,
    pub total_target_balance_gwei: u64,
    pub projected_target_balance_gwei: u64,
    pub estimated_total_gas: u64,
}

impl SimulationSummary {
    /// Returns `true` if all pairs in the batch are eligible for consolidation without errors.
    pub fn is_all_eligible(&self) -> bool {
        self.eligible_pairs == self.total_pairs && self.total_pairs > 0
    }
}

/// Simulation result for a single consolidation pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairSimulationResult {
    pub source_pubkey: String,
    pub source_index: Option<u64>,
    pub source_status: Option<String>,
    pub source_effective_balance_gwei: Option<u64>,
    pub source_withdrawal_credentials: Option<String>,
    pub source_derived_address: Option<String>,
    pub source_slashed: Option<bool>,

    pub target_pubkey: String,
    pub target_index: Option<u64>,
    pub target_status: Option<String>,
    pub target_effective_balance_gwei: Option<u64>,
    pub target_withdrawal_credentials: Option<String>,
    pub target_derived_address: Option<String>,
    pub target_slashed: Option<bool>,

    pub credentials_match: bool,
    pub already_pending: bool,
    pub projected_target_balance_gwei: Option<u64>,
    pub exceeds_max_eb: bool,
    pub eligible: bool,
    pub rejection_reason: Option<String>,
    pub warnings: Vec<String>,
}

/// Complete machine-readable simulation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationReport {
    pub tool_version: String,
    pub timestamp: DateTime<Utc>,
    pub cl_beacon_url: String,
    pub el_rpc_url: Option<String>,
    pub summary: SimulationSummary,
    pub pairs: Vec<PairSimulationResult>,
}
