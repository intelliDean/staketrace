//! Credential extraction and Ethereum execution address derivation from BLS credentials.

use crate::utils::ensure_0x;

/// Extracts an execution address from 0x01 (or 0x02 compounding) withdrawal credentials.
///
/// In Ethereum proof-of-stake, `0x01` withdrawal credentials are 32 bytes (64 hex characters):
/// `[0x01 (1 byte) || 0x00...00 (11 bytes padding) || 20-byte execution address]`
pub fn derive_address_from_credentials(creds: &str) -> Option<String> {
    let trimmed = creds.trim();
    let raw = trimmed.strip_prefix("0x").unwrap_or(trimmed);

    if raw.len() != 64 {
        return None;
    }

    let prefix = &raw[0..2];
    // 0x01 = ETH1 withdrawal address, 0x02 = EIP-7251 compounding withdrawal address
    if prefix != "01" && prefix != "02" {
        return None;
    }

    let addr_part = &raw[24..64];
    Some(ensure_0x(addr_part))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_address_from_credentials() {
        let creds_01 = "0x01000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8";
        assert_eq!(
            derive_address_from_credentials(creds_01),
            Some("0x70997970c51812dc3a010c7d01b50e0d17dc79c8".to_string())
        );

        let creds_02 = "0x02000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8";
        assert_eq!(
            derive_address_from_credentials(creds_02),
            Some("0x70997970c51812dc3a010c7d01b50e0d17dc79c8".to_string())
        );
    }

    #[test]
    fn test_non_eth1_credentials() {
        let bls_creds = "0x00a1b2c3d4e5f60718293a4b5c6d7e8f00112233445566778899aabbccddeeff";
        assert_eq!(derive_address_from_credentials(bls_creds), None);
    }
}
