//! A finding of `rotproof check`, made from what a check found and the heading it is printed under.

use std::borrow::Cow;

use protocol::inbound::Finding;

/// Each of `details` as a finding printed under `check`.
pub fn under(
    check: impl Into<Cow<'static, str>>,
    details: impl IntoIterator<Item = String>,
) -> impl Iterator<Item = Finding> {
    let check = check.into();
    details.into_iter().map(move |detail| Finding {
        check: check.clone(),
        detail,
    })
}
