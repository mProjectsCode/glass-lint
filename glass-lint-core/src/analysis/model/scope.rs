use std::collections::BTreeMap;

use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use smol_str::SmolStr;
use swc_common::Span;

use crate::analysis::syntax::{SymbolCallProvenance, SymbolMemberProvenance, constant::ConstValue};

// ── Identifiers ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(pub usize);

impl ScopeId {
    pub fn index(self) -> usize {
        self.0
    }
}

impl From<usize> for ScopeId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

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
pub struct BindingId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingVersion(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId(pub u32);

impl From<FunctionId> for u32 {
    fn from(id: FunctionId) -> Self {
        id.0
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

    pub fn append_segment(&mut self, segment: NameId) {
        self.path.append(segment);
    }
}

// ── Scope data types ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LexicalScope {
    pub span: Span,
    pub depth: usize,
    pub kind: ScopeKind,
    pub parent: Option<ScopeId>,
    pub bindings: BTreeMap<NameId, BindingProvenance>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingProvenance {
    Local,
    ValueAlias {
        target: NamePath,
    },
    BoundCallable {
        target: NamePath,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    BoundModuleCallable {
        module: SmolStr,
        export: SmolStr,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    ReturnedObject {
        source: NamePath,
    },
    ModuleExport {
        module: SmolStr,
        export: SmolStr,
    },
    ModuleNamespace {
        module: SmolStr,
    },
    StaticString(String),
    StaticNumber(usize),
    StaticStringArray(Vec<String>),
    StaticObjectKeys(Vec<NameId>),
    StaticObjectValues(BTreeMap<NameId, NamePath>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundArgument {
    StaticString(String),
    RootedExpression(NamePath),
}

#[derive(Debug, Clone)]
pub struct IdentValueSeed {
    pub(in crate::analysis) call: SymbolCallProvenance,
    pub(in crate::analysis) rooted_chain: Option<SymbolPath>,
    pub(in crate::analysis) binding: Option<BindingKey>,
    pub(in crate::analysis) constant: ConstValue,
    pub(in crate::analysis) bound_arguments: Option<Vec<Option<BoundArgument>>>,
}

#[derive(Debug, Clone)]
pub struct MemberValueSeed {
    pub(in crate::analysis) syntactic_chain: Option<SymbolPath>,
    pub(in crate::analysis) rooted_chain: Option<NamePath>,
    pub(in crate::analysis) binding: Option<BindingKey>,
    pub(in crate::analysis) module_member: Option<SymbolMemberProvenance>,
    pub(in crate::analysis) returned_member: Option<(NamePath, NamePath)>,
}

#[derive(Debug, Clone)]
pub struct AliasAssignment {
    pub span: Span,
    pub scope: ScopeId,
    pub name: NameId,
    pub version: BindingVersion,
    pub provenance: BindingProvenance,
}

#[derive(Debug, Clone)]
pub struct PropertyAliasFact {
    pub span: Span,
    pub scope: ScopeId,
    pub target: Option<SymbolPath>,
}

#[derive(Debug, Clone)]
pub struct RootedPropertyMutationFact {
    pub span: Span,
    pub scope: ScopeId,
    pub property: Option<NameId>,
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::NameTable;

    use super::*;

    #[test]
    fn binding_versions_are_part_of_identity() {
        let mut first = BindingKey::new(BindingRoot::Binding {
            function: FunctionId(1),
            binding: BindingId(2),
            version: BindingVersion(0),
        });
        let mut names = NameTable::default();
        let value = names.intern("value").unwrap();
        first.append_segment(value);
        let mut second = BindingKey::new(BindingRoot::Binding {
            function: FunctionId(1),
            binding: BindingId(2),
            version: BindingVersion(1),
        });
        second.append_segment(value);
        assert_ne!(first, second);
    }
}
