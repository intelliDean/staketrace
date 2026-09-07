use serde_json::json;
use staketrace::models::ConsolidationStatus;
use staketrace::{
    BeaconClient, ElClient, ExitEngine, generate_exit_csv, generate_exit_markdown,
    parse_exit_manifest_str,
};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SAMPLE_PUBKEY_1: &str = "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
const SAMPLE_PUBKEY_2: &str = "0x96b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c488";
const WITHDRAWAL_CREDS: &str = "0x01000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8";

fn encode_exit_calldata(pubkey: &str, amount: u64) -> String {
    let p = pubkey.trim().trim_start_matches("0x");
    let amt = format!("{:016x}", amount);
    format!("0x{}{}", p, amt)
}

async fn setup_mock_servers() -> (MockServer, MockServer) {
    let el_server = MockServer::start().await;
    let cl_server = MockServer::start().await;

    // Genesis
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/genesis"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "genesis_time": "1606824023",
                "genesis_validators_root": "0x00",
                "genesis_fork_version": "0x00"
            }
        })))
        .mount(&cl_server)
        .await;

    // Finality Checkpoints (epoch 10 finalized)
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/finality_checkpoints"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "previous_justified": { "epoch": "11", "root": "0x11" },
                "current_justified": { "epoch": "12", "root": "0x12" },
                "finalized": { "epoch": "10", "root": "0x10" }
            }
        })))
        .mount(&cl_server)
        .await;

    (el_server, cl_server)
}

#[tokio::test]
async fn test_exit_accepted_finalized() {
    let (el_server, cl_server) = setup_mock_servers().await;

    let tx_hash = "0xaa11223344556677889900aabbccddeeff0011223344556677889900aabbccdd";
    let calldata = encode_exit_calldata(SAMPLE_PUBKEY_1, 0);

    // Mock EL receipt
    Mock::given(method("POST"))
        .and(body_string_contains("eth_getTransactionReceipt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "transactionHash": tx_hash,
                "blockNumber": "0x64", // 100
                "status": "0x1",
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007002",
                "logs": []
            }
        })))
        .mount(&el_server)
        .await;

    // Mock EL transaction
    Mock::given(method("POST"))
        .and(body_string_contains("eth_getTransactionByHash"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "hash": tx_hash,
                "input": calldata,
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007002"
            }
        })))
        .mount(&el_server)
        .await;

    // Mock EL block (slot 100 timestamp: 1606824023 + 100*12 = 1606825223 -> 0x5fc68507)
    Mock::given(method("POST"))
        .and(body_string_contains("eth_getBlockByNumber"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "number": "0x64",
                "timestamp": "0x5fc63507"
            }
        })))
        .mount(&el_server)
        .await;

    // Mock CL validator info
    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "100",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SAMPLE_PUBKEY_1,
                        "withdrawal_credentials": WITHDRAWAL_CREDS,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    // Mock Beacon block at slot 100 (epoch 3 < 10 finalized) with withdrawal request
    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "1",
                    "parent_root": "0xparent",
                    "state_root": "0xpoststate",
                    "body": {
                        "execution_requests": {
                            "withdrawals": [
                                {
                                    "source_address": "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
                                    "validator_pubkey": SAMPLE_PUBKEY_1,
                                    "amount": "0"
                                }
                            ]
                        }
                    }
                },
                "signature": "0x00"
            }
        })))
        .mount(&cl_server)
        .await;

    let manifest_yaml = format!("- pubkey: \"{}\"\n  amount: 0\n", SAMPLE_PUBKEY_1);
    let requests = parse_exit_manifest_str(&manifest_yaml).unwrap();

    let el_client = ElClient::new(el_server.uri());
    let beacon_client = BeaconClient::new(cl_server.uri());

    let receipt = ExitEngine::run_verification(
        &requests,
        &[tx_hash.to_string()],
        &el_client,
        &beacon_client,
    )
    .await
    .unwrap();

    assert_eq!(receipt.summary.total_exits, 1);
    assert_eq!(receipt.summary.accepted, 1);
    assert!(receipt.summary.is_all_accepted());

    let res = &receipt.exits[0];
    assert_eq!(res.status, ConsolidationStatus::Accepted);
    assert!(res.is_full_exit);
    assert_eq!(res.beacon_slot, Some(100));
    assert!(res.finalized);

    let md = generate_exit_markdown(&receipt);
    assert!(md.contains("ALL VALIDATOR EXITS PROVEN ACCEPTED & FINALIZED"));

    let csv = generate_exit_csv(&receipt).unwrap();
    assert!(csv.contains("ACCEPTED"));
    assert!(csv.contains("full_exit"));
}

