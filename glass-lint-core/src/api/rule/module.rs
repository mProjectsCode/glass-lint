//! External module-specifier patterns.

use std::fmt;

use crate::{api::rule::error::MatcherBuildError, project::PackageSpecifier};

/// A package root with boundary-aware subpath matching.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ModuleSpecifierPattern {
    value: PatternValue,
}

/// The pattern kind behind a [`ModuleSpecifierPattern`].
///
/// Exact module identities remain distinct from package-root patterns
/// (core `ARCHITECTURE.md`); today exact module specifiers are modeled as
/// literal-string identities, so this is the single package-root variant.
/// An exact-module variant is the planned extension point.
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
mod tests;
