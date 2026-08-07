//! Semantic fact orchestration over one immutable stream.
//!
//! This module owns the matcher-independent boundary: scope predeclaration is
//! followed by one fact-building AST traversal that produces facts and a
//! module interface. Matcher indexes and function effects are derived together
//! from the resulting frozen stream. Matcher selection is applied only by the
//! project projection layer after that shared state has been built.

use glass_lint_datastructures::NameTable;
use hashbrown::HashMap;

#[cfg(test)]
use crate::analysis::flow::effect::FunctionEffects;
use crate::analysis::{
    matching::OccurrenceIndexes,
    module::ModuleInterface,
    module_request::{ModuleRequestKind, ModuleRequestPolicy, recognize_module_call},
    value::{ValueId, ValueTable},
};

mod arguments;
mod assignments;
mod call_results;
mod calls;
mod control;
mod functions;
mod instance;
mod interface;
mod model;
mod origin_map;
mod pattern;
mod state;
mod stream;
mod visitor;

use glass_lint_datastructures::{ByteRange, NamePath, PathId, PathSegmentInput, SymbolPath};
pub(in crate::analysis) use model::*;
pub(in crate::analysis) use origin_map::{OriginCheckpoint, OriginMap, OriginSnapshot};
use smol_str::SmolStr;
pub(in crate::analysis) use stream::FactStream;
use swc_common::{Span, Spanned};
use swc_ecma_ast::{
    ArrowExpr, AssignExpr, BinExpr, BinaryOp, CallExpr, Callee, ClassDecl, ClassExpr, CondExpr,
    DoWhileStmt, ExportAll, ExportDecl, ExportDefaultDecl, ExportDefaultExpr, Expr, ExprOrSpread,
    FnDecl, ForInStmt, ForOfStmt, ForStmt, Function, Ident, IfStmt, ImportDecl, MemberExpr,
    NamedExport, NewExpr, OptChainBase, OptChainExpr, Pat, Str, SwitchStmt, Tpl, TryStmt,
    UnaryExpr, UnaryOp, UpdateExpr, VarDeclarator, WhileStmt,
};
use swc_ecma_visit::{Visit, VisitWith};

use self::instance::InstanceCallable;
#[cfg(test)]
use crate::analysis::resolution::FrozenFactTables;
use crate::analysis::{
    SemanticBudget,
    resolution::Resolver,
    scope::{BoundArgument, ScopeId},
    syntax::{
        SymbolCallProvenance, SymbolMemberProvenance, effective_callee_expr,
        literal_member_property_name,
    },
    value::FunctionId,
};

/// The single authoritative semantic fact builder.
///
/// After the lexical scope prepass, this visitor walks the AST exactly once
/// and emits an immutable `FactStream` containing all semantic facts and a
/// matcher-independent module interface. The builder owns traversal state,
/// call-result tracking, and instance-level callable resolution — all of
/// which are discarded when `into_built_facts()` finalizes the stream.
struct FactProvenanceState {
    instance_callables: HashMap<ValueId, InstanceCallable>,
    instance_origins: OriginMap<(SmolStr, SmolStr)>,
    class_origins: OriginMap<(SmolStr, SmolStr)>,
    static_string_origins: HashMap<ValueId, ByteRange>,
}

struct ProvenanceCheckpoint {
    instance: OriginCheckpoint,
    class: OriginCheckpoint,
}

struct BranchProvenance {
    instances: OriginSnapshot<(SmolStr, SmolStr)>,
    classes: OriginSnapshot<(SmolStr, SmolStr)>,
}

impl FactProvenanceState {
    fn new() -> Self {
        Self {
            instance_callables: HashMap::new(),
            instance_origins: OriginMap::new(),
            class_origins: OriginMap::new(),
            static_string_origins: HashMap::new(),
        }
    }

