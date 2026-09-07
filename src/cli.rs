use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    All,
    Markdown,
    Json,
    Csv,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => write!(f, "all"),
            Self::Markdown => write!(f, "markdown"),
            Self::Json => write!(f, "json"),
            Self::Csv => write!(f, "csv"),
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "staketrace",
    author = "intelliDean <o.michaeldean@gmail.com>, Staketrace Contributors",
    version,
    about = "Traces, simulates, and verifies Ethereum validator consolidations (EIP-7251 MaxEB) across Execution & Consensus layers.",
    long_about = "A high-assurance CLI tool that simulates and verifies Ethereum validator consolidation requests from Execution Layer predeploy transactions to exact Consensus Layer state delta proofs."
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub verify: VerifyArgs,

    /// Generate shell autocompletions (bash, zsh, fish, powershell, elvish)
    #[arg(long, value_name = "SHELL")]
    pub generate_completions: Option<Shell>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Cross-layer verification of submitted consolidation transactions (EIP-7251)
    Verify(VerifyArgs),
    /// Pre-flight safety simulation of consolidation batches before on-chain submission
    Simulate(SimulateArgs),
    /// Cross-layer verification of Execution-Layer-triggered validator exits (EIP-7002)
    Exit(ExitArgs),
}

#[derive(Parser, Debug, Clone, Default)]
pub struct VerifyArgs {
    /// Path to the validator consolidation manifest file (JSON or YAML)
    #[arg(short, long, value_name = "PATH")]
    pub manifest: Option<PathBuf>,

    /// Execution layer transaction hash(es) separated by comma or specified multiple times
    #[arg(
        short = 't',
        long = "el-tx",
        value_name = "TX_HASH",
        value_delimiter = ','
    )]
    pub el_txs: Vec<String>,

    /// Ethereum Execution Layer JSON-RPC URL (e.g. http://127.0.0.1:8545)
    #[arg(
        long,
        env = "EL_RPC_URL",
        value_name = "URL",
        default_value = "http://127.0.0.1:8545"
    )]
    pub el_rpc: String,

    /// Ethereum Consensus Layer Beacon API URL (e.g. http://127.0.0.1:5052)
    #[arg(
        long,
        env = "CL_BEACON_API_URL",
        value_name = "URL",
        default_value = "http://127.0.0.1:5052"
    )]
    pub cl_beacon_api: String,

    /// Optional Lido stVault Dashboard or AccessControl contract address (for fee-exemption role audit)
    #[arg(long, env = "ST_VAULT_DASHBOARD", value_name = "ADDRESS")]
    pub st_vault_dashboard: Option<String>,

    /// Output directory where receipts and evidence artifacts will be saved
    #[arg(short, long, default_value = "./staketrace_output", value_name = "DIR")]
    pub output_dir: PathBuf,

    /// Output format to print to stdout (all, markdown, json, csv)
    #[arg(long, value_enum, default_value = "all")]
    pub format: OutputFormat,

    /// HTTP request timeout in seconds (for EL RPC and CL Beacon queries)
    #[arg(long, default_value_t = 30, value_name = "SECONDS")]
    pub timeout: u64,

    /// Enable live watch mode, polling until all requests are finalized or timeout
    #[arg(long)]
    pub watch: bool,

    /// Polling interval in seconds when in watch mode (default: 12 seconds)
    #[arg(long, default_value_t = 12, value_name = "SECONDS")]
    pub poll_interval: u64,

    /// Maximum duration in seconds to watch before timing out (default: 1800s / 30m)
    #[arg(long, default_value_t = 1800, value_name = "SECONDS")]
    pub watch_timeout: u64,

    /// Optional webhook URL (HTTP POST) to dispatch JSON notification upon completion
    #[arg(long, env = "STAKETRACE_WEBHOOK_URL", value_name = "URL")]
    pub webhook_url: Option<String>,

    /// Suppress informative logging
    #[arg(short, long)]
    pub quiet: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct SimulateArgs {
    /// Path to the validator consolidation manifest file (JSON or YAML)
    #[arg(short, long, value_name = "PATH")]
    pub manifest: PathBuf,

    /// Ethereum Consensus Layer Beacon API URL (e.g. http://127.0.0.1:5052)
    #[arg(
        long,
        env = "CL_BEACON_API_URL",
        value_name = "URL",
        default_value = "http://127.0.0.1:5052"
    )]
    pub cl_beacon_api: String,

    /// Optional Ethereum Execution Layer JSON-RPC URL
    #[arg(long, env = "EL_RPC_URL", value_name = "URL")]
    pub el_rpc: Option<String>,

    /// Output directory where simulation report and JSON will be saved
    #[arg(
        short,
        long,
        default_value = "./staketrace_simulation",
        value_name = "DIR"
    )]
    pub output_dir: PathBuf,

    /// Output format to print to stdout (all, markdown, json, csv)
    #[arg(long, value_enum, default_value = "all")]
    pub format: OutputFormat,

    /// HTTP request timeout in seconds
    #[arg(long, default_value_t = 30, value_name = "SECONDS")]
    pub timeout: u64,

    /// Suppress informative logging
    #[arg(short, long)]
    pub quiet: bool,
}

