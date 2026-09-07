//! EIP-7251 validator consolidation manifest parser and deserializers.

use crate::error::{AppError, Result};
use crate::models::ConsolidationPair;
use crate::utils::validate_bls_pubkey;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ManifestFormat {
    /// Direct list of pair items (`[{ "source": "...", "target": "..." }]`)
    DirectList(Vec<PairItem>),
    /// Direct list of target-with-sources (`[{ "target_pubkey": "...", "source_pubkeys": ["..."] }]`)
    TargetWithSourcesList(Vec<TargetWithSourcesItem>),
    /// Official Lido map of target -> list of sources (`{ "0xTargetPubkey": ["0xSource1", "0xSource2"] }`)
    TargetToSourcesMap(HashMap<String, Vec<String>>),
    /// Object containing a "pairs" list (`{ "pairs": [...] }`)
    PairsWrapper { pairs: Vec<PairItem> },
    /// Object containing a "consolidations" list (`{ "consolidations": [...] }`)
    ConsolidationsWrapper { consolidations: Vec<PairItem> },
}

impl ManifestFormat {
    /// Unwraps the parsed manifest format into a uniform list of `(source, target)` public key tuples.
    fn into_raw_pairs(self) -> Vec<(String, String)> {
        match self {
            Self::DirectList(list) => list.into_iter().map(|p| (p.source, p.target)).collect(),
            Self::TargetWithSourcesList(list) => {
                let mut pairs = Vec::new();
                for item in list {
                    for src in item.source_pubkeys {
                        pairs.push((src, item.target_pubkey.clone()));
                    }
                }
                pairs
            }
            Self::TargetToSourcesMap(map) => {
                let mut pairs = Vec::new();
                for (target, sources) in map {
                    for src in sources {
                        pairs.push((src, target.clone()));
                    }
                }
                pairs
            }
            Self::PairsWrapper { pairs } => {
                pairs.into_iter().map(|p| (p.source, p.target)).collect()
            }
            Self::ConsolidationsWrapper { consolidations } => consolidations
                .into_iter()
                .map(|p| (p.source, p.target))
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct PairItem {
    #[serde(
        alias = "source",
        alias = "source_pubkey",
        alias = "sourcePubkey",
        alias = "source_validator_pubkey"
    )]
    source: String,
    #[serde(
        alias = "target",
        alias = "target_pubkey",
        alias = "targetPubkey",
        alias = "target_validator_pubkey"
    )]
    target: String,
}

#[derive(Debug, Deserialize)]
struct TargetWithSourcesItem {
    #[serde(
        alias = "target",
        alias = "target_pubkey",
        alias = "targetPubkey",
        alias = "target_validator_pubkey"
    )]
    target_pubkey: String,
    #[serde(
        alias = "sources",
        alias = "source_pubkeys",
        alias = "sourcePubkeys",
        alias = "source_validators"
    )]
    source_pubkeys: Vec<String>,
}

/// Parses and validates an EIP-7251 consolidation manifest file (JSON or YAML).
pub fn parse_manifest_file<P: AsRef<Path>>(path: P) -> Result<Vec<ConsolidationPair>> {
    let p = path.as_ref();
    let content = std::fs::read_to_string(p).map_err(|e| {
        AppError::Manifest(format!(
            "Failed to read manifest file '{}': {}",
            p.display(),
            e
        ))
    })?;

    parse_manifest_str(&content)
}

/// Parses and validates an EIP-7251 consolidation manifest string (JSON or YAML).
pub fn parse_manifest_str(content: &str) -> Result<Vec<ConsolidationPair>> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(AppError::Manifest("Manifest content is empty".to_string()));
    }

    let raw_pairs = serde_json::from_str::<ManifestFormat>(trimmed)
        .map(|f| f.into_raw_pairs())
        .or_else(|_| serde_yaml::from_str::<ManifestFormat>(trimmed).map(|f| f.into_raw_pairs()))
        .map_err(|e| {
            AppError::Manifest(format!(
                "Failed to parse manifest JSON or YAML format: {}",
                e
            ))
        })?;

    if raw_pairs.is_empty() {
        return Err(AppError::Manifest(
            "Manifest contains no consolidation pairs".to_string(),
        ));
    }

    let mut validated_pairs = Vec::with_capacity(raw_pairs.len());
    for (idx, (src, tgt)) in raw_pairs.into_iter().enumerate() {
        let valid_src = validate_bls_pubkey(&src).map_err(|e| {
            AppError::Manifest(format!(
                "Invalid source public key at pair #{}: {}",
                idx + 1,
                e
            ))
        })?;
        let valid_tgt = validate_bls_pubkey(&tgt).map_err(|e| {
            AppError::Manifest(format!(
                "Invalid target public key at pair #{}: {}",
                idx + 1,
                e
            ))
        })?;
        validated_pairs.push(ConsolidationPair::new(valid_src, valid_tgt));
    }

    Ok(validated_pairs)
}