    fn checkpoint(&mut self) -> ProvenanceCheckpoint {
        ProvenanceCheckpoint {
            instance: self.instance_origins.checkpoint(),
            class: self.class_origins.checkpoint(),
        }
    }

    fn restore_branch_entry(&mut self, checkpoint: &ProvenanceCheckpoint) {
        self.instance_origins.restore(&checkpoint.instance);
        self.class_origins.restore(&checkpoint.class);
    }

    fn restore_instance_alternative(&mut self, checkpoint: &ProvenanceCheckpoint) {
        self.instance_origins.restore(&checkpoint.instance);
    }

    /// Complete a control region whose instance origins can flow out of one
    /// modeled alternative, but whose class origins cannot.
    fn finish_control_region(&mut self, checkpoint: &mut ProvenanceCheckpoint) {
        self.restore_instance_alternative(checkpoint);
        self.instance_origins.commit(&mut checkpoint.instance);
        self.class_origins.rollback(&mut checkpoint.class);
    }

    fn snapshot_instances(&self, budget: &SemanticBudget) -> OriginSnapshot<(SmolStr, SmolStr)> {
        self.instance_origins.snapshot(budget)
    }

    fn snapshot_classes(&self, budget: &SemanticBudget) -> OriginSnapshot<(SmolStr, SmolStr)> {
        self.class_origins.snapshot(budget)
    }

    fn branch_provenance(&self, budget: &SemanticBudget) -> BranchProvenance {
        BranchProvenance {
            instances: self.snapshot_instances(budget),
            classes: self.snapshot_classes(budget),
        }
    }

    fn restore_instance_snapshot(&mut self, snapshot: OriginSnapshot<(SmolStr, SmolStr)>) {
        self.instance_origins.restore_from(snapshot);
    }

    fn retain_common_instance_origins(
        &mut self,
        snapshot: &OriginSnapshot<(SmolStr, SmolStr)>,
        budget: &SemanticBudget,
    ) {
        self.instance_origins.retain_common(snapshot, budget);
    }

    fn retain_common_class_origins(
        &mut self,
        snapshot: &OriginSnapshot<(SmolStr, SmolStr)>,
        budget: &SemanticBudget,
    ) {
        self.class_origins.retain_common(snapshot, budget);
    }

    fn finish_branch_with_else(
        &mut self,
        checkpoint: &mut ProvenanceCheckpoint,
        then: &BranchProvenance,
        budget: &SemanticBudget,
    ) {
        self.retain_common_instance_origins(&then.instances, budget);
        self.retain_common_class_origins(&then.classes, budget);
        self.instance_origins.commit(&mut checkpoint.instance);
        self.class_origins.commit(&mut checkpoint.class);
    }

    fn finish_branch_without_else(&mut self, checkpoint: &mut ProvenanceCheckpoint) {
        self.instance_origins.rollback(&mut checkpoint.instance);
        self.class_origins.rollback(&mut checkpoint.class);
    }

    fn seed_declaration(
        &mut self,
        targets: &[ValueId],
        callable: Option<&InstanceCallable>,
        instance_origin: Option<&(SmolStr, SmolStr)>,
        class_origin: Option<&(SmolStr, SmolStr)>,
        budget: &SemanticBudget,
    ) {
        for &target in targets {
            if let Some(callable) = callable {
                self.instance_callables.insert(target, callable.clone());
            }
            if let Some(origin) = instance_origin {
                self.instance_origins.insert(target, origin.clone(), budget);
            }
            if let Some(origin) = class_origin {
                self.class_origins.insert(target, origin.clone(), budget);
            }
        }
    }
}

pub struct FactBuilder<'builder, 'resolver> {
    /// Scope and provenance answers are prepared before this AST walk.
    resolver: &'builder mut Resolver<'resolver>,
    /// Facts are appended in source traversal order and never rewritten.
    stream: FactStream<Building>,
    /// Traversal-only state is kept separate from fact allocation and indexing.
    traversal: state::TraversalState,
    /// Call results are retained for effective-call and value-flow projections.
    call_results: call_results::CallResultTable,
    /// Provenance and instance state with checkpoint/rollback semantics.
    provenance: FactProvenanceState,
    /// Module requests and export slots collected during the same canonical
    /// walk as the semantic facts, owned by a focused interface builder.
    interface: interface::ModuleInterfaceBuilder,
}

