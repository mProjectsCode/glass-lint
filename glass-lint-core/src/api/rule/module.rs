//! External module-specifier patterns.

use std::fmt;

use crate::{api::rule::error::MatcherBuildError, project::types::package::PackageName};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
/// An exact module specifier or a package root with boundary-aware subpaths.
pub struct ModuleSpecifierPattern {
    name: String,
    kind: PatternKind,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum PatternKind {
    Exact,
    Package,
}

impl PatternKind {
    fn matches(self, name: &str, authored: &str) -> bool {
        authored == name
            || (matches!(self, Self::Package)
                && authored
                    .strip_prefix(name)
                    .is_some_and(|suffix| suffix.starts_with('/')))
    }

    fn is_package(self) -> bool {
        matches!(self, Self::Package)
    }
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
            name,
            kind: PatternKind::Exact,
        })
    }

    /// Construct a package-root pattern matching the root and `/...` subpaths.
    pub fn package(name: impl Into<String>) -> Result<Self, MatcherBuildError> {
        let name = name.into().trim().to_string();
        let package = PackageName::parse(&name).map_err(|_| {
            MatcherBuildError::InvalidModuleSpecifier(format!("invalid package specifier `{name}`"))
        })?;
        Ok(Self {
            name: package.as_str().to_owned(),
            kind: PatternKind::Package,
        })
    }

    pub fn matches(&self, authored: &str) -> bool {
        self.kind.matches(&self.name, authored)
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Return whether this pattern matches a package root and its subpaths.
    pub fn is_package(&self) -> bool {
        self.kind.is_package()
    }
}

impl fmt::Display for ModuleSpecifierPattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
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
