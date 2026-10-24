//! Technical details: the I/O with the outside, in both directions. May also import external libraries.
//!
//! Its two levels never import each other. What both need is built at run time by `composition` and passed to each
//! (configuration, a connection, a logger), is a contract in `protocol` (a shape of data), or comes from an external
//! library (a technical tool).

pub mod inbound;
pub mod outbound;
