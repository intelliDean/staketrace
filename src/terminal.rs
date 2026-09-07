use crate::cli::OutputFormat;
use crate::models::{ConsolidationStatus, VerificationReceipt};
use colored::*;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};

/// Prints the ASCII banner for staketrace.
pub fn print_banner() {
    println!(
        "{}",
        "=========================================================".cyan()
    );
    println!(
        "{}",
        "          STAKETRACE - Ethereum Validator Auditor        "
            .bold()
            .cyan()
    );
    println!(
        "{}",
        "          EIP-7251 (MaxEB) & EIP-7002 Verification       "
            .dimmed()
            .cyan()
    );
    println!(
        "{}",
        "=========================================================".cyan()
    );
}

/// Prints connection endpoints and manifest item counts.
pub fn print_connection_info(pair_count: usize, el_rpc: &str, cl_beacon_api: &str) {
    println!("   Found {} consolidation pairs in manifest.", pair_count);
    println!("⚡ Connecting to Execution Layer RPC: {}", el_rpc);
    println!("📡 Connecting to Consensus Beacon API: {}", cl_beacon_api);
    println!("🔍 Running cross-layer verification...");
}

/// Renders the full comfy-table results and summary status in the terminal.
pub fn print_verification_results(receipt: &VerificationReceipt) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);

    table.set_header(vec![
        Cell::new("#").fg(Color::Cyan),
        Cell::new("Source Validator").fg(Color::Cyan),
        Cell::new("Target Validator").fg(Color::Cyan),
        Cell::new("EL Tx").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
    ]);

    for (i, pair) in receipt.pairs.iter().enumerate() {
        let (status_cell, status_color) = match pair.status {
            ConsolidationStatus::Accepted => ("ACCEPTED", Color::Green),
            ConsolidationStatus::Queued => ("QUEUED", Color::Yellow),
            ConsolidationStatus::NotAccepted => ("NOT_ACCEPTED", Color::Red),
            ConsolidationStatus::Indeterminate => ("INDETERMINATE", Color::Magenta),
        };

        let src_text = format!(
            "{} ({})",
            pair.source_index
                .map(|idx| format!("#{}", idx))
                .unwrap_or_default(),
            &pair.source_pubkey[..pair.source_pubkey.len().min(10)]
        );
        let tgt_text = format!(
            "{} ({})",
            pair.target_index
                .map(|idx| format!("#{}", idx))
                .unwrap_or_default(),
            &pair.target_pubkey[..pair.target_pubkey.len().min(10)]
        );
        let tx_text = pair
            .el_tx_hash
            .as_deref()
            .map(|h| format!("{}...", &h[..h.len().min(10)]))
            .unwrap_or_else(|| "N/A".to_string());

        table.add_row(Row::from(vec![
            Cell::new(i + 1),
            Cell::new(src_text),
            Cell::new(tgt_text),
            Cell::new(tx_text),
            Cell::new(status_cell).fg(status_color),
        ]));
    }

    println!("\n{}", table);

    println!("\n{}", "--- Summary ---".bold());
    println!(
        "Total Pairs: {} | Accepted: {} | Queued: {} | Not Accepted: {} | Indeterminate: {}",
        receipt.summary.total_pairs,
        receipt.summary.accepted.to_string().green(),
        receipt.summary.queued.to_string().yellow(),
        receipt.summary.not_accepted.to_string().red(),
        receipt.summary.indeterminate.to_string().magenta(),
    );

    if receipt.fee_exemption.is_any_role_active() {
        println!(
            "\n{}",
            "⚠️  WARNING: vaults.NodeOperatorFee.FeeExemptRole is currently ACTIVE on one or more source validator accounts."
                .bold()
                .yellow()
        );
    }
}

