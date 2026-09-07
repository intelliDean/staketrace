//! Manifest parsing module for consolidation pairs (EIP-7251) and validator exits (EIP-7002).

pub mod consolidation;
pub mod exit;

pub use consolidation::{parse_manifest_file, parse_manifest_str};
pub use exit::{parse_exit_manifest_file, parse_exit_manifest_str};

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SRC: &str = "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
    const SAMPLE_TGT: &str = "0x96b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c488";

    #[test]
    fn test_parse_json_direct_list() {
        let json = format!(
            r#"[{{"source": "{}", "target": "{}"}}]"#,
            SAMPLE_SRC, SAMPLE_TGT
        );
        let pairs = parse_manifest_str(&json).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].source_pubkey, SAMPLE_SRC);
        assert_eq!(pairs[0].target_pubkey, SAMPLE_TGT);
    }

    #[test]
    fn test_parse_official_lido_map_format() {
        let json = format!(r#"{{"{}": ["{}"]}}"#, SAMPLE_TGT, SAMPLE_SRC);
        let pairs = parse_manifest_str(&json).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].source_pubkey, SAMPLE_SRC);
        assert_eq!(pairs[0].target_pubkey, SAMPLE_TGT);
    }

    #[test]
    fn test_parse_target_with_sources_list() {
        let json = format!(
            r#"[{{"target_pubkey": "{}", "source_pubkeys": ["{}"]}}]"#,
            SAMPLE_TGT, SAMPLE_SRC
        );
        let pairs = parse_manifest_str(&json).unwrap();
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn test_parse_json_pairs_wrapper() {
        let json = format!(
            r#"{{"pairs": [{{"source_pubkey": "{}", "target_pubkey": "{}"}}]}}"#,
            SAMPLE_SRC, SAMPLE_TGT
        );
        let pairs = parse_manifest_str(&json).unwrap();
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn test_parse_yaml_format() {
        let yaml = format!(
            "- source: \"{}\"\n  target: \"{}\"\n",
            SAMPLE_SRC, SAMPLE_TGT
        );
        let pairs = parse_manifest_str(&yaml).unwrap();
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn test_invalid_pubkey_length() {
        let json = format!(r#"[{{"source": "0x1234", "target": "{}"}}]"#, SAMPLE_TGT);
        let err = parse_manifest_str(&json).unwrap_err();
        assert!(err.to_string().contains("Invalid public key length"));
    }

    #[test]
    fn test_invalid_hex_characters() {
        let bad_src = SAMPLE_SRC.replace('a', "z");
        let json = format!(
            r#"[{{"source": "{}", "target": "{}"}}]"#,
            bad_src, SAMPLE_TGT
        );
        let err = parse_manifest_str(&json).unwrap_err();
        assert!(err.to_string().contains("non-hexadecimal"));
    }
}
