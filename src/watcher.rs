use crate::cl::BeaconClient;
use crate::el::ElClient;
use crate::engine::VerificationEngine;
use crate::error::Result;
use crate::exit::ExitEngine;
use crate::exit::models::{ExitReceipt, ExitRequest};
use crate::models::{ConsolidationPair, VerificationReceipt};
use crate::webhook::{WebhookPayload, send_webhook};
use std::time::{Duration, Instant};

/// Watcher runner for tracking in-flight consolidations and exits until consensus finality.
pub struct Watcher;

impl Watcher {
    /// Watches a batch of EIP-7251 consolidation requests until all requests reach a terminal status or timeout.
    #[allow(clippy::too_many_arguments)]
    pub async fn watch_consolidations(
        pairs: &[ConsolidationPair],
        el_txs: &[String],
        el_client: &ElClient,
        beacon_client: &BeaconClient,
        st_vault_dashboard: Option<&str>,
        poll_interval: Duration,
        watch_timeout: Duration,
        webhook_url: Option<&str>,
        quiet: bool,
    ) -> Result<VerificationReceipt> {
        let start_time = Instant::now();
        let mut poll_count = 0;

        if !quiet {
            println!(
                "\n⏱️  Starting live watch mode (Poll interval: {:?}, Max timeout: {:?})...\n",
                poll_interval, watch_timeout
            );
        }

        loop {
            poll_count += 1;
            let elapsed = start_time.elapsed();

            // Run verification pass
            let receipt = VerificationEngine::run_verification(
                pairs,
                el_txs,
                el_client,
                beacon_client,
                st_vault_dashboard,
            )
            .await?;

            let summary = &receipt.summary;
            let is_terminal = summary.queued == 0;

            if !quiet {
                println!(
                    "  [Poll #{:<2} | Elapsed: {:>3}s] 📊 Total: {} | ✅ Accepted: {} | ⏳ Queued: {} | ❌ Not Accepted: {} | ⚠️ Indeterminate: {}",
                    poll_count,
                    elapsed.as_secs(),
                    summary.total_pairs,
                    summary.accepted,
                    summary.queued,
                    summary.not_accepted,
                    summary.indeterminate
                );
            }

            if is_terminal {
                if !quiet {
                    println!(
                        "\n🎉 All consolidation requests reached terminal state in {}s!",
                        elapsed.as_secs()
                    );
                }

                if let Some(url) = webhook_url {
                    let payload = WebhookPayload::for_consolidation(
                        &receipt,
                        "consolidation_completed",
                        elapsed.as_secs(),
                    );
                    let _ = send_webhook(url, &payload).await;
                }

                return Ok(receipt);
            }

            if elapsed >= watch_timeout {
                if !quiet {
                    println!(
                        "\n⏰ Watch timeout reached ({}s). {} request(s) remain QUEUED.\n",
                        watch_timeout.as_secs(),
                        summary.queued
                    );
                }

                if let Some(url) = webhook_url {
                    let payload = WebhookPayload::for_consolidation(
                        &receipt,
                        "watch_timeout",
                        elapsed.as_secs(),
                    );
                    let _ = send_webhook(url, &payload).await;
                }

                return Ok(receipt);
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Watches a batch of EIP-7002 validator exit requests until all requests reach a terminal status or timeout.
    #[allow(clippy::too_many_arguments)]
    pub async fn watch_exits(
        requests: &[ExitRequest],
        el_txs: &[String],
        el_client: &ElClient,
        beacon_client: &BeaconClient,
        poll_interval: Duration,
        watch_timeout: Duration,
        webhook_url: Option<&str>,
        quiet: bool,
    ) -> Result<ExitReceipt> {
        let start_time = Instant::now();
        let mut poll_count = 0;

        if !quiet {
            println!(
                "\n⏱️  Starting live exit watch mode (Poll interval: {:?}, Max timeout: {:?})...\n",
                poll_interval, watch_timeout
            );
        }

        loop {
            poll_count += 1;
            let elapsed = start_time.elapsed();

            // Run exit verification pass
            let receipt =
                ExitEngine::run_verification(requests, el_txs, el_client, beacon_client).await?;

            let summary = &receipt.summary;
            let is_terminal = summary.queued == 0;

            if !quiet {
                println!(
                    "  [Poll #{:<2} | Elapsed: {:>3}s] 📊 Total: {} | ✅ Accepted: {} | ⏳ Queued: {} | ❌ Not Accepted: {} | ⚠️ Indeterminate: {}",
                    poll_count,
                    elapsed.as_secs(),
                    summary.total_exits,
                    summary.accepted,
                    summary.queued,
                    summary.not_accepted,
                    summary.indeterminate
                );
            }

            if is_terminal {
                if !quiet {
                    println!(
                        "\n🎉 All validator exit requests reached terminal state in {}s!",
                        elapsed.as_secs()
                    );
                }

                if let Some(url) = webhook_url {
                    let payload =
                        WebhookPayload::for_exit(&receipt, "exit_completed", elapsed.as_secs());
                    let _ = send_webhook(url, &payload).await;
                }

                return Ok(receipt);
            }

            if elapsed >= watch_timeout {
                if !quiet {
                    println!(
                        "\n⏰ Watch timeout reached ({}s). {} exit request(s) remain QUEUED.\n",
                        watch_timeout.as_secs(),
                        summary.queued
                    );
                }

                if let Some(url) = webhook_url {
                    let payload =
                        WebhookPayload::for_exit(&receipt, "watch_timeout", elapsed.as_secs());
                    let _ = send_webhook(url, &payload).await;
                }

                return Ok(receipt);
            }

            tokio::time::sleep(poll_interval).await;
        }
    }
}