/// Renders the simulation table results and dry-run summary in the terminal.
pub fn print_simulation_results(report: &crate::simulate::SimulationReport) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);

    table.set_header(vec![
        Cell::new("#").fg(Color::Cyan),
        Cell::new("Source Validator").fg(Color::Cyan),
        Cell::new("Target Validator").fg(Color::Cyan),
        Cell::new("Credentials").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
        Cell::new("Diagnosis").fg(Color::Cyan),
    ]);

    for (i, pair) in report.pairs.iter().enumerate() {
        let (status_cell, status_color) = if pair.eligible {
            if !pair.warnings.is_empty() {
                ("READY (WARN)", Color::Yellow)
            } else {
                ("ELIGIBLE", Color::Green)
            }
        } else {
            ("INELIGIBLE", Color::Red)
        };

        let src_text = format!(
            "{} ({})",
            pair.source_index
                .map(|idx| format!("#{}", idx))
                .unwrap_or_else(|| "unreg".to_string()),
            &pair.source_pubkey[..pair.source_pubkey.len().min(10)]
        );
        let tgt_text = format!(
            "{} ({})",
            pair.target_index
                .map(|idx| format!("#{}", idx))
                .unwrap_or_else(|| "unreg".to_string()),
            &pair.target_pubkey[..pair.target_pubkey.len().min(10)]
        );

        let creds_text = if pair.credentials_match {
            let prefix = pair
                .source_withdrawal_credentials
                .as_deref()
                .and_then(|c| c.get(..4))
                .unwrap_or("0x??");
            format!("MATCH ({})", prefix)
        } else {
            "MISMATCH".to_string()
        };

        let diagnosis = if let Some(reason) = &pair.rejection_reason {
            reason.clone()
        } else if !pair.warnings.is_empty() {
            pair.warnings.join("; ")
        } else {
            let src_eth = pair.source_effective_balance_gwei.unwrap_or(0) as f64 / 1e9;
            let tgt_eth = pair.target_effective_balance_gwei.unwrap_or(0) as f64 / 1e9;
            format!(
                "Ready (+{:.1} ETH -> {:.1} ETH)",
                src_eth,
                tgt_eth + src_eth
            )
        };

        table.add_row(Row::from(vec![
            Cell::new(i + 1),
            Cell::new(src_text),
            Cell::new(tgt_text),
            Cell::new(creds_text).fg(if pair.credentials_match {
                Color::Green
            } else {
                Color::Red
            }),
            Cell::new(status_cell).fg(status_color),
            Cell::new(diagnosis),
        ]));
    }

    println!("\n{}", table);

    println!("\n{}", "--- Simulation Summary ---".bold());
    println!(
        "Total Pairs: {} | Eligible: {} | Ineligible: {} | Warnings: {}",
        report.summary.total_pairs,
        report.summary.eligible_pairs.to_string().green(),
        report.summary.ineligible_pairs.to_string().red(),
        report.summary.warning_count.to_string().yellow(),
    );

    let src_total_eth = report.summary.total_source_balance_gwei as f64 / 1e9;
    let proj_total_eth = report.summary.projected_target_balance_gwei as f64 / 1e9;
    println!(
        "Balances: Source {:.2} ETH | Projected Target Total: {:.2} ETH",
        src_total_eth, proj_total_eth
    );
    println!(
        "Estimated Execution Gas: ~{} units (across {} requests)",
        report.summary.estimated_total_gas, report.summary.total_pairs
    );

    if report.summary.ineligible_pairs > 0 {
        println!(
            "\n{}",
            "❌ PRE-FLIGHT CHECK FAILED: Do NOT broadcast consolidation transactions. Fix the reported issues above."
                .bold()
                .red()
        );
    } else {
        println!(
            "\n{}",
            "✅ PRE-FLIGHT CHECK PASSED: All validator pairs are eligible for consolidation."
                .bold()
                .green()
        );
    }
}

/// Prints the requested output format payload to standard output.
pub fn print_requested_format(format: OutputFormat, markdown: &str, json_str: &str, csv_str: &str) {
    match format {
        OutputFormat::Markdown => println!("{}", markdown),
        OutputFormat::Json => println!("{}", json_str),
        OutputFormat::Csv => println!("{}", csv_str),
        OutputFormat::All => {}
    }
}