impl<'builder, 'resolver> FactBuilder<'builder, 'resolver> {
    pub(super) fn static_string_origin(&self, value: ValueId) -> Option<ByteRange> {
        self.provenance
            .static_string_origins
            .get(&value)
            .copied()
            .or_else(|| {
                self.resolver
                    .static_string_terminal_id(value)
                    .and_then(|terminal| {
                        self.provenance
                            .static_string_origins
                            .get(&terminal)
                            .copied()
                    })
            })
    }

    pub(super) fn remember_static_string_alias(&mut self, target: ValueId, source: ValueId) {
        self.provenance.static_string_origins.remove(&target);
        if let Some(origin) = self.static_string_origin(source) {
            self.provenance.static_string_origins.insert(target, origin);
        }
    }

    pub(super) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.resolver.name_path(path)
    }

    pub(super) fn rooted_path(&self, path: Option<&SymbolPath>) -> Option<NamePath> {
        path.and_then(|path| self.name_path(&path.without_this_prefix()))
    }

    pub(super) fn returned_path(
        &self,
        paths: Option<&(SymbolPath, SymbolPath)>,
    ) -> Option<(NamePath, NamePath)> {
        paths.and_then(|(source, member)| Some((self.name_path(source)?, self.name_path(member)?)))
    }

    #[cfg(test)]
    pub(super) fn new(resolver: &'builder mut Resolver<'resolver>) -> Self {
        Self::with_limit(resolver, MAX_FACTS)
    }

    pub fn with_limit(resolver: &'builder mut Resolver<'resolver>, max_facts: usize) -> Self {
        Self {
            resolver,
            stream: FactStream::with_limit(max_facts),
            traversal: state::TraversalState::default(),
            call_results: call_results::CallResultTable::default(),
            provenance: FactProvenanceState::new(),
            interface: interface::ModuleInterfaceBuilder::new(),
        }
    }

    fn scope_at(&self, span: Span) -> ScopeId {
        self.resolver.scope_at(span)
    }

    fn append_path(&mut self, parent: PathId, segment: PathSegmentInput<'_>) -> PathId {
        self.resolver.budget.try_charge();
        if self.resolver.budget.exhausted() {
            return PathId::EMPTY;
        }
        let segment = match segment {
            PathSegmentInput::Property(name) => self
                .intern_name(Some(name))
                .map(PathSegmentInput::PropertyId),
            other => Some(other),
        };
        let Some(segment) = segment else {
            return PathId::EMPTY;
        };
        self.stream
            .intern_path_input(parent, segment)
            .unwrap_or_else(|| {
                self.stream.mark_path_exhausted();
                PathId::EMPTY
            })
    }

    fn intern_name(&mut self, name: Option<&str>) -> Option<glass_lint_datastructures::NameId> {
        name.and_then(|name| {
            self.resolver.budget.try_charge();
            if let Ok(id) = self.resolver.intern_name(name) {
                Some(id)
            } else {
                self.stream.mark_name_exhausted();
                None
            }
        })
    }

    fn emit(&mut self, span: Span, payload: FactPayload) {
        if self.resolver.budget.exhausted() {
            return;
        }
        let scope = self.scope_at(span);
        let normalized_span = if span.is_dummy() {
            match &payload {
                FactPayload::Call { callee_span, .. }
                | FactPayload::Construction { callee_span, .. } => Some(*callee_span),
                _ => None,
            }
        } else {
            self.byte_range(span)
        };
        let Some(span) = normalized_span else {
            return;
        };
        self.resolver.budget.try_charge();
        let function = if self.traversal.current_function() == FunctionId::new(0) {
            self.resolver.function_scope_at(scope)
        } else {
            self.traversal.current_function()
        };
        let _ = self.stream.try_push(span, function, payload);
    }

    fn byte_range(&mut self, span: Span) -> Option<ByteRange> {
        if span.is_dummy() {
            return Some(ByteRange::empty());
        }
        if let Ok(range) = self.resolver.normalize_span(span) {
            Some(range)
        } else {
            self.stream.mark_invalid_parser_span();
            None
        }
    }

    #[cfg(test)]
    pub(super) fn into_stream(self) -> FactStream<Frozen> {
        let tables = FrozenFactTables::for_test(
            self.resolver.name_snapshot(),
            self.resolver.value_snapshot(),
        );
        self.stream.freeze(tables)
    }

    pub(in crate::analysis) fn into_built_facts(self) -> BuiltFacts {
        BuiltFacts {
            stream: self.stream,
            interface: self.interface.finish(),
        }
    }

    pub(super) fn record_local(&mut self, name: impl Into<SmolStr>) {
        self.interface.record_local(name);
    }

    pub(super) fn record_pattern_locals(&mut self, pattern: &Pat) {
        self.interface.record_pattern_locals(pattern);
    }

    pub(super) fn record_local_imports(&mut self, import: &ImportDecl) {
        self.interface.record_local_imports(import);
    }

    pub(super) fn record_export_decl(&mut self, declaration: &swc_ecma_ast::Decl) {
        self.interface
            .record_export_decl(declaration, self.resolver);
    }

    pub(super) fn record_module_call_request(
        &mut self,
        call: &CallExpr,
    ) -> Option<(String, swc_common::Span)> {
        let request = recognize_module_call(call, self.resolver, ModuleRequestPolicy::interface())?;
        let span = self.byte_range(request.specifier_span())?;
        match request.kind() {
            ModuleRequestKind::DynamicImport => {
                let module = request.module().to_owned();
                self.interface.record_import_request(span, &module);
                Some((module, request.specifier_span()))
            }
            ModuleRequestKind::Require => {
                self.interface
                    .record_require_request(span, request.module());
                None
            }
            ModuleRequestKind::WrappedRequire => None,
        }
    }

    pub(super) fn record_named_export(&mut self, export: &NamedExport) {
        if export.type_only {
            return;
        }
        if let Some(source) = export.src.as_ref() {
            let Some(span) = self.byte_range(source.span) else {
                return;
            };
            self.interface.record_reexports(export, source, span);
        } else {
            self.interface
                .record_local_named_exports(&export.specifiers, self.resolver);
        }
    }

    pub(super) fn record_export_all(&mut self, export: &ExportAll) {
        let Some(span) = self.byte_range(export.src.span) else {
            return;
        };
        self.interface.record_export_all(export, span);
    }

    pub(super) fn record_default_expr(&mut self, export: &ExportDefaultExpr) {
        self.interface.record_default_expr(export, self.resolver);
    }

    pub(super) fn record_default_decl(&mut self, export: &ExportDefaultDecl) {
        self.interface.record_default_decl(export, self.resolver);
    }

    pub(super) fn record_commonjs_export(&mut self, assignment: &swc_ecma_ast::AssignExpr) {
        self.interface
            .record_commonjs_export(assignment, self.resolver);
    }
}

