//! What Rotproof keeps, as rules on values: the layers, what code says, the records, and how text, paths and source
//! are read.
//!
//! No I/O, and no call through a port: a rule takes the values a use case in `application` has read, and gives back
//! what it decides. The ports are contracts, in `protocol`. Imports no other layer.

pub mod approvals;
pub mod arrows;
pub mod bundle;
pub mod code;
pub mod direction;
pub mod follows;
pub mod frontmatter;
pub mod hook;
pub mod layers;
pub mod links;
pub mod markdown;
pub mod markers;
pub mod migrate;
pub mod modules;
pub mod paths;
pub mod project;
pub mod records;
pub mod rust;
pub mod schema;
pub mod structure;
pub mod text;
pub mod tree;
pub mod upgrade;
