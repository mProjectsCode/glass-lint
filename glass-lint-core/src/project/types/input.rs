use std::{borrow::Borrow, ops::Deref, sync::Arc};

use smol_str::SmolStr;

use crate::{SourceLanguage, project::types::ProjectRelativePath};

mod errors;
mod resolution;

pub use errors::*;
pub use resolution::*;

/// Shared source text accepted once at the project boundary.
///
/// The public project DTO still serializes as a string, but every internal
/// consumer clones only this cheap handle instead of copying the source.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct SourceText(Arc<str>);

impl SourceText {
    pub fn new(source: impl Into<Arc<str>>) -> Self {
        Self(source.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for SourceText {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl From<String> for SourceText {
    fn from(source: String) -> Self {
        Self::new(Arc::<str>::from(source))
    }
}

impl From<Arc<str>> for SourceText {
    fn from(source: Arc<str>) -> Self {
        Self(source)
    }
}

impl From<&str> for SourceText {
    fn from(source: &str) -> Self {
        Self::new(Arc::<str>::from(source))
    }
}

macro_rules! impl_validated_text_traits {
    ($type:ty) => {
        impl Deref for $type {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                self.as_str()
            }
        }

        impl AsRef<str> for $type {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl Borrow<str> for $type {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl std::fmt::Display for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl PartialEq<&str> for $type {
            fn eq(&self, other: &&str) -> bool {
                self.as_str() == *other
            }
        }
    };
}

/// The canonical validated package-root value (e.g., "lodash",
/// "@angular/core") shared by project inputs and rule declarations.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageSpecifier(SmolStr);

impl PackageSpecifier {
    pub fn new(s: impl Into<SmolStr>) -> Result<Self, ProjectInputError> {
        let inner = s.into();
        let value = inner.trim();
        if value.is_empty()
            || value.contains('\0')
            || value.contains(char::is_whitespace)
            || value.starts_with('.')
            || value.starts_with('/')
            || value.starts_with('\\')
        {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }

        if let Some(scoped) = value.strip_prefix('@') {
            let mut segments = scoped.split('/');
            if segments.next().is_none_or(str::is_empty)
                || segments.next().is_none_or(str::is_empty)
                || segments.next().is_some()
            {
                return Err(ProjectInputError::InvalidTarget(inner.to_string()));
            }
        } else if value.contains('/') {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }

        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated scheme-qualified builtin module name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BuiltinModuleName(SmolStr);

impl BuiltinModuleName {
    pub fn new(s: impl Into<SmolStr>) -> Result<Self, ProjectInputError> {
        let inner = s.into();
        let trimmed = inner.trim();
        if trimmed.is_empty() || trimmed.contains('\0') || trimmed.contains(char::is_whitespace) {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }
        let Some((scheme, name)) = trimmed.split_once(':') else {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        };
        if scheme.is_empty() || name.is_empty() {
            return Err(ProjectInputError::InvalidTarget(inner.to_string()));
        }
        Ok(Self(trimmed.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A normalized outside-project path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NormalizedOutsidePath(SmolStr);

impl NormalizedOutsidePath {
    pub fn new(s: impl Into<SmolStr>) -> Result<Self, ProjectInputError> {
        let inner = s.into();
        let normalized = crate::project::input::normalize_outside_target(inner.as_str())?;
        Ok(Self(SmolStr::from(normalized)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl_validated_text_traits!(PackageSpecifier);
impl_validated_text_traits!(BuiltinModuleName);
impl_validated_text_traits!(NormalizedOutsidePath);

impl AsRef<std::path::Path> for NormalizedOutsidePath {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(&self.0)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SourceFile {
    path: ProjectRelativePath,
    language: SourceLanguage,
    source: SourceText,
}

impl SourceFile {
    /// Construct a virtual source using JavaScript parser semantics.
    /// Filename extensions do not select a language; filesystem acceptance
    /// supplies one explicitly through [`Self::with_language`].
    pub fn new(
        path: impl Into<String>,
        source: impl Into<SourceText>,
    ) -> Result<Self, ProjectInputError> {
        Self::with_language(path, source, SourceLanguage::JavaScript)
    }

    /// Construct a virtual source with an explicit parser language.
    pub fn with_language(
        path: impl Into<String>,
        source: impl Into<SourceText>,
        language: SourceLanguage,
    ) -> Result<Self, ProjectInputError> {
        let path = ProjectRelativePath::new(path.into())?;
        Ok(Self::from_parts(path, source.into(), language))
    }

    /// Construct from a validated project-relative path with an explicit
    /// language, ignoring the filename extension.
    pub fn from_relative_with_language(
        path: ProjectRelativePath,
        source: impl Into<SourceText>,
        language: SourceLanguage,
    ) -> Self {
        Self::from_parts(path, source.into(), language)
    }

    fn from_parts(path: ProjectRelativePath, source: SourceText, language: SourceLanguage) -> Self {
        Self {
            path,
            language,
            source,
        }
    }

    pub fn path(&self) -> &ProjectRelativePath {
        &self.path
    }

    pub fn language(&self) -> SourceLanguage {
        self.language
    }

    pub fn source(&self) -> &SourceText {
        &self.source
    }
}

#[cfg(test)]
mod tests;