#[cfg(test)]
pub fn build_test_stream<'a>(
    program: &'a swc_ecma_ast::Program,
    resolver: &'a mut Resolver<'a>,
) -> FactStream<Frozen> {
    let mut builder = FactBuilder::new(resolver);
    program.visit_with(&mut builder);
    builder.into_stream()
}

#[cfg(test)]
pub fn build_test_facts(source: &str, filename: &str) -> FactStream<Frozen> {
    let parsed = crate::parse_test_source(source, filename).expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    build_test_stream(&parsed.program, &mut resolver)
}

#[cfg(test)]
mod tests;

/// Facts collected by the source-order visitor before names and values are
/// frozen into the immutable semantic artifact.
pub(in crate::analysis) struct BuiltFacts {
    pub(in crate::analysis) stream: FactStream<Building>,
    pub(in crate::analysis) interface: ModuleInterface,
}

// ── SemanticFacts ───────────────────────────────────────────────────────

#[derive(Debug)]
/// Immutable per-file semantic state shared by all selected matchers.
///
/// A malformed or budget-exhausted stream remains available for diagnostics,
/// but indexing and projection fail closed rather than consuming partial facts.
pub(in crate::analysis) struct SemanticFacts {
    stream: FactStream<Frozen>,
    index: OccurrenceIndexes,
    interface: ModuleInterface,
}

