//! Keeps a project's structure: makes and checks its layers (as `.config/rotproof.toml` declares them) and the records
//! an agent works from (specs, work items, knowledge and log in `docs/`, written in OKF 0.2), and writes their index
//! files.
//!
//! The entry file: it calls `composition` to build the program, and starts it. No layer.

use std::process::ExitCode;

fn main() -> ExitCode {
    composition::run()
}
