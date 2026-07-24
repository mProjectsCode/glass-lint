use glass_lint_datastructures::NameId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Stable identity of a lexical scope within one analyzed module.
pub(in crate::analysis) struct ScopeId(usize);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A name resolved within one lexical scope.
pub(in crate::analysis) struct ScopedName {
    scope: ScopeId,
    name: NameId,
}

impl ScopedName {
    pub(in crate::analysis) fn new(scope: ScopeId, name: NameId) -> Self {
        Self { scope, name }
    }

    pub(in crate::analysis) fn scope(&self) -> ScopeId {
        self.scope
    }

    pub(in crate::analysis) fn name(&self) -> NameId {
        self.name
    }
}

impl ScopeId {
    pub(in crate::analysis) fn index(self) -> usize {
        self.0
    }
}

impl From<usize> for ScopeId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
