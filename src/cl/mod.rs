//! Consensus Layer (Beacon API) clients, data types, slot scanning, and state delta verifiers.

pub mod client;
pub mod credentials;
pub mod scanner;
pub mod state_delta;
pub mod types;
pub mod verifier;

pub use client::BeaconClient;
pub use credentials::derive_address_from_credentials;
pub use scanner::{fetch_beacon_blocks_for_timestamps, resolve_parent_state_root};
pub use state_delta::{StateDeltaMapping, resolve_state_deltas_for_blocks};
pub use types::{
    BeaconBlockBody, BeaconBlockData, BeaconBlockMessage, BeaconBlockResponse, Checkpoint,
    ClVerificationEvidence, ClVerifiedPairEvidence, ConsolidationRequestItem, ExecutionRequests,
    FinalityCheckpointsData, FinalityCheckpointsResponse, GenesisData, GenesisResponse,
    PendingConsolidationItem, PendingConsolidationsResponse, ValidatorData, ValidatorDetails,
    ValidatorsResponse, WithdrawalRequestItem,
};
pub use verifier::verify_consensus_layer;
