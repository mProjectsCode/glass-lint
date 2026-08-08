//! Typed, scoped completeness state for bounded semantic analysis.

use std::collections::BTreeSet;

use crate::{
    parse::ParseFailureKind,
    project::{AnalysisDiagnostic, ProjectRelativePath, types::DiagnosticKind},
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum AnalysisComponent {
    Facts,
    Effects,
    Flow,
    Linking,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ModuleInterfaceKind {
    CommonJsExports,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResolutionKind {
    Unsupported,
    OutsideProject,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum IncompleteReason {
    InvalidParserSpan,
    ParseFailure {
        kind: ParseFailureKind,
    },
    SemanticBudgetExhausted {
        limit: usize,
        used: usize,
    },
    FactCapacityExhausted {
        limit: usize,
    },
    PathCapacityExhausted,
    ValueArenaExhausted,
    BudgetExhausted {
        component: AnalysisComponent,
        limit: usize,
        observed: Option<usize>,
    },
    NameExhausted {
        limit: usize,
        attempted: usize,
    },
    UnsupportedModuleInterface {
        kind: ModuleInterfaceKind,
    },
    UnsupportedResolution {
        request: String,
        kind: ResolutionKind,
    },
    MissingInternalResolution {
        request: String,
    },
    AmbiguousStarExport {
        request: String,
    },
    ScopeShapeMismatch {
        count: usize,
    },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StatusScope {
    File(ProjectRelativePath),
    Project,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) struct StatusEntry {
    pub(in crate::analysis) scope: StatusScope,
    pub(in crate::analysis) reason: IncompleteReason,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AnalysisStatus {
    entries: BTreeSet<StatusEntry>,
}

pub struct StatusDiagnostics {
    files: Vec<(ProjectRelativePath, AnalysisDiagnostic)>,
    project: Vec<AnalysisDiagnostic>,
}

impl StatusDiagnostics {
    pub fn into_parts(
        self,
    ) -> (
        Vec<(ProjectRelativePath, AnalysisDiagnostic)>,
        Vec<AnalysisDiagnostic>,
    ) {
        (self.files, self.project)
    }
}

impl AnalysisStatus {
    pub fn record(&mut self, scope: StatusScope, reason: IncompleteReason) {
        self.entries.insert(StatusEntry { scope, reason });
    }

    pub(in crate::analysis) fn extend(&mut self, other: &Self) {
        self.entries.extend(other.entries.iter().cloned());
    }

    pub fn is_complete(&self) -> bool {
        self.entries.is_empty()
    }

    pub(in crate::analysis) fn for_local_file(&self, path: &ProjectRelativePath) -> Self {
        Self {
            entries: self
                .entries
                .iter()
                .map(|entry| StatusEntry {
                    scope: match &entry.scope {
                        StatusScope::Project => StatusScope::File(path.clone()),
                        StatusScope::File(existing) => StatusScope::File(existing.clone()),
                    },
                    reason: entry.reason.clone(),
                })
                .collect(),
        }
    }

    pub(crate) fn diagnostics(&self) -> StatusDiagnostics {
        let mut files = Vec::new();
        let mut project = Vec::new();
        for entry in &self.entries {
            // Parse status and parser presentation deliberately have separate
            // payloads. The status entry is always recorded from the parser
            // code and is the sole completion input; the structured parser
            // diagnostic separately retains its original message and range.
            // Skipping it here prevents duplicate presentation without making
            // the presentation diagnostic a completion side channel.
            if matches!(entry.reason, IncompleteReason::ParseFailure { .. }) {
                continue;
            }
            let diagnostic = entry.reason.diagnostic();
            match &entry.scope {
                StatusScope::File(path) => files.push((path.clone(), diagnostic)),
                StatusScope::Project => project.push(diagnostic),
            }
        }
        StatusDiagnostics { files, project }
    }
}

impl ParseFailureKind {
    fn diagnostic(self) -> (DiagnosticKind, &'static str) {
        match self {
            Self::Syntax => (DiagnosticKind::SyntaxError, "source could not be parsed"),
            Self::SourceTooLarge => (
                DiagnosticKind::SourceTooLarge,
                "source exceeds the analysis limit",
            ),
            Self::SyntaxDepth => (
                DiagnosticKind::SyntaxDepthExceeded,
                "source exceeds the nesting-depth analysis limit",
            ),
        }
    }
}

impl AnalysisComponent {
    fn budget_diagnostic(self) -> (DiagnosticKind, &'static str) {
        match self {
            Self::Facts => (
                DiagnosticKind::FactsBudgetExhausted,
                "semantic analysis exceeded its bounded fact budget",
            ),
            Self::Effects => (
                DiagnosticKind::EffectsBudgetExhausted,
                "function-effect extraction exceeded its bounded budget",
            ),
            Self::Flow => (
                DiagnosticKind::FlowBudgetExhausted,
                "qualified function-effect projection exceeded its bounded budget",
            ),
            Self::Linking => (
                DiagnosticKind::LinkingBudgetExhausted,
                "module linking exceeded its bounded budget",
            ),
        }
    }
}

impl IncompleteReason {
    fn diagnostic(&self) -> AnalysisDiagnostic {
        // Single match over all status variants: each arm pairs a diagnostic
        // kind with a message template. Keeping them together ensures every
        // variant maps to exactly one (code, message) pair without drift.
        let (code, message) = match self {
            Self::InvalidParserSpan => (
                DiagnosticKind::InvalidParserSpan,
                "parser produced a source range outside authored UTF-8 boundaries".into(),
            ),
            Self::ParseFailure { kind } => {
                let (code, text) = kind.diagnostic();
                (code, text.into())
            }
            Self::SemanticBudgetExhausted { limit, used } => (
                DiagnosticKind::SemanticBudgetExhausted,
                format!("semantic analysis exceeded its step budget; limit={limit}, used={used}"),
            ),
            Self::FactCapacityExhausted { limit } => (
                DiagnosticKind::FactCapacityExhausted,
                format!("semantic analysis exceeded its fact capacity; limit={limit}"),
            ),
            Self::PathCapacityExhausted => (
                DiagnosticKind::PathCapacityExhausted,
                "semantic analysis exceeded its path interning capacity".into(),
            ),
            Self::ValueArenaExhausted => (
                DiagnosticKind::ValueArenaExhausted,
                "semantic analysis exceeded its value arena capacity".into(),
            ),
            Self::BudgetExhausted {
                component,
                limit,
                observed,
            } => {
                let (code, text) = component.budget_diagnostic();
                (
                    code,
                    format!("{text}; limit={limit}, observed={observed:?}"),
                )
            }
            Self::NameExhausted { limit, attempted } => (
                DiagnosticKind::NameBudgetExhausted,
                format!("semantic name table exhausted; limit={limit}, attempted={attempted}"),
            ),
            Self::UnsupportedModuleInterface {
                kind: ModuleInterfaceKind::CommonJsExports,
            } => (
                DiagnosticKind::UnsupportedCommonjsExports,
                "CommonJS export shape is dynamic or ambiguous".into(),
            ),
            Self::UnsupportedResolution { request, kind } => {
                let text = match kind {
                    ResolutionKind::Unsupported => "is not an analyzable project target",
                    ResolutionKind::OutsideProject => "resolves outside the project",
                };
                let code = match kind {
                    ResolutionKind::Unsupported => DiagnosticKind::UnsupportedProjectTarget,
                    ResolutionKind::OutsideProject => DiagnosticKind::OutsideProjectTarget,
                };
                (code, format!("module request `{request}` {text}"))
            }
            Self::MissingInternalResolution { request } => (
                DiagnosticKind::UnresolvedInternalRequest,
                format!("internal module request `{request}` has no resolution"),
            ),
            Self::AmbiguousStarExport { request } => (
                DiagnosticKind::AmbiguousStarExport,
                format!("module interface for `{request}` is ambiguous"),
            ),
            Self::ScopeShapeMismatch { count } => (
                DiagnosticKind::ScopeShapeMismatch,
                format!("scope collection encountered {count} structural issue(s)"),
            ),
        };
        AnalysisDiagnostic::new(code.into(), message, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> ProjectRelativePath {
        ProjectRelativePath::new("main.js").unwrap()
    }

    #[test]
    fn status_diagnostics_are_deduplicated_and_stable() {
        let mut status = AnalysisStatus::default();
        let reason = IncompleteReason::BudgetExhausted {
            component: AnalysisComponent::Facts,
            limit: 2,
            observed: Some(2),
        };
        status.record(StatusScope::File(file()), reason.clone());
        status.record(StatusScope::File(file()), reason);
        let (files, project) = status.diagnostics().into_parts();
        assert_eq!(files.len(), 1);
        assert!(project.is_empty());
        assert_eq!(files[0].1.code().as_str(), "semantic_budget_exhausted");
        assert!(files[0].1.message().contains("limit=2"));
    }

    #[test]
    fn completion_depends_only_on_status_entries() {
        let mut status = AnalysisStatus::default();
        assert!(status.is_complete());
        status.record(
            StatusScope::Project,
            IncompleteReason::MissingInternalResolution {
                request: "./dep.js".into(),
            },
        );
        assert!(!status.is_complete());
    }

    #[test]
    fn local_file_conversion_preserves_existing_file_scope() {
        let mut status = AnalysisStatus::default();
        let reason = IncompleteReason::PathCapacityExhausted;
        status.record(StatusScope::Project, reason.clone());
        status.record(StatusScope::File(file()), reason);

        let converted = status.for_local_file(&ProjectRelativePath::new("other.js").unwrap());
        let (files, project) = converted.diagnostics().into_parts();

        assert!(project.is_empty());
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0.as_str(), "main.js");
        assert_eq!(files[1].0.as_str(), "other.js");
    }
}
