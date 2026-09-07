//! Terminal ASCII branding and connection info banners.

use colored::*;

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
