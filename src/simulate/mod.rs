pub mod markdown;
pub mod models;
pub mod runner;

pub use markdown::generate_simulation_markdown;
pub use models::{
    ESTIMATED_GAS_PER_CONSOLIDATION, MAX_EFFECTIVE_BALANCE_ELECTRA_GWEI,
    MIN_ACTIVATION_BALANCE_GWEI, PairSimulationResult, SimulationReport, SimulationSummary,
};
pub use runner::SimulationEngine;
