//! `MAJOR.MINOR.PATCH` versions (tags `vX.Y.Z`).
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// `1.2.3` or `v1.2.3`; nothing else (no pre-release suffixes).
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let mut parts = text.split('.');
        let mut next = || -> Option<u32> {
            let part = parts.next()?;
            if part.is_empty() || part.len() > 9 || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let version = Self {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        parts.next().is_none().then_some(version)
    }
    /// The version this program was built as (Cargo workspace version).
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Self {
            major: 0,
            minor: 0,
            patch: 0,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_parse_and_order() {
        let v = |t| Version::parse(t).unwrap();
        assert_eq!(v("v2.5.3").to_string(), "2.5.3");
        assert_eq!(v("2.5.3"), v("v2.5.3"));
        assert!(v("2.5.10") > v("2.5.9"));
        assert!(v("2.10.0") > v("2.9.99"));
        assert!(v("3.0.0") > v("2.99.99"));
        for bad in [
            "",
            "v",
            "2.5",
            "2.5.3.1",
            "2.5.x",
            "v2.5.3-beta",
            "2..3",
            "+2.5.3",
            " v 2.5.3",
            "1234567890.0.0",
        ] {
            assert!(Version::parse(bad).is_none(), "{bad:?}");
        }
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
