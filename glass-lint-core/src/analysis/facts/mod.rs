//! Semantic fact orchestration over one immutable stream.
//!
//! This module owns the matcher-independent boundary: scope predeclaration is
//! followed by one fact-building AST traversal that produces facts and a
//! module interface. Matcher indexes and function effects are derived together
//! from the resulting frozen stream. Matcher selection is applied only by the
//! project projection layer after that shared state has been built.

use glass_lint_datastructures::NameTable;

#[cfg(test)]
use crate::analysis::flow::effect::FunctionEffects;
use crate::analysis::{
    DerivedPhaseAvailability, DerivedPhaseCapabilities,
    matching::OccurrenceIndexes,
    model::{
        module::{ImportedBinding, ModuleInterface},
        value::{ValueId, ValueTable},
    },
    module_request::{ModuleRequestPolicy, recognize_module_call},
};

mod arguments;
mod assignments;
mod call_results;
mod calls;
mod construction;
mod control;
mod functions;
mod instance;
mod interface;
mod origin_map;
mod pattern;
mod provenance;
mod reads;
mod state;
pub(in crate::analysis) mod stream;
mod visitor;

pub(in crate::analysis) use calls::ResolvedCallee;
pub(in crate::analysis::facts) use calls::call_apply_wrapper;
use glass_lint_datastructures::{ByteRange, NamePath, PathId, PathSegmentInput, SymbolPath};
pub(in crate::analysis) use origin_map::{OriginCheckpoint, OriginMap, OriginSnapshot};
use provenance::{OriginChannels, TargetProvenance};
use smol_str::{SmolStr, ToSmolStr};
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
use crate::analysis::facts::stream::FrozenStorage;
pub(in crate::analysis) use crate::analysis::model::fact::{
    ArgumentView, Building, CallArgInfo, CallUnwrap, ClassFactRole, ControlKind, ControlRegionId,
    FactId, FactPayload, Frozen, FunctionBoundary, MAX_FACTS, ParameterBinding, SemanticFact,
};
#[cfg(test)]
use crate::analysis::semantic::with_test_collection;
use crate::analysis::{
    model::scope::FunctionId,
    resolution::Resolver,
    scope::{BoundArgument, ScopeId},
    syntax::{
        SymbolCallProvenance, SymbolMemberProvenance, effective_callee_expr,
        literal_member_property_name,
    },
};

/// The single authoritative semantic fact builder.
///
/// After the lexical scope prepass, this visitor walks the AST exactly once
/// and emits an immutable `FactStream` containing all semantic facts and a
/// matcher-independent module interface. The builder owns traversal state,
/// call-result tracking, and instance-level callable resolution — all of
/// which are discarded when `into_built_facts()` finalizes the stream.
pub(in crate::analysis) struct FactBuilder<'builder, 'resolver> {
    /// Scope and provenance answers are prepared before this AST walk.
    resolver: &'builder mut Resolver<'resolver>,
    /// Facts are appended in source traversal order and never rewritten.
    stream: FactStream<Building>,
    /// Traversal-only state is kept separate from fact allocation and indexing.
    traversal: state::TraversalState,
    /// Call results are retained for effective-call and value-flow projections.
    call_results: call_results::CallResultTable,
    /// Provenance and instance state with checkpoint/rollback semantics.
    provenance: OriginChannels,
    /// Module requests and export slots collected during the same canonical
    /// walk as the semantic facts.
    interface: ModuleInterface,
}

impl<'builder, 'resolver> FactBuilder<'builder, 'resolver> {
    pub(super) fn static_string_origin(&self, value: ValueId) -> Option<ByteRange> {
        self.provenance.static_string_origin(value).or_else(|| {
            self.resolver
                .static_string_terminal_id(value)
                .and_then(|terminal| self.provenance.static_string_origin(terminal))
        })
    }

