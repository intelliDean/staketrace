# Staketrace (`staketrace`)

[![Crates.io](https://img.shields.io/crates/v/staketrace.svg)](https://crates.io/crates/staketrace)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![CI](https://github.com/intelliDean/staketrace/actions/workflows/ci.yml/badge.svg)](https://github.com/intelliDean/staketrace/actions/workflows/ci.yml)

**Staketrace** is a high-assurance CLI tool and Rust library to trace, simulate, and verify Ethereum validator consolidation requests (**EIP-7251 MaxEB**) and validator exits (**EIP-7002**) across the **Execution Layer (EL)** and **Consensus Layer (CL)** without relying on third-party indexers or commercial APIs.

---

## The Problem

Under Ethereum's **EIP-7251 (MaxEB)** consolidation mechanism (distinct from **EIP-7002**, which governs execution-layer-triggered validator exits), validator consolidation requests are initiated on the Execution Layer via the consolidation predeploy contract (`0x0000BBdDc7CE488642fb579F8B00f3a590007251`).

1. **The Cross-Layer Disconnect:** An EL transaction can succeed (`status == 1`) and consume gas, while the Consensus Layer may reject or drop the consolidation request during block state transition (e.g. mismatched withdrawal credentials, invalid source/target state, or validation rule violations).
2. **Partial Batch Failures:** In multi-validator consolidation batches, some pairs may succeed while others fail consensus validation rules.
3. **Hardware Decommissioning Danger:** If node operators assume EL transaction success equals consolidation completion, prematurely shutting down source validator keys before consensus processing will result in offline inactivity penalties.
4. **Dangling Fee-Exemption Permissions:** Temporary fee-exemption roles in staking vault contracts may remain unrevoked after batch consolidation workflows, creating accounting and governance risks.

---

## The Solution

`staketrace` traces every `source -> target` validator pair across both layers using exact block-level state delta proofs:

```
[Manifest & EL Tx Hashes]
        │
        ├──► 1. Execution Layer (EL RPC)
        │      • Confirms tx receipt status == 1
        │      • Matches exact 96-byte [source || target] predeploy calldata chunks
        │      • Maps EL block number & timestamp
        │
        ├──► 2. Consensus Layer (Beacon API)
        │      • Resolves 48-byte BLS pubkeys to CL Validator Indices & 0x01 credentials
        │      • Scans subsequent Beacon Block bodies for exact consolidation requests
        │      • Compares pending_consolidations in parent state (absent) vs post state (present)
        │      • Verifies block epoch finality checkpoints
        │
        ├──► 3. Staking / Vault Protocol Role Audit
        │      • Derives execution addresses from source validator withdrawal credentials
        │      • Audits fee-exemption role state across all source accounts (e.g. Lido stVault)
        │      • Warns if elevated privileges remain unrevoked
        │
        └──► 4. Generates Comprehensive Audit Receipts
               • Markdown Summary (`receipt_summary.md`)
               • Canonical Machine-Readable JSON (`receipt.json`)
               • Pair-by-Pair CSV (`consolidations.csv`)
               • Raw Evidence Dumps (`evidence/`)
```

---

## Status Classification

Every validator pair is deterministically classified into one of four statuses:

| Status | Meaning |
| :--- | :--- |
| **`ACCEPTED`** | Request proven included in a finalized Beacon block and newly transitioned (absent in parent state, present in post state). |
| **`QUEUED`** | Request verified on EL and/or included in Beacon block, awaiting block finalization or consensus epoch processing. |
| **`NOT_ACCEPTED`** | EL transaction reverted on-chain, or request was included in a block but rejected during block execution (absent in post state). |
| **`INDETERMINATE`** | Evidence cannot be proven (e.g. historical state pruned, endpoint returned 404/500, or missing receipt). |

> [!IMPORTANT]
> **Acceptance Condition:** The tool never returns `ACCEPTED` unless it proves the exact request was included in a finalized Beacon block and the exact pair was newly added between that block's parent state and post state.

---

## Installation & Building

### From Crates.io

```bash
cargo install staketrace
```

### From Source

```bash
# Requires Rust 1.85+
git clone https://github.com/intelliDean/staketrace.git
cd staketrace

# Build in release mode
cargo build --release

# The compiled binary will be at ./target/release/staketrace
```

---

## CLI Usage

`staketrace` provides two primary workflows: **Pre-Flight Simulation (`simulate`)** to dry-run validator batches before spending gas or signing transactions, and **Cross-Layer Verification (`verify`)** to prove exact Consensus Layer execution after broadcasting.

```
                  ┌─────────────────────────────────────────┐
                  │ 1. PRE-FLIGHT SIMULATION (`simulate`)   │
                  │   • Validates 0x01/0x02 credentials     │
                  │   • Enforces ≥32 ETH activation balance │
                  │   • Enforces ≤2,048 ETH MaxEB ceiling   │
                  │   • Detects pending queue collisions    │
                  │   • Estimates total batch gas overhead  │
                  └────────────────────┬────────────────────┘
                                       │ (Proceed only if all ELIGIBLE)
                                       ▼
                  ┌─────────────────────────────────────────┐
                  │ 2. BROADCAST ON-CHAIN TRANSACTION       │
                  │    Call 0x0000BBdDc7CE488642fb579F8B00  │
                  └────────────────────┬────────────────────┘
                                       │ (EL Tx Hash generated)
                                       ▼
                  ┌─────────────────────────────────────────┐
                  │ 3. CROSS-LAYER VERIFICATION (`verify`)  │
                  │   • Predeploy calldata byte matching    │
                  │   • Consensus block request scanning    │
                  │   • State delta (absent -> present)     │
                  │   • Epoch finality checkpoint proofs    │
                  └─────────────────────────────────────────┘
```

---

### Command 1: Pre-Flight Simulation (`staketrace simulate`)

Dry-run validator consolidation manifests against the live Beacon Chain before broadcasting transactions.

```bash
staketrace simulate --manifest <PATH> --cl-beacon-api <URL> [OPTIONS]
```

#### Options:
| Flag | Env Var | Default | Description |
| :--- | :--- | :--- | :--- |
| `-m, --manifest <PATH>` | - | *Required* | Path to validator consolidation manifest (JSON or YAML) |
| `--cl-beacon-api <URL>` | `CL_BEACON_API_URL` | `http://127.0.0.1:5052` | Consensus Layer Beacon API endpoint |
| `--el-rpc <URL>` | `EL_RPC_URL` | - | Optional Execution Layer JSON-RPC endpoint |
| `-o, --output-dir <DIR>` | - | `./staketrace_simulation` | Directory to save simulation report and JSON |
| `--format <FORMAT>` | - | `all` | Output format to print to stdout (`all`, `markdown`, `json`, `csv`) |
| `--timeout <SECONDS>` | - | `30` | Beacon API HTTP request timeout in seconds |
| `-q, --quiet` | - | `false` | Suppress interactive banners |

#### Example:
```bash
staketrace simulate \
  --manifest ./manifest.json \
  --cl-beacon-api https://bn.hoodi.ethpandaops.io \
  --output-dir ./sim_results
```

---

### Command 2: Cross-Layer Verification (`staketrace verify` or top-level)

Trace and mathematically prove consolidation request inclusion, state delta transitions, and finality across execution and consensus layers.

```bash
staketrace verify --manifest <PATH> --el-tx <TX_HASH> --el-rpc <URL> --cl-beacon-api <URL> [OPTIONS]
```

*(Note: top-level invocation `staketrace --manifest ... --el-tx ...` is fully backward compatible)*

#### Options:
| Flag | Env Var | Default | Description |
| :--- | :--- | :--- | :--- |
| `-m, --manifest <PATH>` | - | *Required* | Path to validator consolidation manifest (JSON or YAML) |
| `-t, --el-tx <TX_HASH>` | - | *Required* | EL transaction hash (comma-separated or repeated) |
| `--el-rpc <URL>` | `EL_RPC_URL` | `http://127.0.0.1:8545` | Execution Layer JSON-RPC endpoint |
| `--cl-beacon-api <URL>` | `CL_BEACON_API_URL` | `http://127.0.0.1:5052` | Consensus Layer Beacon API endpoint |
| `--st-vault-dashboard <ADDR>` | `ST_VAULT_DASHBOARD` | - | Optional Lido stVault Dashboard / ACL contract address |
| `-o, --output-dir <DIR>` | - | `./staketrace_output` | Directory to write receipts and evidence |
| `--format <FORMAT>` | - | `all` | Output format to print to stdout (`all`, `markdown`, `json`, `csv`) |
| `--timeout <SECONDS>` | - | `30` | HTTP request timeout for RPC and Beacon API queries |
| `--generate-completions <SHELL>` | - | - | Generate autocompletions (`bash`, `zsh`, `fish`, `powershell`, `elvish`) |
| `-q, --quiet` | - | `false` | Suppress interactive banners and informative logs |

#### Example:
```bash
staketrace verify \
  --manifest ./tests/fixtures/hoodi/manifest.json \
  --el-tx 0x4a2a33f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c4 \
  --el-rpc https://rpc.hoodi.ethpandaops.io \
  --cl-beacon-api https://bn.hoodi.ethpandaops.io \
  --st-vault-dashboard 0x1234567890123456789012345678901234567890 \
  --output-dir ./verification_receipts
```

---

## Manifest Format Support

Supports target-to-sources mapping formats as well as flat lists:

### Target-to-Sources Map Format:
```json
{
  "0x96b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c488": [
    "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1",
    "0xa4a233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee2"
  ]
}
```

### List Format:
```json
[
  {
    "target_pubkey": "0x96b6...",
    "source_pubkeys": ["0x8a92...", "0xa4a2..."]
  }
]
```

---

## Generated Output Files

### Verification Artifacts (`staketrace verify`):
1. **`receipt_summary.md`**: Human-readable report with status badges, delta proofs, and derived account role audits.
2. **`receipt.json`**: Full machine-readable receipt for automated CI/CD pipelines.
3. **`consolidations.csv`**: Tabular CSV breakdown of all source/target indices, tx hashes, and statuses.
4. **`evidence/`**: Raw block headers, execution receipts, and state query responses.

### Simulation Artifacts (`staketrace simulate`):
1. **`simulation_summary.md`**: Human-readable pre-flight safety report with gas estimates and balance projections.
2. **`simulation.json`**: Machine-readable pre-flight diagnosis for automated batch orchestrators.

---

## Running Tests

```bash
# Run all unit, mock integration, and wiremock tests
cargo test

# Run tests with live stdout output
cargo test -- --nocapture
```

---

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

