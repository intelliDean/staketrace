use crate::exit::models::ExitRequest;

/// EIP-7002 Execution Layer Triggerable Exit Predeploy contract address on Ethereum.
pub const EXIT_PREDEPLOY_ADDRESS: &str = "0x0000bbddc7ce488642fb579f8b00f3a590007002";

/// Length of a BLS12-381 public key in bytes.
pub const PUBKEY_BYTE_LEN: usize = 48;

/// Expected calldata length for an EIP-7002 exit request with explicit amount (48B pubkey + 8B amount).
pub const EXIT_CALLDATA_LEN: usize = 56;

pub struct ExitPredeploy;

impl ExitPredeploy {
    /// Checks if an address matches the EIP-7002 exit predeploy address.
    pub fn is_predeploy_address(address: &str) -> bool {
        let clean = address
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X");

        clean.eq_ignore_ascii_case("0000bbddc7ce488642fb579f8b00f3a590007002")
    }

    /// Decodes an EIP-7002 calldata payload (either 56-byte pubkey+amount or 48-byte pubkey).
    pub fn decode_exit_calldata(data: &[u8]) -> Option<ExitRequest> {
        if data.len() == EXIT_CALLDATA_LEN {
            let pubkey_bytes = &data[0..PUBKEY_BYTE_LEN];
            let mut amount_bytes = [0u8; 8];
            amount_bytes.copy_from_slice(&data[PUBKEY_BYTE_LEN..EXIT_CALLDATA_LEN]);
            let amount = u64::from_be_bytes(amount_bytes);

            Some(ExitRequest {
                pubkey: format!("0x{}", hex::encode(pubkey_bytes)),
                amount_gwei: amount,
                source_address: None,
            })
        } else if data.len() == PUBKEY_BYTE_LEN {
            Some(ExitRequest {
                pubkey: format!("0x{}", hex::encode(data)),
                amount_gwei: 0,
                source_address: None,
            })
        } else {
            None
        }
    }

    /// Scans calldata for occurrences of requested validator pubkeys.
    pub fn match_exits_in_calldata(calldata: &[u8], requests: &[ExitRequest]) -> Vec<ExitRequest> {
        if calldata.len() < PUBKEY_BYTE_LEN {
            return Vec::new();
        }

        let mut matched = Vec::with_capacity(requests.len());

        for req in requests {
            let clean = req.pubkey.trim_start_matches("0x").trim_start_matches("0X");

            if let Ok(pubkey_bytes) = hex::decode(clean) {
                if pubkey_bytes.len() == PUBKEY_BYTE_LEN
                    && contains_subslice(calldata, &pubkey_bytes)
                {
                    matched.push(req.clone());
                }
            }
        }

        matched
    }
}

/// Fast search for a needle subslice inside haystack bytes.
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PUBKEY: &str = "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";

    #[test]
    fn test_is_exit_predeploy_address() {
        assert!(ExitPredeploy::is_predeploy_address(
            "0x0000BBdDc7CE488642fb579F8B00f3a590007002"
        ));
        assert!(ExitPredeploy::is_predeploy_address(
            "0000bbddc7ce488642fb579f8b00f3a590007002"
        ));
        assert!(!ExitPredeploy::is_predeploy_address(
            "0x0000BBdDc7CE488642fb579F8B00f3a590007251"
        ));
    }

    #[test]
    fn test_decode_56_byte_exit_calldata() {
        let pubkey_bytes = hex::decode(&SAMPLE_PUBKEY[2..]).unwrap();
        let mut calldata = pubkey_bytes;
        calldata.extend_from_slice(&32_000_000_000u64.to_be_bytes());

        let decoded = ExitPredeploy::decode_exit_calldata(&calldata).expect("should decode");
        assert_eq!(decoded.pubkey.to_lowercase(), SAMPLE_PUBKEY.to_lowercase());
        assert_eq!(decoded.amount_gwei, 32_000_000_000);
    }

    #[test]
    fn test_match_exits_in_calldata() {
        let pubkey_bytes = hex::decode(&SAMPLE_PUBKEY[2..]).unwrap();
        let mut calldata = vec![0xaa, 0xbb];
        calldata.extend_from_slice(&pubkey_bytes);
        calldata.extend_from_slice(&[0x00; 8]);

        let req = ExitRequest::new(SAMPLE_PUBKEY, 0);
        let matched = ExitPredeploy::match_exits_in_calldata(&calldata, std::slice::from_ref(&req));
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].pubkey, req.pubkey);
    }
}
