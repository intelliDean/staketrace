//! Terminal display, progress reporting, and tabular rendering.

pub mod banner;
pub mod tables;

pub use banner::{print_banner, print_connection_info};
pub use tables::{
    print_exit_results, print_requested_format, print_simulation_results,
    print_verification_results,
};