#[tokio::test]
async fn test_exit_partial_withdrawal_accepted() {
    let (el_server, cl_server) = setup_mock_servers().await;

    let tx_hash = "0xbb11223344556677889900aabbccddeeff0011223344556677889900aabbccdd";
    let partial_amount: u64 = 16_000_000_000; // 16 ETH
    let calldata = encode_exit_calldata(SAMPLE_PUBKEY_2, partial_amount);

    Mock::given(method("POST"))
        .and(body_string_contains("eth_getTransactionReceipt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "transactionHash": tx_hash,
                "blockNumber": "0x64",
                "status": "0x1",
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007002",
                "logs": []
            }
        })))
        .mount(&el_server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains("eth_getTransactionByHash"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "hash": tx_hash,
                "input": calldata,
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007002"
            }
        })))
        .mount(&el_server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains("eth_getBlockByNumber"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "number": "0x64",
                "timestamp": "0x5fc63507"
            }
        })))
        .mount(&el_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "200",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SAMPLE_PUBKEY_2,
                        "withdrawal_credentials": WITHDRAWAL_CREDS,
                        "effective_balance": "64000000000",
                        "slashed": false
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "1",
                    "parent_root": "0xparent",
                    "state_root": "0xpoststate",
                    "body": {
                        "execution_requests": {
                            "withdrawals": [
                                {
                                    "source_address": "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
                                    "validator_pubkey": SAMPLE_PUBKEY_2,
                                    "amount": "16000000000"
                                }
                            ]
                        }
                    }
                },
                "signature": "0x00"
            }
        })))
        .mount(&cl_server)
        .await;

    let manifest_json = format!(
        "[{{\"pubkey\": \"{}\", \"amount\": {}}}]",
        SAMPLE_PUBKEY_2, partial_amount
    );
    let requests = parse_exit_manifest_str(&manifest_json).unwrap();

    let el_client = ElClient::new(el_server.uri());
    let beacon_client = BeaconClient::new(cl_server.uri());

    let receipt = ExitEngine::run_verification(
        &requests,
        &[tx_hash.to_string()],
        &el_client,
        &beacon_client,
    )
    .await
    .unwrap();

    assert_eq!(receipt.summary.accepted, 1);
    let res = &receipt.exits[0];
    assert!(!res.is_full_exit);
    assert_eq!(res.amount_gwei, partial_amount);
}

#[tokio::test]
async fn test_exit_queued_unfinalized() {
    let el_server = MockServer::start().await;
    let cl_server = MockServer::start().await;

    // Genesis
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/genesis"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "genesis_time": "1606824023",
                "genesis_validators_root": "0x00",
                "genesis_fork_version": "0x00"
            }
        })))
        .mount(&cl_server)
        .await;

    // Finality checkpoints at epoch 2 (slot 100 epoch 3 is NOT finalized)
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/finality_checkpoints"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "previous_justified": { "epoch": "2", "root": "0x02" },
                "current_justified": { "epoch": "3", "root": "0x03" },
                "finalized": { "epoch": "2", "root": "0x02" }
            }
        })))
        .mount(&cl_server)
        .await;

    let tx_hash = "0xdd11223344556677889900aabbccddeeff0011223344556677889900aabbccdd";
    let calldata = encode_exit_calldata(SAMPLE_PUBKEY_1, 0);

    Mock::given(method("POST"))
        .and(body_string_contains("eth_getTransactionReceipt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "transactionHash": tx_hash,
                "blockNumber": "0x64",
                "status": "0x1",
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007002",
                "logs": []
            }
        })))
        .mount(&el_server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains("eth_getTransactionByHash"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "hash": tx_hash,
                "input": calldata,
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007002"
            }
        })))
        .mount(&el_server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains("eth_getBlockByNumber"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "number": "0x64",
                "timestamp": "0x5fc63507"
            }
        })))
        .mount(&el_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "100",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SAMPLE_PUBKEY_1,
                        "withdrawal_credentials": WITHDRAWAL_CREDS,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "1",
                    "parent_root": "0xparent",
                    "state_root": "0xpoststate",
                    "body": {
                        "execution_requests": {
                            "withdrawals": [
                                {
                                    "source_address": "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
                                    "validator_pubkey": SAMPLE_PUBKEY_1,
                                    "amount": "0"
                                }
                            ]
                        }
                    }
                },
                "signature": "0x00"
            }
        })))
        .mount(&cl_server)
        .await;

    let requests = parse_exit_manifest_str(&format!("[\"{}\"]", SAMPLE_PUBKEY_1)).unwrap();
    let el_client = ElClient::new(el_server.uri());
    let beacon_client = BeaconClient::new(cl_server.uri());

    let receipt = ExitEngine::run_verification(
        &requests,
        &[tx_hash.to_string()],
        &el_client,
        &beacon_client,
    )
    .await
    .unwrap();

    assert_eq!(receipt.summary.queued, 1);
    assert_eq!(receipt.summary.accepted, 0);
    assert_eq!(receipt.exits[0].status, ConsolidationStatus::Queued);
    assert!(!receipt.exits[0].finalized);
}

#[test]
fn test_exit_manifest_parsing_formats() {
    // Format 1: List of string pubkeys
    let json_strings = format!("[\"{}\", \"{}\"]", SAMPLE_PUBKEY_1, SAMPLE_PUBKEY_2);
    let reqs1 = parse_exit_manifest_str(&json_strings).unwrap();
    assert_eq!(reqs1.len(), 2);
    assert_eq!(reqs1[0].pubkey, SAMPLE_PUBKEY_1);
    assert_eq!(reqs1[0].amount_gwei, 0);

    // Format 2: List of objects with amount
    let json_objs = format!(
        "[{{\"pubkey\": \"{}\", \"amount_gwei\": 16000000000}}]",
        SAMPLE_PUBKEY_1
    );
    let reqs2 = parse_exit_manifest_str(&json_objs).unwrap();
    assert_eq!(reqs2.len(), 1);
    assert_eq!(reqs2[0].amount_gwei, 16_000_000_000);

    // Format 3: YAML with exits key
    let yaml = format!(
        "exits:\n  - pubkey: \"{}\"\n    amount: 32000000000\n",
        SAMPLE_PUBKEY_2
    );
    let reqs3 = parse_exit_manifest_str(&yaml).unwrap();
    assert_eq!(reqs3.len(), 1);
    assert_eq!(reqs3[0].pubkey, SAMPLE_PUBKEY_2);
    assert_eq!(reqs3[0].amount_gwei, 32_000_000_000);
}
