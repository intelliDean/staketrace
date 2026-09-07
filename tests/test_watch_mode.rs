use serde_json::json;
use staketrace::models::{ConsolidationPair, ConsolidationStatus};
use staketrace::{BeaconClient, ElClient, ExitRequest, Watcher};
use std::time::Duration;
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

#[tokio::test]
async fn test_watch_exits_completed_with_webhook() {
    let el_server = MockServer::start().await;
    let cl_server = MockServer::start().await;
    let webhook_server = MockServer::start().await;

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

    let tx_hash = "0xaa11223344556677889900aabbccddeeff0011223344556677889900aabbccdd";
    let calldata = encode_exit_calldata(SAMPLE_PUBKEY_1, 0);

    // Mock EL receipt
    Mock::given(method("POST"))
        .and(body_string_contains("eth_getTransactionReceipt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "status": "0x1",
                "blockNumber": "0x64",
                "transactionHash": tx_hash,
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
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007002",
                "blockNumber": "0x64"
            }
        })))
        .mount(&el_server)
        .await;

    // Mock EL block header (slot 100 timestamp: 1606824023 + 100*12 = 1606825223 -> 0x5fc63507)
    Mock::given(method("POST"))
        .and(body_string_contains("eth_getBlockByNumber"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "number": "0x64",
                "hash": "0xblockhash",
                "parentHash": "0xparenthash",
                "timestamp": "0x5fc63507",
                "transactions": [tx_hash]
            }
        })))
        .mount(&el_server)
        .await;

    // Mock CL validator index query
    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "12345",
                    "balance": "32000000000",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SAMPLE_PUBKEY_1,
                        "withdrawal_credentials": WITHDRAWAL_CREDS,
                        "effective_balance": "32000000000",
                        "slashed": false,
                        "activation_eligibility_epoch": "0",
                        "activation_epoch": "0",
                        "exit_epoch": "18446744073709551615",
                        "withdrawable_epoch": "18446744073709551615"
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    // Mock Beacon block at slot 100
    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "1",
                    "parent_root": "0xparentblockroot",
                    "state_root": "0xblock100stateroot",
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

    // Mock Webhook Receiver
    Mock::given(method("POST"))
        .and(path("/webhook"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "received"})))
        .mount(&webhook_server)
        .await;

    let el_client = ElClient::new(el_server.uri());
    let beacon_client = BeaconClient::new(cl_server.uri());

    let requests = vec![ExitRequest::new(SAMPLE_PUBKEY_1, 0)];
    let txs = vec![tx_hash.to_string()];
    let webhook_url = format!("{}/webhook", webhook_server.uri());

    let receipt = Watcher::watch_exits(
        &requests,
        &txs,
        &el_client,
        &beacon_client,
        Duration::from_millis(50),
        Duration::from_secs(5),
        Some(&webhook_url),
        true,
    )
    .await
    .unwrap();

    assert_eq!(receipt.summary.total_exits, 1);
    assert_eq!(receipt.summary.accepted, 1);
    assert_eq!(receipt.exits[0].status, ConsolidationStatus::Accepted);
}

#[tokio::test]
async fn test_watch_consolidations_timeout_handling() {
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

    // Finality Checkpoints (epoch 0 finalized)
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/finality_checkpoints"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "previous_justified": { "epoch": "1", "root": "0x11" },
                "current_justified": { "epoch": "2", "root": "0x12" },
                "finalized": { "epoch": "0", "root": "0x10" }
            }
        })))
        .mount(&cl_server)
        .await;

    let tx_hash = "0xbb11223344556677889900aabbccddeeff0011223344556677889900aabbccdd";
    let calldata = format!(
        "0x{}{}",
        SAMPLE_PUBKEY_1.trim_start_matches("0x"),
        SAMPLE_PUBKEY_2.trim_start_matches("0x")
    );

    // Mock EL receipt
    Mock::given(method("POST"))
        .and(body_string_contains("eth_getTransactionReceipt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "status": "0x1",
                "blockNumber": "0x64",
                "transactionHash": tx_hash,
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007251",
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
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007251",
                "blockNumber": "0x64"
            }
        })))
        .mount(&el_server)
        .await;

    // Mock EL block header (slot 100 timestamp)
    Mock::given(method("POST"))
        .and(body_string_contains("eth_getBlockByNumber"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "number": "0x64",
                "hash": "0xblockhash",
                "parentHash": "0xparenthash",
                "timestamp": "0x5fc63507",
                "transactions": [tx_hash]
            }
        })))
        .mount(&el_server)
        .await;

    // Mock CL validator index queries
    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "101",
                    "balance": "32000000000",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SAMPLE_PUBKEY_1,
                        "withdrawal_credentials": WITHDRAWAL_CREDS,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                },
                {
                    "index": "102",
                    "balance": "32000000000",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SAMPLE_PUBKEY_2,
                        "withdrawal_credentials": WITHDRAWAL_CREDS,
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    // Mock Beacon block at slot 100
    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "42",
                    "parent_root": "0xparent_root_100",
                    "state_root": "0xstate_root_100",
                    "body": {
                        "execution_requests": {
                            "consolidations": [
                                { "source_pubkey": SAMPLE_PUBKEY_1, "target_pubkey": SAMPLE_PUBKEY_2, "source_index": "101", "target_index": "102" }
                            ]
                        }
                    }
                },
                "signature": "0x1234"
            }
        })))
        .mount(&cl_server)
        .await;

    // Parent state: absent
    Mock::given(method("GET"))
        .and(path(
            "/eth/v1/beacon/states/0xparent_root_100/pending_consolidations",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&cl_server)
        .await;

    // Post state: present
    Mock::given(method("GET"))
        .and(path(
            "/eth/v1/beacon/states/0xstate_root_100/pending_consolidations",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({ "data": [{ "source_index": "101", "target_index": "102" }] }),
            ),
        )
        .mount(&cl_server)
        .await;

    let el_client = ElClient::new(el_server.uri());
    let beacon_client = BeaconClient::new(cl_server.uri());

    let pairs = vec![ConsolidationPair::new(SAMPLE_PUBKEY_1, SAMPLE_PUBKEY_2)];
    let txs = vec![tx_hash.to_string()];

    // Watch mode with 100ms timeout -> should timeout and return QUEUED status
    let receipt = Watcher::watch_consolidations(
        &pairs,
        &txs,
        &el_client,
        &beacon_client,
        None,
        Duration::from_millis(20),
        Duration::from_millis(100),
        None,
        true,
    )
    .await
    .unwrap();

    assert_eq!(receipt.summary.total_pairs, 1);
    assert_eq!(receipt.summary.queued, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::Queued);
}
