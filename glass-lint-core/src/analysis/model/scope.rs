use std::collections::BTreeMap;

use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use hashbrown::HashMap;
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

    pub fn binding_slot(&self) -> Option<(FunctionId, BindingId, NamePath)> {
        match self.root {
            BindingRoot::Binding {
                function, binding, ..
            } => Some((function, binding, self.path.clone())),
            BindingRoot::Global(_) => None,
        }
    }
}

// ── Scope data types ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LexicalScope {
    pub span: Span,
    pub depth: usize,
    pub kind: ScopeKind,
    pub parent: Option<ScopeId>,
    pub bindings: HashMap<NameId, BindingProvenance>,
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
    /// A default ESM import is callable as the module's `default` export and
    /// also acts as a namespace-like object for member access.
    DefaultImport {
        module: SmolStr,
    },
    ModuleNamespace {
        module: SmolStr,
    },
    ConstructedInstance {
        module: SmolStr,
        export: SmolStr,
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
    /// One or more provenances. Multiple alternatives arise from control-flow
    /// joins where different paths disagree. An empty vec means unknown.
    pub alternatives: Vec<BindingProvenance>,
    /// Whether this assignment represents an unknown alternative in addition
    /// to the retained provenances.
    pub unknown: bool,
    /// Whether this is the synthetic assignment installed after a control-flow
    /// join. A write in a branch is precise within that branch; only the
    /// synthetic post-join value carries multiple path alternatives.
    pub joined: bool,
}

impl AliasAssignment {
    pub fn single(
        span: Span,
        scope: ScopeId,
        name: NameId,
        version: BindingVersion,
        provenance: BindingProvenance,
    ) -> Self {
        Self {
            span,
            scope,
            name,
            version,
            alternatives: vec![provenance],
            unknown: false,
            joined: false,
        }
    }
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
    use swc_common::{BytePos, Span};

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

    #[test]
    fn scope_id_index_and_from_usize() {
        let id: ScopeId = 5usize.into();
        assert_eq!(id.index(), 5);
    }

    #[test]
    fn scoped_name_round_trips_scope_and_name() {
        let mut names = NameTable::default();
        let nid = names.intern("foo").unwrap();
        let sn = ScopedName::new(ScopeId(3), nid);
        assert_eq!(sn.scope(), ScopeId(3));
        assert_eq!(sn.name(), nid);
    }

    #[test]
    fn binding_root_global_variant() {
        let a = BindingRoot::Global("window".into());
        let b = BindingRoot::Global("window".into());
        let c = BindingRoot::Global("document".into());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn binding_root_binding_variants_differ_on_version() {
        let a = BindingRoot::Binding {
            function: FunctionId(1),
            binding: BindingId(2),
            version: BindingVersion(0),
        };
        let b = BindingRoot::Binding {
            function: FunctionId(1),
            binding: BindingId(2),
            version: BindingVersion(1),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn binding_key_new_creates_empty_path() {
        let key = BindingKey::new(BindingRoot::Global("g".into()));
        assert_eq!(
            key,
            BindingKey {
                root: BindingRoot::Global("g".into()),
                path: NamePath::new(),
            }
        );
    }

    #[test]
    fn scope_kind_variants_are_distinct() {
        assert_ne!(ScopeKind::Program, ScopeKind::Function);
        assert_ne!(ScopeKind::Function, ScopeKind::Block);
        assert_ne!(ScopeKind::Block, ScopeKind::Dynamic);
        assert_eq!(ScopeKind::Program, ScopeKind::Program);
    }

    #[test]
    fn scope_effect_dynamic_evaluation_span() {
        let span = Span::new(BytePos(10), BytePos(20));
        let effect = ScopeEffect::DynamicEvaluation { span };
        assert_eq!(effect.span(), span);
    }

    #[test]
    fn binding_provenance_variants() {
        let local = BindingProvenance::Local;
        let alias = BindingProvenance::ValueAlias {
            target: NamePath::new(),
        };
        let bound_callable = BindingProvenance::BoundCallable {
            target: NamePath::new(),
            bound_arguments: vec![Some(BoundArgument::StaticString("x".into()))],
        };
        let module_ns = BindingProvenance::ModuleNamespace {
            module: "pkg".into(),
        };
        let static_string = BindingProvenance::StaticString("hello".into());
        assert_eq!(local, BindingProvenance::Local);
        assert_ne!(local, alias);
        assert_ne!(alias, bound_callable);
        assert_ne!(bound_callable, module_ns);
        assert_ne!(module_ns, static_string);
    }

    #[test]
    fn bound_argument_static_string_and_rooted_expression() {
        let s = BoundArgument::StaticString("exact".into());
        let r = BoundArgument::RootedExpression(NamePath::new());
        assert_ne!(s, r);
        assert_eq!(s, BoundArgument::StaticString("exact".into()));
    }

    #[test]
    fn function_id_converts_to_u32() {
        let id = FunctionId(42);
        let raw: u32 = id.into();
        assert_eq!(raw, 42);
    }

    #[test]
    fn binding_id_and_version_are_newtypes() {
        assert_ne!(BindingId(1), BindingId(2));
        assert_ne!(BindingVersion(0), BindingVersion(1));
        assert_eq!(BindingId(5), BindingId(5));
    }
}
