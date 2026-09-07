//! EIP-7002 validator exit manifest parser and deserializers.

use crate::error::{AppError, Result};
use crate::exit::models::ExitRequest;
use crate::utils::validate_bls_pubkey;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ExitManifestFormat {
    /// Format 1: Direct list of pubkey strings (`["0x...", "0x..."]`)
    StringList(Vec<String>),
    /// Format 2: Direct list of exit request items (`[{ "pubkey": "...", "amount": 0 }]`)
    ItemList(Vec<ExitManifestItem>),
    /// Format 3: Object containing "exits" or "requests"
    ExitsWrapper {
        #[serde(alias = "requests", alias = "withdrawals")]
        exits: Vec<ExitManifestItem>,
    },
}

impl ExitManifestFormat {
    fn into_raw_items(self) -> Vec<(String, u64)> {
        match self {
            Self::StringList(list) => list.into_iter().map(|p| (p, 0)).collect(),
            Self::ItemList(list) => list
                .into_iter()
                .map(|item| (item.pubkey, item.amount.unwrap_or(0)))
                .collect(),
            Self::ExitsWrapper { exits } => exits
                .into_iter()
                .map(|item| (item.pubkey, item.amount.unwrap_or(0)))
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ExitManifestItem {
    #[serde(
        alias = "pubkey",
        alias = "validator_pubkey",
        alias = "validatorPubkey"
    )]
    pubkey: String,
    #[serde(alias = "amount", alias = "amount_gwei", alias = "amountGwei", default)]
    amount: Option<u64>,
}

/// Parses and validates an EIP-7002 validator exit manifest file (JSON or YAML).
pub fn parse_exit_manifest_file<P: AsRef<Path>>(path: P) -> Result<Vec<ExitRequest>> {
    let p = path.as_ref();
    let content = std::fs::read_to_string(p).map_err(|e| {
        AppError::Manifest(format!(
            "Failed to read exit manifest file '{}': {}",
            p.display(),
            e
        ))
    })?;

    parse_exit_manifest_str(&content)
}

/// Parses and validates an EIP-7002 validator exit manifest string (JSON or YAML).
pub fn parse_exit_manifest_str(content: &str) -> Result<Vec<ExitRequest>> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(AppError::Manifest(
            "Exit manifest content is empty".to_string(),
        ));
    }

    let raw_items: Vec<(String, u64)> = serde_json::from_str::<ExitManifestFormat>(trimmed)
        .map(|f| f.into_raw_items())
        .or_else(|_| {
            serde_yaml::from_str::<ExitManifestFormat>(trimmed).map(|f| f.into_raw_items())
        })
        .map_err(|e| {
            AppError::Manifest(format!(
                "Failed to parse exit manifest JSON or YAML format: {}",
                e
            ))
        })?;

    if raw_items.is_empty() {
        return Err(AppError::Manifest(
            "Exit manifest contains no validator exit requests".to_string(),
        ));
    }

    let mut validated_requests = Vec::with_capacity(raw_items.len());
    for (idx, (pubkey, amount_gwei)) in raw_items.into_iter().enumerate() {
        let valid_pubkey = validate_bls_pubkey(&pubkey).map_err(|e| {
            AppError::Manifest(format!(
                "Invalid validator public key at exit request #{}: {}",
                idx + 1,
                e
            ))
        })?;
        validated_requests.push(ExitRequest::new(valid_pubkey, amount_gwei));
    }

    Ok(validated_requests)
}
