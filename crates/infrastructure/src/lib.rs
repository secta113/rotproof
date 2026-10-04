//! The ports in `domain` on the outside: the file system, git, and the parsers of code.
//!
//! Imports `domain`, `utils` and external libraries.

pub mod cargo;
pub mod disk;
pub mod git;
pub mod python;
pub mod readers;
pub mod typescript;
