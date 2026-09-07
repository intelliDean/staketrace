use super::models::ExitReceipt;
use crate::models::ConsolidationStatus;
use std::fmt::Write;

/// Generates a GitHub-flavored Markdown audit report for EIP-7002 validator exits.
pub fn generate_exit_markdown(receipt: &ExitReceipt) -> String {
    let mut md = String::with_capacity(4096);

    let _ = writeln!(
        md,
        "# Ethereum Validator Exit Verification Receipt (EIP-7002)\n"
    );
    let _ = writeln!(md, "**Generated At:** {}", receipt.timestamp.to_rfc3339());
    let _ = writeln!(md, "**CLI Tool Version:** v{}", receipt.tool_version);
    let _ = writeln!(md, "**Execution Layer RPC:** `{}`", receipt.el_rpc_url);
    let _ = writeln!(
        md,
        "**Consensus Beacon API:** `{}`\n",
        receipt.cl_beacon_url
    );

    // Status Banner
    if receipt.summary.is_all_accepted() {
        let _ = writeln!(md, "> [!NOTE]");
        let _ = writeln!(md, "> **ALL VALIDATOR EXITS PROVEN ACCEPTED & FINALIZED**");
        let _ = writeln!(
            md,
            "> Every exit request was verified in an Execution Layer transaction and included in a finalized Consensus Layer block.\n"
        );
    } else {
        let _ = writeln!(md, "> [!WARNING]");
        let _ = writeln!(
            md,
            "> **ATTENTION: {} OF {} EXITS ARE NOT FULLY FINALIZED**",
            receipt.summary.total_exits - receipt.summary.accepted,
            receipt.summary.total_exits
        );
        let _ = writeln!(
            md,
            "> Do not decommission validator hardware or key material until requests transition to finalized status.\n"
        );
    }

    // Summary Metrics
    let _ = writeln!(md, "## Exit Summary Metrics\n");
    let _ = writeln!(md, "| Metric | Count | Percentage |");
    let _ = writeln!(md, "| :--- | :--- | :--- |");
    let _ = writeln!(
        md,
        "| **Total Exits** | `{}` | `100.0%` |",
        receipt.summary.total_exits
    );
    let _ = writeln!(
        md,
        "| **Accepted** | `{}` | `{:.1}%` |",
        receipt.summary.accepted,
        pct(receipt.summary.accepted, receipt.summary.total_exits)
    );
    let _ = writeln!(
        md,
        "| **Queued / Pending** | `{}` | `{:.1}%` |",
        receipt.summary.queued,
        pct(receipt.summary.queued, receipt.summary.total_exits)
    );
    let _ = writeln!(
        md,
        "| **Not Accepted** | `{}` | `{:.1}%` |",
        receipt.summary.not_accepted,
        pct(receipt.summary.not_accepted, receipt.summary.total_exits)
    );
    let _ = writeln!(
        md,
        "| **Indeterminate** | `{}` | `{:.1}%` |\n",
        receipt.summary.indeterminate,
        pct(receipt.summary.indeterminate, receipt.summary.total_exits)
    );

    // Exits Table
    let _ = writeln!(md, "## Validator Exit Breakdown\n");
    let _ = writeln!(
        md,
        "| # | Validator | Type | EL Tx | Beacon Slot | Finalized | Status |"
    );
    let _ = writeln!(md, "| :-: | :--- | :--- | :--- | :--- | :-: | :--- |");

    for (i, exit) in receipt.exits.iter().enumerate() {
        let val_label = format!(
            "`{}` ({})",
            exit.validator_index
                .map(|idx| format!("#{}", idx))
                .unwrap_or_else(|| truncate_key(&exit.pubkey)),
            truncate_key(&exit.pubkey)
        );

        let type_label = if exit.is_full_exit {
            "Full Exit".to_string()
        } else {
            format!("Partial ({:.2} ETH)", exit.amount_gwei as f64 / 1e9)
        };

        let tx_label = exit
            .el_tx_hash
            .as_deref()
            .map(truncate_tx)
            .unwrap_or_else(|| "N/A".to_string());

        let slot_label = exit
            .beacon_slot
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Pending".to_string());

        let fin_label = if exit.finalized { "✅" } else { "⏳" };

        let status_badge = match exit.status {
            ConsolidationStatus::Accepted => "🟩 `ACCEPTED`",
            ConsolidationStatus::Queued => "🟨 `QUEUED`",
            ConsolidationStatus::NotAccepted => "🟥 `NOT_ACCEPTED`",
            ConsolidationStatus::Indeterminate => "🟪 `INDETERMINATE`",
        };

        let _ = writeln!(
            md,
            "| {} | {} | {} | `{}` | `{}` | {} | {} |",
            i + 1,
            val_label,
            type_label,
            tx_label,
            slot_label,
            fin_label,
            status_badge
        );
    }

    md
}

/// Generates a CSV table of all validator exit verification results.
pub fn generate_exit_csv(receipt: &ExitReceipt) -> Result<String, csv::Error> {
    let mut wtr = csv::WriterBuilder::new().from_writer(vec![]);

    wtr.write_record([
        "index",
        "validator_index",
        "validator_pubkey",
        "type",
        "amount_gwei",
        "derived_source_address",
        "el_tx_hash",
        "beacon_slot",
        "finalized",
        "status",
        "rejection_reason",
    ])?;

    for (i, exit) in receipt.exits.iter().enumerate() {
        let idx_str = (i + 1).to_string();
        let val_idx_str = exit
            .validator_index
            .map(|idx| idx.to_string())
            .unwrap_or_default();
        let type_str = if exit.is_full_exit {
            "full_exit"
        } else {
            "partial_withdrawal"
        };
        let amt_str = exit.amount_gwei.to_string();
        let derived_addr = exit.derived_source_address.as_deref().unwrap_or_default();
        let tx_hash = exit.el_tx_hash.as_deref().unwrap_or_default();
        let slot_str = exit.beacon_slot.map(|s| s.to_string()).unwrap_or_default();
        let fin_str = if exit.finalized { "true" } else { "false" };
        let status_str = match exit.status {
            ConsolidationStatus::Accepted => "ACCEPTED",
            ConsolidationStatus::Queued => "QUEUED",
            ConsolidationStatus::NotAccepted => "NOT_ACCEPTED",
            ConsolidationStatus::Indeterminate => "INDETERMINATE",
        };
        let reason_str = exit.rejection_reason.as_deref().unwrap_or_default();

        wtr.write_record([
            &idx_str,
            &val_idx_str,
            &exit.pubkey,
            type_str,
            &amt_str,
            derived_addr,
            tx_hash,
            &slot_str,
            fin_str,
            status_str,
            reason_str,
        ])?;
    }

    let bytes = wtr.into_inner().map_err(|e| e.into_error())?;
    String::from_utf8(bytes)
        .map_err(|e| csv::Error::from(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
}

fn pct(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (part as f64 / total as f64) * 100.0
    }
}

fn truncate_key(key: &str) -> String {
    let clean = key.trim_start_matches("0x");
    if clean.len() >= 10 {
        format!("0x{}...", &clean[..8])
    } else {
        key.to_string()
    }
}

fn truncate_tx(tx: &str) -> String {
    if tx.len() >= 12 {
        format!("{}...", &tx[..10])
    } else {
        tx.to_string()
    }
}
