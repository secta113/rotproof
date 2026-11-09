//! Keeps a project's structure: makes and checks its layers (as `.config/rotproof.toml` declares them) and the records
//! that keep the code's reasons (knowledge pinned to the code, and the log, in `docs/`, written in OKF 0.2), and writes
//! their index files.
//!
//! The entry file: it calls `composition` to build the program, and starts it. No layer.

use std::process::ExitCode;

fn main() -> ExitCode {
    composition::run()
}
