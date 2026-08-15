use smol_str::SmolStr;

use super::{
    BuiltinModuleName, NormalizedOutsidePath, PackageSpecifier, ProjectPhaseError,
    ProjectRelativePath,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResolutionRequestKind {
    StaticImport,
    DynamicImport,
    Require,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ResolutionRequestKey {
    importer: ProjectRelativePath,
    kind: ResolutionRequestKind,
    range: glass_lint_datastructures::SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionRequest {
    key: ResolutionRequestKey,
    request: SmolStr,
}

impl ResolutionRequestKey {
    pub fn new(
        importer: ProjectRelativePath,
        kind: ResolutionRequestKind,
        range: glass_lint_datastructures::SourceRange,
    ) -> Self {
        Self {
            importer,
            kind,
            range,
        }
    }

    pub fn importer(&self) -> &ProjectRelativePath {
        &self.importer
    }

    pub fn kind(&self) -> ResolutionRequestKind {
        self.kind
    }

    pub fn range(&self) -> &glass_lint_datastructures::SourceRange {
        &self.range
    }
}

impl ResolutionRequest {
    pub fn new(key: ResolutionRequestKey, specifier: impl Into<SmolStr>) -> Self {
        Self {
            key,
            request: specifier.into(),
        }
    }

    pub fn key(&self) -> &ResolutionRequestKey {
        &self.key
    }

    pub fn importer(&self) -> &ProjectRelativePath {
        self.key.importer()
    }

    pub fn kind(&self) -> ResolutionRequestKind {
        self.key.kind()
    }

    pub fn range(&self) -> &glass_lint_datastructures::SourceRange {
        self.key.range()
    }

    pub fn specifier(&self) -> &SmolStr {
        &self.request
    }
}

/// The classified target of a resolved module request that is not linked
/// internally, shared by the authored-outcome and linked-target shapes so a
/// new target kind is declared once.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolvedTargetKind {
    External { package: PackageSpecifier },
    Builtin { name: BuiltinModuleName },
    Missing,
    OutsideProject { path: NormalizedOutsidePath },
    Unsupported { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolverOutcome {
    Internal { path: ProjectRelativePath },
    Target(ResolvedTargetKind),
}

impl ResolverOutcome {
    pub(crate) fn validate(self) -> Result<Self, ProjectPhaseError> {
        if let Self::Target(ResolvedTargetKind::Unsupported { reason }) = &self
            && reason.trim().is_empty()
        {
            return Err(ProjectPhaseError::InvalidTarget(reason.clone()));
        }
        Ok(self)
    }
}

impl From<ResolvedTargetKind> for ResolverOutcome {
    fn from(target: ResolvedTargetKind) -> Self {
        Self::Target(target)
    }
}

/// Stable opaque identity assigned from normalized project path order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModuleId(u32);

impl ModuleId {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkedModuleTarget {
    Internal { id: ModuleId },
    Target(ResolvedTargetKind),
}

impl From<ResolvedTargetKind> for LinkedModuleTarget {
    fn from(target: ResolvedTargetKind) -> Self {
        Self::Target(target)
    }
}
