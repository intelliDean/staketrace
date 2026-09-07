use clap::Parser;
use colored::*;
use staketrace::cli::{CliArgs, Commands, SimulateArgs, VerifyArgs};
use staketrace::terminal;
use staketrace::{
    AppError, BeaconClient, ElClient, SimulationEngine, VerificationEngine,
    generate_and_save_receipts, generate_simulation_markdown, parse_manifest_file,
};
use std::fs;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let args = CliArgs::parse();

    // Handle shell completion generation if requested
    if let Some(shell) = args.generate_completions {
        CliArgs::print_completions(shell);
        return ExitCode::SUCCESS;
    }

    let result = match args.command {
        Some(Commands::Simulate(sim_args)) => run_simulation(sim_args).await,
        Some(Commands::Verify(verify_args)) => run_verification(verify_args).await,
        None => run_verification(args.verify).await,
    };

    match result {
        Ok(success) => {
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("{} {}", "ERROR:".bold().red(), e);
            ExitCode::from(e.exit_code())
        }
    }
}

/// Executes the pre-flight simulation engine.
/// Returns `Ok(true)` if all pairs are eligible for consolidation, or `Ok(false)` if any are ineligible.
async fn run_simulation(args: SimulateArgs) -> Result<bool, AppError> {
    if !args.quiet {
        terminal::print_banner();
        println!("🔬 Mode: Pre-flight Consolidation Simulation (Dry-Run)");
        println!("📂 Parsing manifest: {}", args.manifest.display());
    }

    // Step 1: Parse Manifest
    let pairs = parse_manifest_file(&args.manifest)?;

    if !args.quiet {
        println!("   Found {} consolidation pairs in manifest.", pairs.len());
        println!(
            "📡 Connecting to Consensus Beacon API: {}",
            args.cl_beacon_api
        );
        if let Some(ref el) = args.el_rpc {
            println!("⚡ Connecting to Execution Layer RPC: {}", el);
        }
        println!("🧪 Simulating EIP-7251 rules, balance caps & credentials...");
    }

    // Step 2: Initialize Consensus Client
    let timeout = std::time::Duration::from_secs(args.timeout);
    let beacon_client = BeaconClient::with_timeout(&args.cl_beacon_api, timeout);

    // Step 3: Run Simulation
    let report =
        SimulationEngine::run_simulation(&beacon_client, &pairs, args.el_rpc.as_deref()).await?;

    // Step 4: Generate and Save Simulation Artifacts
    fs::create_dir_all(&args.output_dir)?;
    let md_content = generate_simulation_markdown(&report);
    let json_content = serde_json::to_string_pretty(&report)?;

    let md_path = args.output_dir.join("simulation_summary.md");
    let json_path = args.output_dir.join("simulation.json");

    fs::write(&md_path, &md_content)?;
    fs::write(&json_path, &json_content)?;

    if !args.quiet {
        println!(
            "💾 Simulation report saved to directory: {}",
            args.output_dir.display()
        );
        terminal::print_simulation_results(&report);
    }

    // Print raw output format if requested
    terminal::print_requested_format(args.format, &md_content, &json_content, "");

    Ok(report.summary.is_all_eligible())
}

/// Executes the cross-layer verification pipeline.
/// Returns `Ok(true)` if all pairs are accepted, or `Ok(false)` if any require attention.
async fn run_verification(args: VerifyArgs) -> Result<bool, AppError> {
    let manifest_path = args.manifest.as_ref().ok_or_else(|| {
        AppError::Manifest("Missing required argument: --manifest <PATH>".to_string())
    })?;

    if !args.quiet {
        terminal::print_banner();
        println!("📂 Parsing manifest: {}", manifest_path.display());
    }

    // Step 1: Parse Manifest
    let pairs = parse_manifest_file(manifest_path)?;

    if !args.quiet {
        terminal::print_connection_info(pairs.len(), &args.el_rpc, &args.cl_beacon_api);
    }

    // Step 2: Initialize RPC Clients
    let timeout = std::time::Duration::from_secs(args.timeout);
    let el_client = ElClient::with_timeout(&args.el_rpc, timeout);
    let beacon_client = BeaconClient::with_timeout(&args.cl_beacon_api, timeout);

    // Step 3: Execute Cross-Layer Verification
    let normalized_txs = args.normalized_el_txs();
    let receipt = VerificationEngine::run_verification(
        &pairs,
        &normalized_txs,
        &el_client,
        &beacon_client,
        args.st_vault_dashboard.as_deref(),
    )
    .await?;

    // Step 4: Generate and Save Artifacts (Markdown, JSON, CSV, Evidence)
    let artifacts = generate_and_save_receipts(&args.output_dir, &receipt)?;

    if !args.quiet {
        println!(
            "💾 Receipts saved to directory: {}",
            args.output_dir.display()
        );
        terminal::print_verification_results(&receipt);
    }

    // Print raw output format if requested (Markdown, JSON, or CSV)
    terminal::print_requested_format(
        args.format,
        &artifacts.markdown,
        &artifacts.json,
        &artifacts.csv,
    );

    Ok(receipt.summary.is_all_accepted())
}
