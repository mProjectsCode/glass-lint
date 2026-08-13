use glass_lint_datastructures::SourceRange;

use crate::project::types::ProjectRelativePath;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SourceLocation {
    path: ProjectRelativePath,
    range: SourceRange,
}

impl SourceLocation {
    pub fn new(path: ProjectRelativePath, range: SourceRange) -> Self {
        Self { path, range }
    }

    pub fn path(&self) -> &ProjectRelativePath {
        &self.path
    }

    pub fn range(&self) -> &SourceRange {
        &self.range
    }

    pub fn range_owned(&self) -> SourceRange {
        self.range.clone()
    }
}
