//! Hex formatting, parsing, and normalization utilities.

use crate::constants::BLS_PUBKEY_HEX_LEN;
use crate::error::{AppError, Result};

/// Strips any leading "0x" or "0X" prefix from a hex string.
#[inline]
pub fn strip_0x(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(stripped) = trimmed.strip_prefix("0x") {
        stripped
    } else if let Some(stripped) = trimmed.strip_prefix("0X") {
        stripped
    } else {
        trimmed
    }
}

/// Ensures a hex string is lowercase and prefixed with "0x".
pub fn ensure_0x(s: &str) -> String {
    let stripped = strip_0x(s);
    let mut out = String::with_capacity(stripped.len() + 2);
    out.push_str("0x");
    out.push_str(&stripped.to_lowercase());
    out
}

/// Normalizes a BLS public key string into a lowercase `0x`-prefixed 96-char hex string.
pub fn normalize_pubkey(pubkey: &str) -> String {
    ensure_0x(pubkey)
}

/// Validates that a string is a valid BLS public key (96 hex characters, ignoring 0x prefix).
pub fn validate_bls_pubkey(pubkey: &str) -> Result<String> {
    let raw = strip_0x(pubkey);
    if raw.len() != BLS_PUBKEY_HEX_LEN {
        return Err(AppError::Manifest(format!(
            "Invalid public key length: expected {} hex characters, found {} in '{}'",
            BLS_PUBKEY_HEX_LEN,
            raw.len(),
            pubkey
        )));
    }
    if !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Manifest(format!(
            "Invalid public key '{}': contains non-hexadecimal characters",
            pubkey
        )));
    }
    Ok(ensure_0x(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_and_ensure_0x() {
        assert_eq!(strip_0x("0xABCD"), "ABCD");
        assert_eq!(strip_0x("0X1234"), "1234");
        assert_eq!(strip_0x("abcd"), "abcd");
        assert_eq!(ensure_0x("ABCD"), "0xabcd");
        assert_eq!(ensure_0x("0xABCD"), "0xabcd");
    }

    #[test]
    fn test_validate_bls_pubkey() {
        let valid_96 = "8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
        assert!(validate_bls_pubkey(valid_96).is_ok());
        assert!(validate_bls_pubkey(&format!("0x{}", valid_96)).is_ok());
        assert!(validate_bls_pubkey("too_short").is_err());
        assert!(validate_bls_pubkey(&valid_96.replace('a', "z")).is_err());
    }
}
