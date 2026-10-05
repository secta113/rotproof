//! General-purpose parts that know no project concept.
//!
//! A part that can be named only by referring to where it is used, or that knows a project concept, belongs in
//! `domain`.
//!
//! Imports no other layer.

pub mod frontmatter;
pub mod markdown;
pub mod paths;
pub mod rust;
pub mod text;