impl SemanticFacts {
    /// Assemble immutable indexes from the stream produced by lowering.
    pub(in crate::analysis) fn from_lowering(
        stream: FactStream<Frozen>,
        interface: ModuleInterface,
        environment: &crate::Environment,
    ) -> Self {
        let index = Self::build_index(&stream, environment);
        Self {
            stream,
            index,
            interface,
        }
    }

    fn build_index(
        stream: &FactStream<Frozen>,
        environment: &crate::Environment,
    ) -> OccurrenceIndexes {
        let mut index = OccurrenceIndexes::with_environment(environment);
        if stream.is_valid() {
            index.build_from_stream(stream);
            index.normalize_occurrences();
        }
        index
    }

    /// Borrow the canonical facts in deterministic source traversal order.
    pub(in crate::analysis) fn stream(&self) -> &FactStream<Frozen> {
        &self.stream
    }

    pub(in crate::analysis) fn names(&self) -> &NameTable {
        self.stream.names()
    }

    pub(in crate::analysis) fn matcher_index(&self) -> &OccurrenceIndexes {
        &self.index
    }

    /// Borrow the frozen value arena for shape lookups by ValueId.
    pub(in crate::analysis) fn values(&self) -> &ValueTable {
        self.stream.values()
    }

    /// Borrow the module requests and export facts collected during the walk.
    pub(in crate::analysis) fn interface(&self) -> &ModuleInterface {
        &self.interface
    }
}

#[cfg(test)]
#[allow(clippy::derivable_impls)]
impl Default for SemanticFacts {
    fn default() -> Self {
        Self {
            stream: FactStream::default(),
            index: OccurrenceIndexes::default(),
            interface: ModuleInterface::default(),
        }
    }
}

#[cfg(test)]
mod stream_tests {
    use glass_lint_datastructures::ByteRange;

    use super::*;
    use crate::{
        analysis::{
            lowering::ResolvedProgram, resolution::Resolver, syntax::SymbolCallProvenance,
            value::FunctionId,
        },
        api::{compiler::rule::CompiledMatcherPlan, rule::EventQuery},
    };

    fn test_call(id: u32, span: ByteRange) -> SemanticFact {
        SemanticFact::new(
            FactId::from_test(id),
            span,
            FunctionId::from_test(0),
            FactPayload::Call {
                callee: ValueId::UNKNOWN,
                receiver: None,
                result: ValueId::UNKNOWN,
                callee_span: span,
                callee_name: None,
                call_provenance: SymbolCallProvenance::Local,
                syntactic_path: None,
                rooted_chain: None,
                module_member: None,
                returned_member: None,
                instance_class: None,
                target_function: None,
                args: Vec::new(),
                unwrap: None,
            },
        )
    }

    fn test_member_read(id: u32, span: ByteRange) -> SemanticFact {
        SemanticFact::new(
            FactId::from_test(id),
            span,
            FunctionId::from_test(0),
            FactPayload::MemberRead {
                syntactic_path: None,
                rooted_chain: None,
                module_member: None,
                returned_member: None,
            },
        )
    }