    fn target_provenance(&mut self, expression: &Expr, source: ValueId) -> TargetProvenance {
        TargetProvenance {
            callable: self.instance_callable_for_expr(expression),
            instance_origin: self.instance_origin_for_expr(expression),
            class_origin: self.constructor_origin_for_expr(expression),
            static_string_origin: self.static_string_origin(source),
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

    fn with_limit(resolver: &'builder mut Resolver<'resolver>, max_facts: usize) -> Self {
        Self {
            resolver,
            stream: FactStream::with_limit(max_facts),
            traversal: state::TraversalState::default(),
            call_results: call_results::CallResultTable::default(),
            provenance: OriginChannels::new(),
            interface: ModuleInterface::default(),
        }
    }

    fn scope_at(&self, span: Span) -> Option<ScopeId> {
        self.resolver.scope_at(span)
    }

    fn append_path(&mut self, parent: PathId, segment: PathSegmentInput<'_>) -> PathId {
        self.resolver.budget().try_charge();
        if self.resolver.budget().exhausted() {
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
        name.and_then(|name| self.resolver.intern_name(name))
    }

    fn emit(&mut self, span: Span, payload: FactPayload) {
        if self.resolver.budget().exhausted() {
            return;
        }
        let Some(scope) = self.scope_at(span) else {
            return;
        };
        let normalized_span = if span.is_dummy() {
            match &payload {
                FactPayload::Call(call) => Some(call.callee_span()),
                FactPayload::Construction { callee_span, .. } => Some(*callee_span),
                _ => None,
            }
        } else {
            self.byte_range(span)
        };
        let Some(span) = normalized_span else {
            return;
        };
        self.resolver.budget().try_charge();
        let function = if self.traversal.current_function() == FunctionId::new(0) {
            self.resolver.function_scope_at(scope)
        } else {
            self.traversal.current_function()
        };
        self.stream.append(span, function, payload);
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
        let storage = FrozenStorage::from_tables(
            self.resolver.name_snapshot(),
            self.resolver.value_snapshot(),
        );
        self.stream.freeze(storage)
    }

    pub(in crate::analysis) fn into_built_facts(self) -> BuiltFacts {
        BuiltFacts {
            stream: self.stream,
            interface: self.interface,
        }
    }

    pub(super) fn record_local(&mut self, name: impl Into<SmolStr>) {
        self.interface.add_local(name);
    }

    pub(super) fn record_pattern_locals(&mut self, pattern: &Pat) {
        self.interface.record_pattern_locals(pattern);
    }

    pub(super) fn record_local_imports(&mut self, import: &ImportDecl) {
        self.interface.record_local_imports(import);
    }

    pub(super) fn record_static_import(&mut self, import: &ImportDecl) {
        if import.type_only {
            return;
        }
        let module = import.src.value.to_string_lossy().to_string();
        let bindings = import
            .specifiers
            .iter()
            .filter(|specifier| !specifier.is_type_only())
            .map(|specifier| match specifier {
                swc_ecma_ast::ImportSpecifier::Named(named) => {
                    ImportedBinding::named(named.imported.as_ref().map_or_else(
                        || named.local.sym.to_smolstr(),
                        |name| crate::analysis::syntax::module_export_name(name).to_smolstr(),
                    ))
                }
                swc_ecma_ast::ImportSpecifier::Default(_) => ImportedBinding::named("default"),
                swc_ecma_ast::ImportSpecifier::Namespace(_) => ImportedBinding::namespace(),
            })
            .collect();
        self.record_local_imports(import);
        let Some(span) = self.byte_range(import.src.span) else {
            return;
        };
        self.interface
            .add_import_request(span, module.clone(), bindings);
        self.emit(import.src.span, FactPayload::Import { module });
    }

    pub(super) fn record_export_decl(&mut self, declaration: &swc_ecma_ast::Decl) {
        self.interface
            .record_export_decl(declaration, self.resolver);
    }

    pub(super) fn observe_module_call(&mut self, call: &CallExpr) -> Option<String> {
        let request = recognize_module_call(call, self.resolver, ModuleRequestPolicy::interface())?;
        let span = self.byte_range(request.specifier_span())?;
        self.interface.record_module_request(span, &request)
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

/// Build the matcher-independent facts and module interface for one AST.
pub(in crate::analysis) fn build(
    program: &swc_ecma_ast::Program,
    resolver: &mut Resolver<'_>,
    max_facts: usize,
) -> BuiltFacts {
    let mut builder = FactBuilder::with_limit(resolver, max_facts);
    program.visit_with(&mut builder);
    builder.into_built_facts()
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
    let budget = crate::analysis::SemanticBudget::default();
    let mut resolver = Resolver::collect(&parsed.program, source, &budget);
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
    /// Assemble immutable indexes from the stream produced by semantic
    /// analysis.
    pub(in crate::analysis) fn from_analysis(
        stream: FactStream<Frozen>,
        interface: ModuleInterface,
        environment: &crate::Environment,
        capabilities: DerivedPhaseCapabilities,
    ) -> Self {
        let index = Self::build_index(&stream, environment, capabilities.availability());
        Self {
            stream,
            index,
            interface,
        }
    }

    fn build_index(
        stream: &FactStream<Frozen>,
        environment: &crate::Environment,
        availability: DerivedPhaseAvailability,
    ) -> OccurrenceIndexes {
        OccurrenceIndexes::from_stream(stream, environment, availability)
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

    /// Whether this artifact has the complete local state required by
    /// project-level matching and flow projection.
    pub(in crate::analysis) fn is_projectable(&self) -> bool {
        self.stream.is_valid()
            && !self.stream.name_exhausted()
            && self.values().get(ValueId::UNKNOWN).is_some()
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
mod stream_tests;
