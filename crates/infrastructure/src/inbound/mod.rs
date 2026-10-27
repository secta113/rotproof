//! Entry points from outside: the command line. Each turns what comes in into a call of an input port in
//! `protocol.inbound`, and knows nothing else of the project: neither the use cases nor `domain`.
//!
//! May import `protocol.inbound`.

pub mod cli;