    #[test]
    fn direct_lookup_and_linear_test_helper_preserve_fact_order() {
        let span = ByteRange::new(10, 20).unwrap();
        let mut stream = FactStream::<Building>::new();
        stream.push(test_call(0, span));
        stream.push(test_member_read(1, span));
        stream.push(test_call(2, span));

        assert_eq!(
            stream
                .facts()
                .iter()
                .filter(|fact| {
                    fact.span.start() == span.start()
                        && fact.span.end() == span.end()
                        && matches!(fact.payload, FactPayload::Call { .. })
                })
                .map(SemanticFact::id)
                .collect::<Vec<_>>(),
            vec![FactId::from_test(0), FactId::from_test(2)]
        );
        assert!(
            stream
                .fact(FactId::from_test(0))
                .is_some_and(|fact| { matches!(fact.payload, FactPayload::Call { .. }) })
        );
        assert!(
            stream
                .fact(FactId::from_test(2))
                .is_some_and(|fact| { matches!(fact.payload, FactPayload::Call { .. }) })
        );
        assert!(stream.fact(FactId::from_test(3)).is_none());
    }

    #[test]
    fn dense_fact_stream_preserves_every_same_span_fact() {
        let span = ByteRange::new(100, 120).unwrap();
        let mut stream = FactStream::<Building>::new();
        for id in 0..10_001 {
            stream.push(test_call(id, span));
        }
        let calls = stream
            .facts()
            .iter()
            .filter(|fact| {
                fact.span.start() == span.start()
                    && fact.span.end() == span.end()
                    && matches!(fact.payload, FactPayload::Call { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 10_001);
        assert_eq!(
            calls.first().map(|fact| fact.id()),
            Some(FactId::from_test(0))
        );
        assert_eq!(
            calls.last().map(|fact| fact.id()),
            Some(FactId::from_test(10_000))
        );
        assert_eq!(
            stream.fact(FactId::from_test(10_000)).map(SemanticFact::id),
            Some(FactId::from_test(10_000))
        );
    }

    #[test]
    fn fact_ids_have_checked_collection_boundaries() {
        assert_eq!(FactId::from_index(0), Some(FactId::from_test(0)));
        assert_eq!(
            FactId::from_index(MAX_FACTS - 1),
            Some(FactId::from_test(
                u32::try_from(MAX_FACTS - 1).expect("fact limit fits in FactId")
            ))
        );
        assert_eq!(FactId::from_index(MAX_FACTS), None);
        assert_eq!(FactId::from_test(u32::MAX).index(), None);
    }

    #[test]
    fn catalog_selection_and_order_cannot_change_fact_index() {
        let source = "fetch('/api'); document.createElement('script');";
        let parsed = crate::parse_test_source(source, "catalog-fingerprint.js")
            .expect("source should parse");
        let first =
            CompiledMatcherPlan::compile(&[EventQuery::call_global("fetch").unwrap().into_query()])
                .unwrap();
        let second = CompiledMatcherPlan::compile(&[EventQuery::member_call_heuristic(
            "document.createElement",
        )
        .unwrap()
        .into_query()])
        .unwrap();
        let build = |matchers: Vec<&crate::api::compiler::rule::CompiledMatcherPlan>,
                     selected: &[usize]| {
            let _ = (matchers, selected);
            let resolved = ResolvedProgram::collect_for_test(&parsed.program, source);
            let artifact = resolved.freeze(
                &crate::Environment::default(),
                &crate::AnalysisLimits::default(),
                parsed.program.span(),
            );
            format!("{:?}", artifact.facts().matcher_index())
        };

        let forward = build(vec![&first, &second], &[0, 1]);
        assert_eq!(forward, build(vec![&first, &second], &[0]));
        assert_eq!(forward, build(vec![&first, &second], &[1, 0]));
        assert_eq!(forward, build(vec![&first, &second], &[]));
        assert_eq!(forward, build(vec![&second, &first], &[0, 1]));
    }

    #[test]
    fn lowering_shared_derived_pass_matches_standalone_effect_collection() {
        let source = "function helper(value) { return value; } helper('/api');";
        let parsed = crate::parse_test_source(source, "shared-derived-pass.js")
            .expect("source should parse");
        let limits = crate::AnalysisLimits::default()
            .with_effect_operations(usize::MAX)
            .expect("valid effect limit");
        let resolved = ResolvedProgram::collect_for_test(&parsed.program, source);
        let artifact = resolved.freeze(
            &crate::Environment::default(),
            &limits,
            parsed.program.span(),
        );
        let combined_effects = artifact.effects();
        let standalone_effects = FunctionEffects::collect(artifact.facts().stream(), usize::MAX);

        let summarize = |effects: &FunctionEffects| {
            effects
                .iter_effects()
                .map(|effect| {
                    (
                        effect.id(),
                        effect.calls().len(),
                        effect.uses().len(),
                        effect.returns().len(),
                        effect.is_invalid(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            summarize(combined_effects),
            summarize(&standalone_effects),
            "sharing the fact-tape pass must preserve function effects"
        );
        assert_eq!(
            combined_effects.operation_count(),
            standalone_effects.operation_count()
        );
        assert_eq!(
            combined_effects.budget_exhausted(),
            standalone_effects.budget_exhausted()
        );
        assert!(
            !artifact.facts().matcher_index().is_empty(),
            "the same pass must still populate occurrence indexes"
        );
    }

    /// Verify that the fact-driven index populates expected occurrence maps
    /// for a diverse program.
    #[test]
    fn fact_driven_index_populates_expected_maps() {
        let src = r#"
            import { foo } from 'mod';
            import { Bar } from 'other-mod';
            class MyApp extends Bar {}
            const x = foo;
            function greet(name) { return name; }
            greet("hello");
            x.hello();
            new Bar();
            const s = "world";
            require('path');
            const a = [1, 2];
            a.push(3);
        "#;
        let parsed = crate::parse_test_source(src, "char-index.js").expect("source should parse");
        let mut resolver = Resolver::collect(&parsed.program, src);

        let mut builder = FactBuilder::new(&mut resolver);
        swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
        let stream = builder.into_stream();
        let mut index = OccurrenceIndexes::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        assert!(index.has_import("mod"), "should have 'mod' import");
        assert!(
            index.has_import("other-mod"),
            "should have 'other-mod' import"
        );
        assert!(
            index.has_import("path"),
            "should have 'path' require import"
        );
        assert!(index.has_call("greet"), "should have greet call");
        assert!(
            index.has_string("world"),
            "should have 'world' string literal"
        );
        assert!(index.has_any_class(), "should have class entries");
        assert!(
            index.has_module_class("other-mod", "Bar"),
            "should have module class for Bar from other-mod"
        );
        assert!(
            index.has_module_constructor("other-mod", "Bar"),
            "should have module constructor entries"
        );
        assert!(index.has_any_member_call(), "should have member calls");
    }

    /// Verify that .call()/.apply() unwrapping produces the expected
    /// member call entries for the target.
    #[test]
    fn call_apply_unwrapping_populates_indexes() {
        let src = r"
            function fetch(url) { return url; }
            fetch.call(null, '/api');
            fetch.apply(null, ['/api']);
        ";
        let parsed = crate::parse_test_source(src, "unwrap.js").expect("source should parse");
        let mut resolver = Resolver::collect(&parsed.program, src);

        let mut builder = FactBuilder::new(&mut resolver);
        swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
        let stream = builder.into_stream();
        let mut index = OccurrenceIndexes::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        // The unwrap should record 'fetch' as a member call.
        assert!(
            index.has_member_call("fetch"),
            "should have 'fetch' as member call from unwrapping"
        );
    }
}
