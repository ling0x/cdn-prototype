use std::fmt::{Display, Formatter};

/// Stable identifier for a package blob at the edge (path-safe).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageKey(String);

impl PackageKey {
    pub fn new(s: impl Into<String>) -> Result<Self, &'static str> {
        let s = s.into();
        if s.is_empty() || s.contains('/') || s.contains('\\') {
            return Err("package key must be non-empty and must not contain path separators");
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for PackageKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
