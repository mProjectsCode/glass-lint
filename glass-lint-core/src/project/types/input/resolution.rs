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

    pub fn range_owned(&self) -> glass_lint_datastructures::SourceRange {
        self.range.clone()
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

    pub fn range_owned(&self) -> glass_lint_datastructures::SourceRange {
        self.key.range_owned()
    }

    pub fn specifier(&self) -> &SmolStr {
        &self.request
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolverOutcome {
    Internal { path: ProjectRelativePath },
    External { package: PackageSpecifier },
    Builtin { name: BuiltinModuleName },
    Missing,
    OutsideProject { path: NormalizedOutsidePath },
    Unsupported { reason: String },
}

impl ResolverOutcome {
    pub(crate) fn validate(self) -> Result<Self, ProjectPhaseError> {
        if let Self::Unsupported { reason } = &self
            && reason.trim().is_empty()
        {
            return Err(ProjectPhaseError::InvalidTarget(reason.clone()));
        }
        Ok(self)
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
    External { package: PackageSpecifier },
    Builtin { name: BuiltinModuleName },
    Missing,
    OutsideProject { path: NormalizedOutsidePath },
    Unsupported { reason: String },
}
