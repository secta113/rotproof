//! Each command of Rotproof from start to finish: it reads through the ports in `domain`, which `handler` passes in,
//! and leaves each decision to the rules in `domain`.
//!
//! Imports `domain` and `utils`, never `infrastructure`.

pub mod code;
pub mod direction;
pub mod hook;
pub mod init;
pub mod layers;
pub mod links;
pub mod markers;
pub mod project;
pub mod structure;
pub mod tree;
