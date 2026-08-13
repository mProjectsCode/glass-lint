use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use hashbrown::HashMap;
use smol_str::SmolStr;
use swc_common::Span;

use crate::analysis::{
    model::StaticProperties,
    syntax::{SymbolCallProvenance, SymbolMemberProvenance, constant::ConstValue},
};

// ── Identifiers ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ScopeId(usize);

impl ScopeId {
    #[cfg(test)]
    pub(in crate::analysis) const fn from_test(index: usize) -> Self {
        Self(index)
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn index_for_test(self) -> usize {
        self.0
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
pub struct BindingId(u32);

impl BindingId {
    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn from_test(raw: u32) -> Self {
        Self::new(raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingVersion(u32);

impl BindingVersion {
    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn from_test(raw: u32) -> Self {
        Self::new(raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId(u32);

impl FunctionId {
    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub(in crate::analysis) const fn raw(self) -> u32 {
        self.0
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn from_test(raw: u32) -> Self {
        Self::new(raw)
    }
}

impl From<FunctionId> for u32 {
    fn from(id: FunctionId) -> Self {
        id.raw()
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
    bindings: ScopeBindings,
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

#[derive(Debug, Clone, Default)]
struct ScopeBindings(HashMap<NameId, BindingProvenance>);

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
            bindings: ScopeBindings::default(),
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
        self.span.lo <= span.lo && self.span.hi >= span.hi
    }

    pub(in crate::analysis) fn insert_binding(
        &mut self,
        name: NameId,
        provenance: BindingProvenance,
    ) {
        self.bindings.0.insert(name, provenance);
    }

    pub(in crate::analysis) fn update_binding(
        &mut self,
        name: NameId,
        provenance: BindingProvenance,
    ) {
        if let Some(binding) = self.bindings.0.get_mut(&name) {
            *binding = provenance;
        }
    }

    pub(in crate::analysis) fn binding(&self, name: NameId) -> Option<&BindingProvenance> {
        self.bindings.0.get(&name)
    }

    pub(in crate::analysis) fn has_binding(&self, name: NameId) -> bool {
        self.bindings.0.contains_key(&name)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_bindings(&self) -> bool {
        !self.bindings.0.is_empty()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn binding_entries(
        &self,
    ) -> impl Iterator<Item = (&NameId, &BindingProvenance)> {
        self.bindings.0.iter()
    }

    pub(in crate::analysis) fn binding_names(&self) -> impl Iterator<Item = &NameId> {
        self.bindings.0.keys()
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
    StaticObjectKeys(StaticProperties<()>),
    StaticObjectValues(StaticProperties<NamePath>),
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

/// The bounded set of provenance alternatives retained for one assignment.
///
/// A precise write carries one provenance; a control-flow join retains the
/// bounded union of the reachable path alternatives and is marked joined.
/// Unknown and exhausted alternatives are never retained as provenances: they
/// are represented by the `unknown` and `exhausted` flags and cannot establish
/// a witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceAlternatives {
    provenances: Vec<BindingProvenance>,
    unknown: bool,
    joined: bool,
    exhausted: bool,
}

impl ProvenanceAlternatives {
    pub fn single(provenance: BindingProvenance) -> Self {
        Self {
            provenances: vec![provenance],
            unknown: false,
            joined: false,
            exhausted: false,
        }
    }

    /// An unknown alternative with no retained witness.
    pub fn unknown() -> Self {
        Self {
            provenances: vec![],
            unknown: true,
            joined: false,
            exhausted: false,
        }
    }

    /// Union `other` into this set, deduplicating and bounding retention to
    /// `limit`. When the bound is exceeded the set becomes both exhausted and
    /// unknown, because the retained alternatives are no longer complete and
    /// cannot establish a witness.
    fn add_bounded(&mut self, other: &Self, limit: usize) {
        self.unknown |= other.unknown;
        self.exhausted |= other.exhausted;
        self.joined |= other.joined;
        for provenance in &other.provenances {
            if !self.insert_bounded(provenance, limit) {
                return;
            }
        }
    }

    fn insert_bounded(&mut self, provenance: &BindingProvenance, limit: usize) -> bool {
        if self.provenances.contains(provenance) {
            return true;
        }
        if self.provenances.len() >= limit {
            self.exhausted = true;
            self.unknown = true;
            return false;
        }
        self.provenances.push(provenance.clone());
        true
    }

    pub fn is_joined(&self) -> bool {
        self.joined
    }

    #[cfg(test)]
    pub fn is_unknown(&self) -> bool {
        self.unknown
    }

    #[cfg(test)]
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    pub fn has_complete_witness(&self) -> bool {
        !self.provenances.is_empty()
    }

    fn is_incomplete(&self) -> bool {
        self.unknown || self.exhausted
    }

    /// The preferred strict witness at a use position: the single retained
    /// provenance for a precise write, or the first non-local alternative
    /// retained after a control-flow join. `None` when no complete witness is
    /// retained.
    pub fn preferred_witness(&self) -> Option<&BindingProvenance> {
        if self.joined {
            self.provenances
                .iter()
                .find(|p| !matches!(p, BindingProvenance::Local))
        } else {
            self.provenances.first()
        }
    }

    /// Iterate the complete (non-unknown) witnesses retained by this
    /// assignment. Unknown-only assignments iterate nothing.
    pub fn complete_witnesses(&self) -> impl Iterator<Item = &BindingProvenance> + '_ {
        self.provenances.iter()
    }
}

/// A control-flow join whose retention bound is fixed when the merge starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) struct ProvenanceJoin {
    alternatives: ProvenanceAlternatives,
    limit: usize,
}

impl ProvenanceJoin {
    pub(in crate::analysis) fn new(limit: usize) -> Self {
        Self {
            alternatives: ProvenanceAlternatives {
                provenances: Vec::new(),
                unknown: false,
                joined: true,
                exhausted: false,
            },
            limit,
        }
    }

    pub(in crate::analysis) fn add(&mut self, other: &ProvenanceAlternatives) {
        self.alternatives.add_bounded(other, self.limit);
    }

    pub(in crate::analysis) fn alternatives(&self) -> &ProvenanceAlternatives {
        &self.alternatives
    }

    fn into_alternatives(self) -> ProvenanceAlternatives {
        self.alternatives
    }
}

#[derive(Debug, Clone)]
pub struct AliasAssignment {
    span: Span,
    scope: ScopeId,
    name: NameId,
    version: BindingVersion,
    alternatives: ProvenanceAlternatives,
}

impl AliasAssignment {
    /// A precise write carrying a single provenance.
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
            alternatives: ProvenanceAlternatives::single(provenance),
        }
    }

    /// A synthetic assignment installed after a control-flow join. The
    /// `alternatives` set is the bounded union of the reachable paths.
    pub(in crate::analysis) fn joined(
        span: Span,
        scope: ScopeId,
        name: NameId,
        version: BindingVersion,
        join: ProvenanceJoin,
    ) -> Self {
        Self {
            span,
            scope,
            name,
            version,
            alternatives: join.into_alternatives(),
        }
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    pub fn name(&self) -> NameId {
        self.name
    }

    pub fn version(&self) -> BindingVersion {
        self.version
    }

    pub fn is_joined(&self) -> bool {
        self.alternatives.is_joined()
    }

    /// Whether this assignment retained an unknown or exhausted alternative.
    pub fn is_incomplete(&self) -> bool {
        self.alternatives.is_incomplete()
    }

    pub fn preferred_witness(&self) -> Option<&BindingProvenance> {
        self.alternatives.preferred_witness()
    }

    pub fn complete_witnesses(&self) -> impl Iterator<Item = &BindingProvenance> + '_ {
        self.alternatives.complete_witnesses()
    }

    #[cfg(test)]
    pub fn alternatives(&self) -> &ProvenanceAlternatives {
        &self.alternatives
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
mod tests {
    use glass_lint_datastructures::NameTable;
    use swc_common::{BytePos, Span};

    use super::*;

    #[test]
    fn binding_versions_are_part_of_identity() {
        let mut first = BindingKey::new(BindingRoot::Binding {
            function: FunctionId::from_test(1),
            binding: BindingId::from_test(2),
            version: BindingVersion::from_test(0),
        });
        let mut names = NameTable::default();
        let value = names.intern("value").unwrap();
        first.append_segment(value);
        let mut second = BindingKey::new(BindingRoot::Binding {
            function: FunctionId::from_test(1),
            binding: BindingId::from_test(2),
            version: BindingVersion::from_test(1),
        });
        second.append_segment(value);
        assert_ne!(first, second);
    }

    #[test]
    fn scope_id_index_and_from_usize() {
        let id = ScopeId::from_test(5);
        assert_eq!(id.index_for_test(), 5);
    }

    #[test]
    fn scoped_name_round_trips_scope_and_name() {
        let mut names = NameTable::default();
        let nid = names.intern("foo").unwrap();
        let sn = ScopedName::new(ScopeId::from_test(3), nid);
        assert_eq!(sn.scope(), ScopeId::from_test(3));
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
            function: FunctionId::from_test(1),
            binding: BindingId::from_test(2),
            version: BindingVersion::from_test(0),
        };
        let b = BindingRoot::Binding {
            function: FunctionId::from_test(1),
            binding: BindingId::from_test(2),
            version: BindingVersion::from_test(1),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn binding_slot_stays_constant_across_versions() {
        let mut first = BindingKey::new(BindingRoot::Binding {
            function: FunctionId::from_test(1),
            binding: BindingId::from_test(2),
            version: BindingVersion::from_test(0),
        });
        let mut second = BindingKey::new(BindingRoot::Binding {
            function: FunctionId::from_test(1),
            binding: BindingId::from_test(2),
            version: BindingVersion::from_test(1),
        });
        let mut names = NameTable::default();
        let value = names.intern("value").unwrap();
        first.append_segment(value);
        second.append_segment(value);
        assert_eq!(first.binding_slot(), second.binding_slot());
    }

    #[test]
    fn binding_slot_round_trips_construction_from_components() {
        let mut names = NameTable::default();
        let path = NamePath::from_ids([names.intern("a").unwrap()]);
        let slot = BindingSlot::new(FunctionId::from_test(1), BindingId::from_test(2), path);
        let mut key = BindingKey::new(BindingRoot::Binding {
            function: FunctionId::from_test(1),
            binding: BindingId::from_test(2),
            version: BindingVersion::from_test(0),
        });
        key.append_segment(names.intern("a").unwrap());
        assert_eq!(key.binding_slot(), Some(slot));
    }

    #[test]
    fn global_binding_keys_have_no_slot() {
        let key = BindingKey::new(BindingRoot::Global("window".into()));
        assert!(key.binding_slot().is_none());
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
        let id = FunctionId::from_test(42);
        let raw: u32 = id.into();
        assert_eq!(raw, 42);
    }

    #[test]
    fn binding_id_and_version_are_newtypes() {
        assert_ne!(BindingId::from_test(1), BindingId::from_test(2));
        assert_ne!(BindingVersion::from_test(0), BindingVersion::from_test(1));
        assert_eq!(BindingId::from_test(5), BindingId::from_test(5));
    }

    #[test]
    fn provenance_alternatives_overflow_is_exhausted_and_unknown() {
        let alias = BindingProvenance::ValueAlias {
            target: NamePath::new(),
        };
        let mut set = ProvenanceJoin::new(1);
        set.add(&ProvenanceAlternatives::single(BindingProvenance::Local));
        set.add(&ProvenanceAlternatives::single(alias));
        let set = set.alternatives();
        assert!(set.is_exhausted());
        assert!(set.is_unknown());
        assert_eq!(
            set.complete_witnesses().collect::<Vec<_>>(),
            vec![&BindingProvenance::Local]
        );
    }

    #[test]
    fn provenance_alternatives_dedup_within_the_join_bound() {
        let alias = BindingProvenance::ValueAlias {
            target: NamePath::new(),
        };
        let mut set = ProvenanceJoin::new(4);
        set.add(&ProvenanceAlternatives::single(alias.clone()));
        set.add(&ProvenanceAlternatives::single(alias.clone()));
        set.add(&ProvenanceAlternatives::single(BindingProvenance::Local));
        let set = set.alternatives();
        assert!(set.is_joined());
        assert!(!set.is_exhausted());
        assert!(!set.is_unknown());
        assert_eq!(
            set.complete_witnesses().collect::<Vec<_>>(),
            vec![&alias, &BindingProvenance::Local]
        );
    }

    #[test]
    fn provenance_alternatives_duplicate_at_bound_remains_complete() {
        let alias = BindingProvenance::ValueAlias {
            target: NamePath::new(),
        };
        let mut set = ProvenanceJoin::new(1);
        set.add(&ProvenanceAlternatives::single(alias.clone()));
        set.add(&ProvenanceAlternatives::single(alias.clone()));
        let set = set.alternatives();
        assert!(!set.is_exhausted());
        assert!(!set.is_unknown());
        assert_eq!(set.complete_witnesses().collect::<Vec<_>>(), vec![&alias]);
    }

    #[test]
    fn unknown_only_alternatives_have_no_complete_witness() {
        let unknown = ProvenanceAlternatives::unknown();
        assert!(!unknown.has_complete_witness());
        assert_eq!(unknown.complete_witnesses().count(), 0);
        assert_eq!(unknown.preferred_witness(), None);
    }

    #[test]
    fn preferred_witness_prefers_non_local_after_join() {
        let single = ProvenanceAlternatives::single(BindingProvenance::Local);
        assert_eq!(single.preferred_witness(), Some(&BindingProvenance::Local));

        let alias = BindingProvenance::ValueAlias {
            target: NamePath::new(),
        };
        let mut joined = ProvenanceJoin::new(4);
        joined.add(&ProvenanceAlternatives::single(BindingProvenance::Local));
        joined.add(&ProvenanceAlternatives::single(alias.clone()));
        let joined = joined.alternatives();
        assert_eq!(joined.preferred_witness(), Some(&alias));

        let mut local_only = ProvenanceJoin::new(4);
        local_only.add(&ProvenanceAlternatives::single(BindingProvenance::Local));
        let local_only = local_only.alternatives();
        assert_eq!(local_only.preferred_witness(), None);
    }

    #[test]
    fn alias_assignment_constructors_own_the_alternative_set() {
        let mut names = NameTable::default();
        let name = names.intern("value").unwrap();
        let scope = ScopeId::from_test(1);
        let span = Span::new(BytePos(0), BytePos(1));

        let precise = AliasAssignment::single(
            span,
            scope,
            name,
            BindingVersion::from_test(1),
            BindingProvenance::Local,
        );
        assert!(!precise.is_joined());
        assert_eq!(precise.preferred_witness(), Some(&BindingProvenance::Local));
        assert_eq!(precise.complete_witnesses().count(), 1);

        let mut exhausted = ProvenanceJoin::new(0);
        exhausted.add(&ProvenanceAlternatives::single(BindingProvenance::Local));
        assert!(exhausted.alternatives().is_exhausted());
        let joined =
            AliasAssignment::joined(span, scope, name, BindingVersion::from_test(2), exhausted);
        assert!(joined.is_joined());
        assert!(joined.alternatives().is_exhausted());
        assert_eq!(joined.complete_witnesses().count(), 0);
    }
}
