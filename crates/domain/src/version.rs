//! A version as SemVer writes and orders it, and the version of Rotproof that runs.
//!
//! A project's files are up to a version (`files` in the declaration), and the upgrade, the check of it and the
//! crossing of a version each compare two, so how a version is read and ordered is one rule here. A pre-release comes
//! before the release it leads to; build metadata and a leading `v` are not versions. The running version is here too,
//! as Cargo names it, so what reads the declaration and what upgrades it take it from one place.

/// The version of Rotproof that runs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A version as SemVer writes and orders it: `major.minor.patch`, then an optional pre-release after `-`
/// (`1.0.0-rc.1`, the form Cargo takes, which PyPI shows as `1.0.0rc1`). A pre-release comes before the release it
/// leads to, so a project whose files are up to `1.0.0-rc.1` is behind `1.0.0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    core: (u64, u64, u64),
    /// The identifiers of the pre-release, empty for a release
    pre: Vec<Identifier>,
}

/// One dot-separated identifier of a pre-release: digits are compared as a number and come before any other.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Identifier {
    Number(u64),
    Text(String),
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        self.core
            .cmp(&other.core)
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                // Identifier by identifier, and a shorter list that is a prefix of the other comes first, as SemVer
                // orders them
                (false, false) => self.pre.cmp(&other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The version `text` names, or `None` when it is not one: no `v`, no space, no build metadata (`+...`).
pub fn version(text: &str) -> Option<Version> {
    let (core, pre) = match text.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (text, None),
    };
    let mut parts = core.split('.').map(|part| {
        // A part is digits only: no sign, no space
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<u64>().ok())
            .flatten()
    });
    let core = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() {
        return None;
    }
    let pre = match pre {
        None => Vec::new(),
        Some(pre) => pre.split('.').map(identifier).collect::<Option<Vec<_>>>()?,
    };
    Some(Version { core, pre })
}

/// An identifier of a pre-release: ASCII letters, digits and `-`, not empty, and digits alone without a leading zero.
fn identifier(text: &str) -> Option<Identifier> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return None;
    }
    if text.bytes().all(|b| b.is_ascii_digit()) {
        if text.len() > 1 && text.starts_with('0') {
            return None;
        }
        return text.parse().ok().map(Identifier::Number);
    }
    Some(Identifier::Text(text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_three_numbers_and_a_pre_release() {
        for good in [
            "0.2.0",
            "10.0.12",
            "1.0.0-rc.1",
            "1.0.0-alpha",
            "1.0.0-x-y.7.z",
        ] {
            assert!(version(good).is_some(), "{good}");
        }
        for bad in [
            "0.2",
            "0.2.0.1",
            "0.2.x",
            "v0.2.0",
            " 0.2.0",
            "0..1",
            "+1.0.0",
            "1.0.0-",
            "1.0.0-rc..1",
            "1.0.0-rc.01",
            "1.0.0-rc_1",
            "1.0.0+build",
            // The form PyPI shows: the declaration takes the version as Rotproof writes it, from Cargo
            "1.0.0rc1",
        ] {
            assert_eq!(version(bad), None, "{bad}");
        }
        assert!(version("0.10.0") > version("0.9.9"));
    }

    #[test]
    fn a_pre_release_comes_before_its_release_in_semver_order() {
        // SemVer 2.0.0, section 11, with the release Rotproof is going to
        let ordered = [
            "0.4.0",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0-rc.2",
            "1.0.0-rc.10",
            "1.0.0",
            "1.0.1-rc.1",
        ];
        for pair in ordered.windows(2) {
            assert!(version(pair[0]) < version(pair[1]), "{pair:?}");
        }
        assert_eq!(version("1.0.0-rc.1"), version("1.0.0-rc.1"));
    }
}
