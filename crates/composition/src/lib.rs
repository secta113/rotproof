//! The composition root: the one place that wires the program. It builds the implementations in
//! `infrastructure.outbound`, passes them to the use cases in `application`, and passes the use cases to the entry
//! points in `infrastructure.inbound` and to the UI.
//!
//! What the entry points and the connections both need at run time (configuration, a database connection, a logger,
//! a clock) is built here and passed to each.
//!
//! May import `infrastructure`, `application`, `protocol` and `domain`.

use std::process::ExitCode;

use application::commands::Rotproof;
use infrastructure::inbound::cli;
use infrastructure::outbound::Local;

/// Build Rotproof on the machine it runs on, and run the command line with it.
pub fn run() -> ExitCode {
    let machine = Local;
    cli::run(&Rotproof::new(&machine))
}
