use glass_lint_datastructures::SourceRange;

use crate::project::types::{DiagnosticCode, ProjectRelativePath, SourceLocation};

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AnalysisDiagnostic {
    code: DiagnosticCode,
    message: String,
    location: Option<SourceLocation>,
}

impl AnalysisDiagnostic {
    pub fn new(code: DiagnosticCode, message: String, location: Option<SourceLocation>) -> Self {
        Self {
            code,
            message,
            location,
        }
    }

    pub fn code(&self) -> &DiagnosticCode {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    pub(crate) fn set_location(&mut self, location: Option<SourceLocation>) {
        self.location = location;
    }

    pub(crate) fn ordering_key(
        &self,
    ) -> (&str, Option<&ProjectRelativePath>, Option<&SourceRange>) {
        (
            self.code.as_str(),
            self.location.as_ref().map(SourceLocation::path),
            self.location.as_ref().map(SourceLocation::range),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case", tag = "kind"))]
pub enum Diagnostic {
    Parse {
        path: ProjectRelativePath,
        diagnostic: crate::ParseDiagnostic,
    },
    Project(AnalysisDiagnostic),
}

impl Diagnostic {
    pub(crate) fn parse(path: ProjectRelativePath, diagnostic: crate::ParseDiagnostic) -> Self {
        Self::Parse { path, diagnostic }
    }

    pub(crate) fn project(diagnostic: AnalysisDiagnostic) -> Self {
        Self::Project(diagnostic)
    }

    fn inner(
        &self,
    ) -> (
        &str,
        &str,
        Option<&ProjectRelativePath>,
        Option<&SourceRange>,
    ) {
        match self {
            Self::Parse { path, diagnostic } => (
                diagnostic.code().as_str(),
                diagnostic.message(),
                Some(path),
                diagnostic.range(),
            ),
            Self::Project(d) => (
                d.code.as_str(),
                &d.message,
                d.location.as_ref().map(SourceLocation::path),
                d.location.as_ref().map(SourceLocation::range),
            ),
        }
    }

    #[must_use]
    pub fn code(&self) -> &str {
        self.inner().0
    }

    #[must_use]
    pub fn message(&self) -> &str {
        self.inner().1
    }

    #[must_use]
    pub fn path(&self) -> Option<&ProjectRelativePath> {
        self.inner().2
    }

    #[must_use]
    pub fn range(&self) -> Option<&SourceRange> {
        self.inner().3
    }

    #[must_use]
    pub fn parse_diagnostic(&self) -> Option<&crate::ParseDiagnostic> {
        match self {
            Self::Parse { diagnostic, .. } => Some(diagnostic),
            Self::Project(_) => None,
        }
    }

    pub(crate) fn ordering_key(&self) -> (Option<&ProjectRelativePath>, &str, &str) {
        (self.path(), self.code(), self.message())
    }
}
