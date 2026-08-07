//! External module-specifier patterns.

use std::fmt;

use crate::{api::rule::error::MatcherBuildError, project::PackageSpecifier};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
/// An exact module specifier or a package root with boundary-aware subpaths.
pub struct ModuleSpecifierPattern {
    value: PatternValue,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum PatternValue {
    Package(PackageSpecifier),
}

impl ModuleSpecifierPattern {
    /// Construct a package-root pattern matching the root and `/...` subpaths.
    pub fn package(name: impl Into<String>) -> Result<Self, MatcherBuildError> {
        let name = name.into().trim().to_string();
        let package = PackageSpecifier::new(name.clone()).map_err(|_| {
            MatcherBuildError::InvalidModuleSpecifier(format!("invalid package specifier `{name}`"))
        })?;
        Ok(Self {
            value: PatternValue::Package(package),
        })
    }

    pub fn matches(&self, authored: &str) -> bool {
        match &self.value {
            PatternValue::Package(package) => {
                let root = package.as_str();
                authored == root
                    || authored
                        .strip_prefix(root)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            }
        }
    }

    pub fn as_str(&self) -> &str {
        match &self.value {
            PatternValue::Package(package) => package.as_str(),
        }
    }
}

impl fmt::Display for ModuleSpecifierPattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for value in [
            "",
            "pkg/",
            "pkg/subpath",
            "./pkg",
            "/pkg",
            "https://pkg",
            "pkg foo",
            "\\foo",
            "pkg\0foo",
        ] {
            assert!(ModuleSpecifierPattern::package(value).is_err(), "{value}");
        }
    }

    #[test]
    fn display_impl_shows_name() {
        let pkg = ModuleSpecifierPattern::package("bar").unwrap();
        assert_eq!(format!("{pkg}"), "bar");
    }

    #[test]
    fn scoped_package_rejects_empty_scope_or_name() {
        let result = ModuleSpecifierPattern::package("@/pkg");
        assert!(result.is_err());
    }
}
