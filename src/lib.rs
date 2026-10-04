//! The checks and the index generation. `main.rs` is only the command line around them.
//!
//! The modules are moving into the layers they belong to (`domain`, `application`, `infrastructure`); the ones at the
//! top have not been sorted yet.

pub mod application;
pub mod domain;
pub mod infrastructure;

pub mod bundle;
pub mod check;
pub mod create;
pub mod direction;
pub mod links;
pub mod markers;
pub mod schema;
pub mod structure;
