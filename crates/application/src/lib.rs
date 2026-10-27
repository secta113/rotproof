//! Each command of Rotproof from start to finish: `commands.rs` implements the input port of `protocol::inbound`, each
//! use case reads through the output ports of `protocol::outbound`, whose implementations `composition` passes in,
//! and leaves each decision to the rules in `domain`.
//!
//! May import `protocol` and `domain`, never `infrastructure`.

pub mod approvals;
pub mod before;
pub mod bundle;
pub mod check;
pub mod code;
pub mod commands;
pub mod create;
pub mod direction;
pub mod follows;
pub mod hook;
pub mod index;
pub mod init;
pub mod layers;
pub mod links;
pub mod markers;
pub mod project;
pub mod structure;
pub mod tree;
