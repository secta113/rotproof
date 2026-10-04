//! What Rotproof keeps, as rules on values: the layers, what code says, the records, and the types of the ports to the
//! outside.
//!
//! No I/O, and no call through a port: a rule takes the values a use case in `application` has read, and gives back
//! what it decides. Imports `utils` only.

pub mod code;
pub mod hook;
pub mod layers;
pub mod project;
pub mod structure;
pub mod tree;
