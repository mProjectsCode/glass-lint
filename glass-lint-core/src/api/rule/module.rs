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
    Exact(String),
    Package(PackageSpecifier),
}

impl ModuleSpecifierPattern {
    /// Construct an exact authored module specifier.
    pub fn exact(name: impl Into<String>) -> Result<Self, MatcherBuildError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(MatcherBuildError::InvalidModuleSpecifier(
                "module specifier must not be empty".into(),
            ));
        }
        Ok(Self {
            value: PatternValue::Exact(name),
        })
    }

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
            PatternValue::Exact(name) => authored == name,
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
            PatternValue::Exact(name) => name,
            PatternValue::Package(package) => package.as_str(),
        }
    }

    /// Return whether this pattern matches a package root and its subpaths.
    pub fn is_package(&self) -> bool {
        matches!(self.value, PatternValue::Package(_))
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
    fn exact_pattern_matches_itself_and_rejects_subpaths() {
        let pattern = ModuleSpecifierPattern::exact("lodash").unwrap();
        assert!(pattern.matches("lodash"));
        assert!(!pattern.matches("lodash/map"));
        assert!(!pattern.matches("lodash-extra"));
    }

    #[test]
    fn exact_pattern_rejects_empty_string() {
        assert!(ModuleSpecifierPattern::exact("").is_err());
    }

    #[test]
    fn exact_pattern_trims_whitespace() {
        let pattern = ModuleSpecifierPattern::exact("  lodash  ").unwrap();
        assert!(pattern.matches("lodash"));
        assert!(!pattern.matches("  lodash  "));
    }

    #[test]
    fn exact_pattern_as_str_and_not_package() {
        let pattern = ModuleSpecifierPattern::exact("react").unwrap();
        assert_eq!(pattern.as_str(), "react");
        assert!(!pattern.is_package());
    }

    #[test]
    fn package_pattern_as_str_and_is_package() {
        let pattern = ModuleSpecifierPattern::package("@scope/pkg").unwrap();
        assert_eq!(pattern.as_str(), "@scope/pkg");
        assert!(pattern.is_package());
    }

    #[test]
    fn display_impl_shows_name() {
        let exact = ModuleSpecifierPattern::exact("foo").unwrap();
        let pkg = ModuleSpecifierPattern::package("bar").unwrap();
        assert_eq!(format!("{exact}"), "foo");
        assert_eq!(format!("{pkg}"), "bar");
    }

    #[test]
    fn scoped_package_rejects_empty_scope_or_name() {
        let result = ModuleSpecifierPattern::package("@/pkg");
        assert!(result.is_err());
    }
}
