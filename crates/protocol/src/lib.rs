//! The contracts at the boundary of dependency inversion: interfaces, and the data they carry, with no
//! implementation. Because `application` and `infrastructure` both depend on these and not on each other, either
//! side can change or be replaced by a fake.
//!
//! Its two levels never import each other.
//!
//! May import `domain`.

pub mod inbound;
pub mod outbound;