#[derive(Parser, Debug, Clone, Default)]
pub struct ExitArgs {
    /// Path to the validator exit manifest file (JSON or YAML list of pubkeys / requests)
    #[arg(short, long, value_name = "PATH")]
    pub manifest: PathBuf,

    /// Execution layer transaction hash(es) separated by comma or specified multiple times
    #[arg(
        short = 't',
        long = "el-tx",
        value_name = "TX_HASH",
        value_delimiter = ','
    )]
    pub el_txs: Vec<String>,

    /// Ethereum Execution Layer JSON-RPC URL (e.g. http://127.0.0.1:8545)
    #[arg(
        long,
        env = "EL_RPC_URL",
        value_name = "URL",
        default_value = "http://127.0.0.1:8545"
    )]
    pub el_rpc: String,

    /// Ethereum Consensus Layer Beacon API URL (e.g. http://127.0.0.1:5052)
    #[arg(
        long,
        env = "CL_BEACON_API_URL",
        value_name = "URL",
        default_value = "http://127.0.0.1:5052"
    )]
    pub cl_beacon_api: String,

    /// Output directory where exit receipts and evidence will be saved
    #[arg(
        short,
        long,
        default_value = "./staketrace_exit_output",
        value_name = "DIR"
    )]
    pub output_dir: PathBuf,

    /// Output format to print to stdout (all, markdown, json, csv)
    #[arg(long, value_enum, default_value = "all")]
    pub format: OutputFormat,

    /// HTTP request timeout in seconds
    #[arg(long, default_value_t = 30, value_name = "SECONDS")]
    pub timeout: u64,

    /// Enable live watch mode, polling until all requests are finalized or timeout
    #[arg(long)]
    pub watch: bool,

    /// Polling interval in seconds when in watch mode (default: 12 seconds)
    #[arg(long, default_value_t = 12, value_name = "SECONDS")]
    pub poll_interval: u64,

    /// Maximum duration in seconds to watch before timing out (default: 1800s / 30m)
    #[arg(long, default_value_t = 1800, value_name = "SECONDS")]
    pub watch_timeout: u64,

    /// Optional webhook URL (HTTP POST) to dispatch JSON notification upon completion
    #[arg(long, env = "STAKETRACE_WEBHOOK_URL", value_name = "URL")]
    pub webhook_url: Option<String>,

    /// Suppress informative logging
    #[arg(short, long)]
    pub quiet: bool,
}

impl ExitArgs {
    /// Returns trimmed and normalized transaction hashes.
    pub fn normalized_el_txs(&self) -> Vec<String> {
        self.el_txs
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.starts_with("0x") || s.starts_with("0X") {
                    s.to_lowercase()
                } else {
                    format!("0x{}", s.to_lowercase())
                }
            })
            .collect()
    }
}

impl VerifyArgs {
    /// Returns trimmed and normalized transaction hashes.
    pub fn normalized_el_txs(&self) -> Vec<String> {
        self.el_txs
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.starts_with("0x") || s.starts_with("0X") {
                    s.to_lowercase()
                } else {
                    format!("0x{}", s.to_lowercase())
                }
            })
            .collect()
    }
}

impl CliArgs {
    /// Generates shell completion script into the provided writer.
    pub fn generate_completions_to<W: io::Write>(shell: Shell, buf: &mut W) {
        let mut cmd = Self::command();
        clap_complete::generate(shell, &mut cmd, "staketrace", buf);
    }

    /// Prints shell autocompletions directly to standard output.
    pub fn print_completions(shell: Shell) {
        Self::generate_completions_to(shell, &mut io::stdout());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_defaults() {
        let args = CliArgs::parse_from([
            "staketrace",
            "--manifest",
            "manifest.json",
            "-t",
            "0x1234,0x5678",
        ]);

        assert_eq!(args.verify.manifest, Some(PathBuf::from("manifest.json")));
        assert_eq!(args.verify.el_txs, vec!["0x1234", "0x5678"]);
        assert_eq!(args.verify.normalized_el_txs(), vec!["0x1234", "0x5678"]);
        assert_eq!(args.verify.format, OutputFormat::All);
        assert!(!args.verify.quiet);
    }

    #[test]
    fn test_subcommand_simulate_parsing() {
        let args = CliArgs::parse_from([
            "staketrace",
            "simulate",
            "--manifest",
            "manifest.json",
            "--cl-beacon-api",
            "http://beacon:5052",
        ]);

        match args.command {
            Some(Commands::Simulate(sim)) => {
                assert_eq!(sim.manifest, PathBuf::from("manifest.json"));
                assert_eq!(sim.cl_beacon_api, "http://beacon:5052");
            }
            _ => panic!("Expected simulate subcommand"),
        }
    }

    #[test]
    fn test_normalized_el_txs_adds_prefix() {
        let args =
            CliArgs::parse_from(["staketrace", "--manifest", "manifest.json", "-t", "abcdef"]);
        assert_eq!(args.verify.normalized_el_txs(), vec!["0xabcdef"]);
    }

    #[test]
    fn test_generate_completions_output() {
        let mut buf = Vec::new();
        CliArgs::generate_completions_to(Shell::Bash, &mut buf);
        let script = String::from_utf8(buf).expect("completion script not utf8");
        assert!(script.contains("staketrace"));
    }
}
