//! Centralized constants for Ethereum Consensus Layer, Execution Layer, and protocol predeploys.

/// Default Ethereum slot duration in seconds (post-Merge / PoS).
pub const SECONDS_PER_SLOT: u64 = 12;

/// Slots per epoch in Ethereum proof-of-stake.
pub const SLOTS_PER_EPOCH: u64 = 32;

/// Maximum number of subsequent Consensus Layer slots to scan for an execution request.
pub const MAX_SCAN_SLOTS: u64 = 64;

/// Standard forward scan limit when matching Beacon block requests.
pub const MAX_BLOCK_SCAN_FORWARD: u64 = 32;

/// Length of a BLS public key in bytes.
pub const BLS_PUBKEY_BYTE_LEN: usize = 48;

/// Expected length of a BLS public key in hex characters (48 bytes = 96 hex characters).
pub const BLS_PUBKEY_HEX_LEN: usize = 96;

/// Calldata length in bytes for an EIP-7251 consolidation pair ([source (48B) || target (48B)]).
pub const CONSOLIDATION_CALLDATA_LEN: usize = 96;

/// Calldata length in bytes for an EIP-7002 validator exit ([pubkey (48B) || amount (8B)]).
pub const EXIT_CALLDATA_LEN: usize = 56;

/// EIP-7251 (MaxEB) Consolidation Request Predeploy contract address.
pub const CONSOLIDATION_PREDEPLOY_ADDRESS: &str = "0x0000bbddc7ce488642fb579f8b00f3a590007251";

/// EIP-7002 Validator Exit & Partial Withdrawal Predeploy contract address.
pub const EXIT_PREDEPLOY_ADDRESS: &str = "0x0000bbddc7ce488642fb579f8b00f3a590007002";

/// Minimum validator activation balance in Gwei (32 ETH).
pub const MIN_ACTIVATION_BALANCE_GWEI: u64 = 32_000_000_000;

/// MaxEB validator maximum effective balance in Gwei (2,048 ETH).
pub const MAX_EFFECTIVE_BALANCE_GWEI: u64 = 2_048_000_000_000;
