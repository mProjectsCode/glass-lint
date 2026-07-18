//! Validated external module-specifier patterns.

use std::fmt;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
/// An exact module specifier or a package root with boundary-aware subpaths.
pub struct ModuleSpecifierPattern {
    name: String,
    package: bool,
}

impl ModuleSpecifierPattern {
    /// Construct an exact authored module specifier.
    pub fn exact(name: impl Into<String>) -> Result<Self, String> {
        let name = name.into().trim().to_string();
        (!name.is_empty())
            .then_some(Self {
                name,
                package: false,
            })
            .ok_or_else(|| "module specifier must not be empty".into())
    }

    /// Construct a package-root pattern matching the root and `/...` subpaths.
    pub fn package(name: impl Into<String>) -> Result<Self, String> {
        let name = name.into().trim().to_string();
        if name.is_empty()
            || name.ends_with('/')
            || name.starts_with('.')
            || name.starts_with('/')
            || name.contains("://")
        {
            return Err(format!("invalid package specifier `{name}`"));
        }
        if let Some(scope) = name.strip_prefix('@') {
            let mut parts = scope.split('/');
            if parts.next().is_none_or(str::is_empty)
                || parts.next().is_none_or(str::is_empty)
                || parts.next().is_some()
            {
                return Err(format!("invalid scoped package specifier `{name}`"));
            }
        } else if name.contains('/') {
            return Err(format!("package root must not contain `/`: `{name}`"));
        }
        Ok(Self {
            name,
            package: true,
        })
    }

    pub fn matches(&self, authored: &str) -> bool {
        authored == self.name
            || (self.package
                && authored
                    .strip_prefix(&self.name)
                    .is_some_and(|suffix| suffix.starts_with('/')))
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }

    pub fn is_package(&self) -> bool {
        self.package
    }
}

impl fmt::Display for ModuleSpecifierPattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::ModuleSpecifierPattern;

    #[test]
    fn package_patterns_obey_boundaries() {
        let pattern = ModuleSpecifierPattern::package("@scope/pkg").unwrap();
        assert!(pattern.matches("@scope/pkg"));
        assert!(pattern.matches("@scope/pkg/subpath"));
        assert!(!pattern.matches("@scope/pkg-extra"));
        assert!(!pattern.matches("@scope/pkgx/subpath"));
    }

    #[test]
    fn package_patterns_reject_non_packages() {
        for value in ["", "pkg/", "pkg/subpath", "./pkg", "/pkg", "https://pkg"] {
            assert!(ModuleSpecifierPattern::package(value).is_err(), "{value}");
        }
    }
}
