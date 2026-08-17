use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use hashbrown::HashMap;
use swc_common::Span;

use crate::analysis::syntax::span_contains;

mod provenance;
pub(in crate::analysis) use provenance::ProvenanceJoin;
pub use provenance::{
    AliasAssignment, BindingProvenance, BoundArgument, IdentValueSeed, MemberValueSeed,
    ProvenanceAlternatives,
};

// ── Identifiers ──────────────────────────────────────────────────────────

/// Index into [`LexicalScopes`]. Scope storage is an unbounded `Vec`, so this
/// id uses `usize` rather than the bounded `u32` arena-id convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(usize);

impl ScopeId {
    #[cfg(test)]
    pub(in crate::analysis) const fn index_for_test(self) -> usize {
        self.0
    }
}

#[cfg(test)]
crate::impl_test_id_constructor!(ScopeId, usize);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopedName {
    scope: ScopeId,
    name: NameId,
}

impl ScopedName {
    pub fn new(scope: ScopeId, name: NameId) -> Self {
        Self { scope, name }
    }

    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    pub fn name(&self) -> NameId {
        self.name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingId(u32);

impl BindingId {
    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

#[cfg(test)]
crate::impl_test_id_constructor!(BindingId, u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingVersion(u32);

impl BindingVersion {
    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

#[cfg(test)]
crate::impl_test_id_constructor!(BindingVersion, u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId(u32);

impl FunctionId {
    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

#[cfg(test)]
crate::impl_test_id_constructor!(FunctionId, u32);

impl From<FunctionId> for u32 {
    fn from(id: FunctionId) -> Self {
        id.0
    }
}

impl glass_lint_datastructures::IdIndex for FunctionId {
    fn from_raw(raw: u32) -> Self {
        Self::new(raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BindingRoot {
    Binding {
        function: FunctionId,
        binding: BindingId,
        version: BindingVersion,
    },
    Global(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingKey {
    root: BindingRoot,
    path: NamePath,
}

impl BindingKey {
    pub fn new(root: BindingRoot) -> Self {
        Self {
            root,
            path: NamePath::new(),
        }
    }

    pub(in crate::analysis) fn lexical(
        function: FunctionId,
        binding: BindingId,
        version: BindingVersion,
    ) -> Self {
        Self::new(BindingRoot::Binding {
            function,
            binding,
            version,
        })
    }

    pub(in crate::analysis) fn global(name: impl Into<String>) -> Self {
        Self::new(BindingRoot::Global(name.into()))
    }

    pub fn append_segment(&mut self, segment: NameId) {
        self.path.append(segment);
    }

    pub fn binding_slot(&self) -> Option<BindingSlot> {
        match self.root {
            BindingRoot::Binding {
                function, binding, ..
            } => Some(BindingSlot::new(function, binding, self.path.clone())),
            BindingRoot::Global(_) => None,
        }
    }
}

/// Stable identity of one lexical binding slot across binding versions.
/// Binding versions differ at joins, but the slot remains the same variable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingSlot {
    function: FunctionId,
    binding: BindingId,
    path: NamePath,
}

impl BindingSlot {
    pub(in crate::analysis) fn new(
        function: FunctionId,
        binding: BindingId,
        path: NamePath,
    ) -> Self {
        Self {
            function,
            binding,
            path,
        }
    }
}

// ── Scope data types ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LexicalScope {
    span: Span,
    depth: usize,
    kind: ScopeKind,
    parent: Option<ScopeId>,
    bindings: HashMap<NameId, BindingProvenance>,
}

/// Ordered lexical-scope storage owned by the scope-analysis pipeline.
///
/// Scope IDs are stable positions in this collection, but callers use the
/// collection's named accessors rather than depending on its vector layout.
#[derive(Debug, Clone, Default)]
pub(in crate::analysis) struct LexicalScopes(Vec<LexicalScope>);

impl From<Vec<LexicalScope>> for LexicalScopes {
    fn from(scopes: Vec<LexicalScope>) -> Self {
        Self(scopes)
    }
}

impl LexicalScopes {
    pub(in crate::analysis) fn new() -> Self {
        Self::default()
    }

    pub(in crate::analysis) fn push(&mut self, scope: LexicalScope) -> ScopeId {
        let id = ScopeId(self.0.len());
        self.0.push(scope);
        id
    }

    #[cfg(test)]
    pub(in crate::analysis) fn len(&self) -> usize {
        self.0.len()
    }

    pub(in crate::analysis) fn get(&self, scope: ScopeId) -> Option<&LexicalScope> {
        self.0.get(scope.0)
    }

    pub(in crate::analysis) fn get_mut(&mut self, scope: ScopeId) -> Option<&mut LexicalScope> {
        self.0.get_mut(scope.0)
    }

    pub(in crate::analysis) fn program_scope(&self) -> Option<ScopeId> {
        (!self.0.is_empty()).then_some(ScopeId(0))
    }

    pub(in crate::analysis) fn ids(&self) -> impl Iterator<Item = ScopeId> + '_ {
        (0..self.0.len()).map(ScopeId)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn iter(&self) -> impl Iterator<Item = &LexicalScope> {
        self.0.iter()
    }
}

impl LexicalScope {
    pub(in crate::analysis) fn new(
        span: Span,
        depth: usize,
        kind: ScopeKind,
        parent: Option<ScopeId>,
    ) -> Self {
        Self {
            span,
            depth,
            kind,
            parent,
            bindings: HashMap::new(),
        }
    }

    pub(in crate::analysis) fn span(&self) -> Span {
        self.span
    }

    pub(in crate::analysis) fn depth(&self) -> usize {
        self.depth
    }

    pub(in crate::analysis) fn kind(&self) -> ScopeKind {
        self.kind
    }

    pub(in crate::analysis) fn parent(&self) -> Option<ScopeId> {
        self.parent
    }

    pub(in crate::analysis) fn contains(&self, span: Span) -> bool {
        span_contains(self.span, span)
    }

    pub(in crate::analysis) fn insert_binding(
        &mut self,
        name: NameId,
        provenance: BindingProvenance,
    ) {
        self.bindings.insert(name, provenance);
    }

    pub(in crate::analysis) fn update_binding(
        &mut self,
        name: NameId,
        provenance: BindingProvenance,
    ) {
        if let Some(binding) = self.bindings.get_mut(&name) {
            *binding = provenance;
        }
    }

    pub(in crate::analysis) fn binding(&self, name: NameId) -> Option<&BindingProvenance> {
        self.bindings.get(&name)
    }

    pub(in crate::analysis) fn has_binding(&self, name: NameId) -> bool {
        self.bindings.contains_key(&name)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_bindings(&self) -> bool {
        !self.bindings.is_empty()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn binding_entries(
        &self,
    ) -> impl Iterator<Item = (&NameId, &BindingProvenance)> {
        self.bindings.iter()
    }

    pub(in crate::analysis) fn binding_names(&self) -> impl Iterator<Item = &NameId> {
        self.bindings.keys()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScopeKind {
    Program,
    Function,
    Block,
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeEffect {
    DynamicEvaluation { span: Span },
}

impl ScopeEffect {
    pub fn span(&self) -> Span {
        match self {
            Self::DynamicEvaluation { span } => *span,
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct PropertyAliasFact {
    span: Span,
    scope: ScopeId,
    target: Option<SymbolPath>,
}

impl PropertyAliasFact {
    pub(in crate::analysis) fn new(span: Span, scope: ScopeId, target: Option<SymbolPath>) -> Self {
        Self {
            span,
            scope,
            target,
        }
    }

    pub(in crate::analysis) fn span(&self) -> Span {
        self.span
    }

    pub(in crate::analysis) fn scope(&self) -> ScopeId {
        self.scope
    }

    pub(in crate::analysis) fn target(&self) -> Option<&SymbolPath> {
        self.target.as_ref()
    }
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct RootedPropertyMutationFact {
    span: Span,
    scope: ScopeId,
    property: Option<NameId>,
}

impl RootedPropertyMutationFact {
    pub(in crate::analysis) fn new(span: Span, scope: ScopeId, property: Option<NameId>) -> Self {
        Self {
            span,
            scope,
            property,
        }
    }

    pub(in crate::analysis) fn span(&self) -> Span {
        self.span
    }

    pub(in crate::analysis) fn scope(&self) -> ScopeId {
        self.scope
    }

    pub(in crate::analysis) fn property(&self) -> Option<NameId> {
        self.property
    }
}

#[cfg(test)]
mod tests;
