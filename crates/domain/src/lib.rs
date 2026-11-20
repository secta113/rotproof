//! What Rotproof keeps, as rules on values: the layers, what code says, the records, and how text, paths and source
//! are read.
//!
//! No I/O, and no call through a port: a rule takes the values a use case in `application` has read, and gives back
//! what it decides. The ports are contracts, in `protocol`. Imports no other layer.

pub mod approvals;
pub mod baseline;
pub mod bundle;
pub mod code;
pub mod crossing;
pub mod declaration;
pub mod definitions;
pub mod dependencies;
pub mod direction;
pub mod environment;
pub mod follows;
pub mod frontmatter;
pub mod guide;
pub mod hook;
pub mod inner_io;
pub mod layers;
pub mod links;
pub mod locked;
pub mod log;
pub mod markdown;
pub mod markers;
pub mod members;
pub mod modules;
pub mod moving;
pub mod outside;
pub mod paths;
pub mod pointer;
pub mod project;
pub mod python_builtins;
pub mod python_stdlib;
pub mod readers;
pub mod records;
pub mod running;
pub mod rust;
pub mod scaffold;
pub mod schema;
pub mod sentence;
pub mod settings;
pub mod structure;
pub mod suppressions;
pub mod templates;
pub mod text;
pub mod tree;
pub mod units;
pub mod units_doc;
pub mod upgrade;
pub mod version;
