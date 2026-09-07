use serde_json::json;
use staketrace::{
    BeaconClient, SimulationEngine, generate_simulation_markdown, parse_manifest_str,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SRC_PUBKEY: &str = "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
const TGT_PUBKEY: &str = "0x96b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c488";
const VALID_CREDS_1: &str = "0x01000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8";
const VALID_CREDS_2: &str = "0x01000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8";
const MISMATCHED_CREDS: &str = "0x0100000000000000000000003c44cdddb6a900fa2b585dd299e03d12fa4293bc";
const TYPE0_CREDS: &str = "0x00000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8";

#[tokio::test]
async fn test_simulation_eligible_pair() {
    let cl_server = MockServer::start().await;

    // Mock GET /eth/v1/beacon/states/head/validators with source & target
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "100",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SRC_PUBKEY,
                        "withdrawal_credentials": VALID_CREDS_1,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                },
                {
                    "index": "200",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": TGT_PUBKEY,
                        "withdrawal_credentials": VALID_CREDS_2,
                        "effective_balance": "64000000000",
                        "slashed": false
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    // Mock empty pending consolidations
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": []
        })))
        .mount(&cl_server)
        .await;

    let manifest_yaml = format!(
        "consolidations:\n  - source_pubkey: \"{}\"\n    target_pubkey: \"{}\"\n",
        SRC_PUBKEY, TGT_PUBKEY
    );
    let pairs = parse_manifest_str(&manifest_yaml).unwrap();
    let beacon_client = BeaconClient::new(cl_server.uri());

    let report = SimulationEngine::run_simulation(&beacon_client, &pairs, None)
        .await
        .unwrap();

    assert_eq!(report.summary.total_pairs, 1);
    assert_eq!(report.summary.eligible_pairs, 1);
    assert_eq!(report.summary.ineligible_pairs, 0);
    assert!(report.summary.is_all_eligible());
    assert_eq!(report.summary.total_source_balance_gwei, 32_000_000_000);
    assert_eq!(report.summary.total_target_balance_gwei, 64_000_000_000);
    assert_eq!(report.summary.projected_target_balance_gwei, 96_000_000_000);
    assert_eq!(report.summary.estimated_total_gas, 65_000);

    let pair_res = &report.pairs[0];
    assert!(pair_res.eligible);
    assert!(pair_res.credentials_match);
    assert_eq!(pair_res.source_index, Some(100));
    assert_eq!(pair_res.target_index, Some(200));
    assert!(pair_res.rejection_reason.is_none());

    let md = generate_simulation_markdown(&report);
    assert!(md.contains("SIMULATION PASSED"));
    assert!(md.contains("Estimated Total Gas"));
}

#[tokio::test]
async fn test_simulation_mismatched_credentials() {
    let cl_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "100",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SRC_PUBKEY,
                        "withdrawal_credentials": VALID_CREDS_1,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                },
                {
                    "index": "200",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": TGT_PUBKEY,
                        "withdrawal_credentials": MISMATCHED_CREDS,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&cl_server)
        .await;

    let manifest_yaml = format!(
        "consolidations:\n  - source_pubkey: \"{}\"\n    target_pubkey: \"{}\"\n",
        SRC_PUBKEY, TGT_PUBKEY
    );
    let pairs = parse_manifest_str(&manifest_yaml).unwrap();
    let beacon_client = BeaconClient::new(cl_server.uri());

    let report = SimulationEngine::run_simulation(&beacon_client, &pairs, None)
        .await
        .unwrap();

    assert_eq!(report.summary.eligible_pairs, 0);
    assert_eq!(report.summary.ineligible_pairs, 1);
    assert!(!report.summary.is_all_eligible());

    let pair_res = &report.pairs[0];
    assert!(!pair_res.eligible);
    assert!(!pair_res.credentials_match);
    assert!(
        pair_res
            .rejection_reason
            .as_ref()
            .unwrap()
            .contains("Withdrawal credential mismatch")
    );
}

#[tokio::test]
async fn test_simulation_type_0_credentials_rejection() {
    let cl_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "100",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SRC_PUBKEY,
                        "withdrawal_credentials": TYPE0_CREDS,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                },
                {
                    "index": "200",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": TGT_PUBKEY,
                        "withdrawal_credentials": VALID_CREDS_2,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&cl_server)
        .await;

    let manifest_yaml = format!(
        "consolidations:\n  - source_pubkey: \"{}\"\n    target_pubkey: \"{}\"\n",
        SRC_PUBKEY, TGT_PUBKEY
    );
    let pairs = parse_manifest_str(&manifest_yaml).unwrap();
    let beacon_client = BeaconClient::new(cl_server.uri());

    let report = SimulationEngine::run_simulation(&beacon_client, &pairs, None)
        .await
        .unwrap();

    let pair_res = &report.pairs[0];
    assert!(!pair_res.eligible);
    assert!(
        pair_res
            .rejection_reason
            .as_ref()
            .unwrap()
            .contains("0x00 BLS credentials")
    );
}

#[tokio::test]
async fn test_simulation_insufficient_source_balance() {
    let cl_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "100",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SRC_PUBKEY,
                        "withdrawal_credentials": VALID_CREDS_1,
                        "effective_balance": "31000000000", // 31 ETH < 32 ETH
                        "slashed": false
                    }
                },
                {
                    "index": "200",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": TGT_PUBKEY,
                        "withdrawal_credentials": VALID_CREDS_2,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&cl_server)
        .await;

    let manifest_yaml = format!(
        "consolidations:\n  - source_pubkey: \"{}\"\n    target_pubkey: \"{}\"\n",
        SRC_PUBKEY, TGT_PUBKEY
    );
    let pairs = parse_manifest_str(&manifest_yaml).unwrap();
    let beacon_client = BeaconClient::new(cl_server.uri());

    let report = SimulationEngine::run_simulation(&beacon_client, &pairs, None)
        .await
        .unwrap();

    let pair_res = &report.pairs[0];
    assert!(!pair_res.eligible);
    assert!(
        pair_res
            .rejection_reason
            .as_ref()
            .unwrap()
            .contains("below 32 ETH minimum activation threshold")
    );
}

#[tokio::test]
async fn test_simulation_pending_queue_warning() {
    let cl_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "100",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SRC_PUBKEY,
                        "withdrawal_credentials": VALID_CREDS_1,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                },
                {
                    "index": "200",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": TGT_PUBKEY,
                        "withdrawal_credentials": VALID_CREDS_2,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    // Return validator 100 in pending_consolidations
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "source_index": "100",
                    "target_index": "200"
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    let manifest_yaml = format!(
        "consolidations:\n  - source_pubkey: \"{}\"\n    target_pubkey: \"{}\"\n",
        SRC_PUBKEY, TGT_PUBKEY
    );
    let pairs = parse_manifest_str(&manifest_yaml).unwrap();
    let beacon_client = BeaconClient::new(cl_server.uri());

    let report = SimulationEngine::run_simulation(&beacon_client, &pairs, None)
        .await
        .unwrap();

    assert_eq!(report.summary.eligible_pairs, 1);
    assert_eq!(report.summary.warning_count, 1);

    let pair_res = &report.pairs[0];
    assert!(pair_res.eligible);
    assert!(pair_res.already_pending);
    assert!(!pair_res.warnings.is_empty());
    assert!(pair_res.warnings[0].contains("already queued in pending_consolidations"));
}
