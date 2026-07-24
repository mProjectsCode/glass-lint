//! The single authoritative semantic fact walk.
//!
//! `FactBuilder` is the only post-scope SWC visitor.  It resolves
//! identities, interns values, and emits one canonical `SemanticFact` for
//! each semantic role.  It does not receive matchers or populate evidence.

mod arguments;
mod assignments;
mod call_results;
mod calls;
mod control;
mod functions;
mod instance;
mod interface;
mod state;
mod visitor;

use std::collections::BTreeMap;

use glass_lint_datastructures::{ByteRange, NamePath, PathId, PathSegmentInput, SymbolPath};
use smol_str::SmolStr;
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
use crate::analysis::{
    facts::{
        Building, CallArgInfo, CallUnwrap, ControlKind, ControlRegionId, FactKind, FactPayload,
        FactStream, FunctionBoundary,
    },
    resolution::Resolver,
    scope::{BoundArgument, ScopeId},
    syntax::{
        SymbolCallProvenance, SymbolMemberProvenance, effective_callee_expr, member_property_name,
    },
    value::{FunctionId, ValueId},
};

/// The single authoritative semantic fact builder.
///
/// After the lexical scope prepass, this visitor walks the AST exactly once
/// and emits an immutable `FactStream` containing all semantic facts and a
/// matcher-independent module interface. The builder owns traversal state,
/// call-result tracking, and instance-level callable resolution — all of
/// which are discarded when `into_parts()` finalizes the stream.
pub struct FactBuilder<'builder, 'resolver> {
    /// Scope and provenance answers are prepared before this AST walk.
    resolver: &'builder mut Resolver<'resolver>,
    /// Facts are appended in source traversal order and never rewritten.
    stream: FactStream<Building>,
    /// Traversal-only state is kept separate from fact allocation and indexing.
    traversal: state::TraversalState,
    /// Call results are retained for effective-call and value-flow projections.
    call_results: call_results::CallResultTable,
    /// Proven callable members extracted from the current module instance.
    instance_callables: BTreeMap<ValueId, InstanceCallable>,
    /// Module requests and export slots collected during the same canonical
    /// walk as the semantic facts, owned by a focused interface builder.
    interface: interface::ModuleInterfaceBuilder,
}

impl<'builder, 'resolver> FactBuilder<'builder, 'resolver> {
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
        Self::with_limit(resolver, crate::analysis::facts::MAX_FACTS)
    }

    pub fn with_limit(resolver: &'builder mut Resolver<'resolver>, max_facts: usize) -> Self {
        Self {
            resolver,
            stream: FactStream::with_limit(max_facts),
            traversal: state::TraversalState::default(),
            call_results: call_results::CallResultTable::default(),
            instance_callables: BTreeMap::new(),
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

    fn emit(&mut self, kind: FactKind, span: Span, payload: FactPayload) {
        if self.resolver.budget.exhausted() {
            return;
        }
        #[cfg(not(test))]
        let _ = kind;
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
        let function = if self.traversal.current_function() == FunctionId(0) {
            self.resolver.function_scope_at(scope)
        } else {
            self.traversal.current_function()
        };
        let _ = self.stream.try_push(span, function, kind, payload);
    }

    fn byte_range(&mut self, span: Span) -> Option<ByteRange> {
        // TypeScript lowering deliberately synthesizes wrapper nodes with
        // DUMMY_SP. They retain semantic connectivity at a non-reportable
        // empty range; this is expected transform output, not invalid parser
        // data. Findings explicitly discard empty ranges.
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
    pub(super) fn into_stream(self) -> FactStream<crate::analysis::facts::Frozen> {
        self.stream.freeze(
            self.resolver.name_snapshot(),
            self.resolver.value_snapshot(),
        )
    }

    pub(in crate::analysis) fn into_built_facts(self) -> crate::analysis::facts::BuiltFacts {
        crate::analysis::facts::BuiltFacts {
            stream: self.stream,
            interface: self.interface.finish(),
        }
    }

    #[cfg(test)]
    pub fn into_parts(
        self,
    ) -> (
        FactStream<Building>,
        crate::analysis::module::ModuleInterface,
    ) {
        let built = self.into_built_facts();
        (built.stream, built.interface)
    }

    pub(super) fn record_local(&mut self, name: impl Into<SmolStr>) {
        self.interface.record_local(name);
    }

    pub(super) fn record_pattern_locals(&mut self, pattern: &Pat) {
        self.interface.record_pattern_locals(pattern);
    }

    // -- Module interface delegation --

    pub(super) fn record_local_imports(&mut self, import: &ImportDecl) {
        self.interface.record_local_imports(import);
    }

    pub(super) fn record_export_decl(&mut self, declaration: &swc_ecma_ast::Decl) {
        self.interface
            .record_export_decl(declaration, self.resolver);
    }

    pub(super) fn record_module_call_request(&mut self, call: &CallExpr) {
        use swc_ecma_ast::Callee;
        match &call.callee {
            Callee::Import(_) => {
                let Some(Expr::Lit(swc_ecma_ast::Lit::Str(specifier))) =
                    call.args.first().map(|a| &*a.expr)
                else {
                    return;
                };
                let Some(span) = self.byte_range(specifier.span) else {
                    return;
                };
                self.interface.record_import_request(span, specifier);
            }
            Callee::Expr(callee) => {
                let Expr::Ident(ident) = &**callee else {
                    return;
                };
                if !self
                    .resolver
                    .is_unshadowed_commonjs_name(ident, crate::analysis::module::COMMONJS_REQUIRE)
                {
                    return;
                }
                if call.args.len() != 1 {
                    return;
                }
                let Some(Expr::Lit(swc_ecma_ast::Lit::Str(specifier))) =
                    call.args.first().map(|a| &*a.expr)
                else {
                    return;
                };
                let Some(span) = self.byte_range(specifier.span) else {
                    return;
                };
                self.interface.record_require_request(span, specifier);
            }
            Callee::Super(_) => {}
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
            self.interface
                .record_reexports_from_source(export, source, span);
        } else {
            self.interface
                .record_local_named_exports_only(&export.specifiers, self.resolver);
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
/// Build the canonical fact stream used by fact-construction tests.
pub fn build_test_stream<'a>(
    program: &'a swc_ecma_ast::Program,
    resolver: &'a mut Resolver<'a>,
) -> FactStream<crate::analysis::facts::Frozen> {
    let mut builder = FactBuilder::new(resolver);
    program.visit_with(&mut builder);
    builder.into_stream()
}

#[cfg(test)]
mod tests;
