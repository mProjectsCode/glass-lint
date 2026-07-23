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
mod interface;
mod state;
mod visitor;

use std::collections::BTreeMap;

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

use crate::{
    ByteRange,
    analysis::{
        SymbolPath,
        facts::{
            Building, CallArgInfo, CallUnwrap, ControlKind, ControlRegionId, FactKind, FactPayload,
            FactStream, FunctionBoundary, ParameterBinding,
        },
        resolution::Resolver,
        scope::{BoundArgument, ScopeId},
        syntax::{
            SymbolCallProvenance, SymbolMemberProvenance, effective_callee_expr,
            member_property_name,
        },
        value::{NamePath, PathId, PathSegmentInput, ValueId},
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
/// A callable member extracted from a proven module-backed instance.
pub(super) struct InstanceCallable {
    module: SmolStr,
    export: SmolStr,
    member: SymbolPath,
}

impl InstanceCallable {
    pub(super) fn new(
        module: impl Into<SmolStr>,
        export: impl Into<SmolStr>,
        member: SymbolPath,
    ) -> Self {
        Self {
            module: module.into(),
            export: export.into(),
            member,
        }
    }

    pub(super) fn class_identity(&self) -> (SmolStr, SmolStr) {
        (self.module.clone(), self.export.clone())
    }

    pub(super) fn member(&self) -> &SymbolPath {
        &self.member
    }
}

/// The single authoritative semantic fact builder.
///
/// After the lexical scope prepass, this visitor walks the AST exactly once
/// and emits an immutable `FactStream` containing all semantic facts and a
/// matcher-independent module interface. The builder owns traversal state,
/// call-result tracking, and instance-level callable resolution — all of
/// which are discarded when `into_parts()` finalizes the stream.
pub struct FactBuilder<'a> {
    /// Scope and provenance answers are prepared before this AST walk.
    resolver: &'a mut Resolver,
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

impl<'a> FactBuilder<'a> {
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
    pub(super) fn new(resolver: &'a mut Resolver) -> Self {
        Self::with_limit(resolver, crate::analysis::facts::MAX_FACTS)
    }

    pub fn with_limit(resolver: &'a mut Resolver, max_facts: usize) -> Self {
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

    fn intern_name(&mut self, name: Option<&str>) -> Option<crate::analysis::name::NameId> {
        name.and_then(|name| {
            if let Ok(id) = self.resolver.intern_name(name) {
                Some(id)
            } else {
                self.stream.mark_name_exhausted();
                None
            }
        })
    }

    fn emit(&mut self, kind: FactKind, span: Span, payload: FactPayload) {
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
        let _ = self
            .stream
            .try_push(span, self.resolver.function_scope_at(scope), kind, payload);
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
    resolver: &'a mut Resolver,
) -> FactStream<crate::analysis::facts::Frozen> {
    let mut builder = FactBuilder::new(resolver);
    program.visit_with(&mut builder);
    builder.into_stream()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_facts(src: &str, filename: &str) -> FactStream<crate::analysis::facts::Frozen> {
        let parsed = crate::parse(src, filename).expect("source should parse");
        let mut resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&mut resolver);
        parsed.program.visit_with(&mut builder);
        builder.into_stream()
    }

    #[test]
    fn fact_builder_emits_facts_for_diverse_program() {
        let src = r#"
            const x = 1;
            function foo(a) {
                const y = a + x;
                return y;
            }
            foo(2);
            const obj = { prop: 3 };
            obj.prop = 4;
            new Error("fail");
        "#;
        let stream = build_facts(src, "fact-builder.js");
        let facts = stream.facts();

        assert!(!facts.is_empty(), "fact builder should emit facts");

        let kinds: Vec<_> = facts.iter().map(|f| f.kind).collect();
        assert!(kinds.contains(&FactKind::Declaration));
        assert!(kinds.contains(&FactKind::Call));
        assert!(kinds.contains(&FactKind::PropertyWrite));
        assert!(kinds.contains(&FactKind::MemberRead));
    }

    #[test]
    fn facts_record_the_lexical_function_owner() {
        let stream = build_facts("fetch(); function helper() { fetch(); }", "owners.js");
        let calls = stream
            .facts()
            .iter()
            .filter(|fact| fact.kind == FactKind::Call)
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert_ne!(calls[0].function, calls[1].function);
    }

    #[test]
    fn fact_ids_are_sequential_and_deterministic() {
        let src = "const a = 1; const b = 2; foo();";
        let stream1 = build_facts(src, "ids.js");
        let stream2 = build_facts(src, "ids.js");

        let ids1: Vec<_> = stream1.facts().iter().map(|f| f.id.0).collect();
        let ids2: Vec<_> = stream2.facts().iter().map(|f| f.id.0).collect();
        assert_eq!(
            ids1, ids2,
            "identical programs must produce identical fact IDs"
        );
        assert_eq!(
            ids1,
            (0..u32::try_from(ids1.len()).expect("test fact count fits in u32"))
                .collect::<Vec<_>>(),
            "IDs must be sequential from 0"
        );
    }

    #[test]
    fn fact_count_is_independent_of_enabled_rules() {
        let src = "fetch('/api'); document.createElement('div');";
        let stream = build_facts(src, "invariant.js");
        let count = stream.len();

        let stream2 = build_facts(src, "invariant.js");
        assert_eq!(
            count,
            stream2.len(),
            "fact count must be invariant across runs"
        );
        assert_eq!(
            stream.fingerprint(),
            stream2.fingerprint(),
            "fact payloads and IDs must be invariant across runs"
        );
    }

    #[test]
    fn optional_chain_does_not_double_record_roles() {
        let src = "foo?.bar?.baz();";
        let stream = build_facts(src, "opt.js");
        let facts = stream.facts();

        assert_eq!(
            facts.iter().filter(|f| f.kind == FactKind::Call).count(),
            1,
            "optional call must emit exactly one Call fact"
        );

        let member_facts: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == FactKind::MemberRead)
            .collect();
        assert!(
            member_facts.len() <= 3,
            "optional member chain should not over-produce MemberRead facts, got {}",
            member_facts.len()
        );
    }

    #[test]
    fn nested_call_and_member_roles_have_distinct_facts() {
        let stream = build_facts("outer(inner(value.prop));", "nested.js");
        let calls = stream
            .facts()
            .iter()
            .filter(|fact| fact.kind == FactKind::Call)
            .collect::<Vec<_>>();
        let members = stream
            .facts()
            .iter()
            .filter(|fact| fact.kind == FactKind::MemberRead)
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert_eq!(members.len(), 1);
        assert_ne!(calls[0].id, calls[1].id);
        assert!(members[0].span.start() >= calls[0].span.start());
        assert!(members[0].span.end() <= calls[0].span.end());
    }

    #[test]
    fn repeated_builds_yield_identical_fact_fingerprints() {
        let src = r"
            const a = fetch('https://example.com');
            a.then(x => x.json());
            document.getElementById('root');
        ";

        let extract = |stream: FactStream<crate::analysis::facts::Frozen>| {
            stream
                .facts()
                .iter()
                .map(|f| (f.kind, f.span.start(), f.span.end(), f.function))
                .collect::<Vec<_>>()
        };

        let fp1 = extract(build_facts(src, "fp.js"));
        let fp2 = extract(build_facts(src, "fp.js"));
        let fp3 = extract(build_facts(src, "fp.js"));
        assert_eq!(
            fp1, fp2,
            "repeated builds must produce identical fingerprints"
        );
        assert_eq!(
            fp2, fp3,
            "repeated builds must produce identical fingerprints"
        );
    }

    #[test]
    fn call_fact_captures_callee_provenance() {
        let src = "fetch('/api');";
        let stream = build_facts(src, "call-prov.js");
        let call_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| f.kind == FactKind::Call)
            .collect();
        assert_eq!(call_facts.len(), 1);
        if let FactPayload::Call {
            call_provenance,
            callee_name,
            ..
        } = &call_facts[0].payload
        {
            assert!(
                matches!(call_provenance, SymbolCallProvenance::Global { name } if name == "fetch"),
                "fetch should resolve to global provenance"
            );
            assert_eq!(
                callee_name.and_then(|id| stream.resolve_name(id)),
                Some("fetch")
            );
        } else {
            panic!("expected Call payload");
        }
    }

    #[test]
    fn facts_retain_current_value_identities() {
        let src = r"
            function factory() {}
            const source = factory();
            const target = {};
            target.slot = source;
            const read = target.slot;
            class Constructor {}
            new Constructor();
            function outer() { function inner() {} }
        ";
        let stream = build_facts(src, "fact-identities.js");

        assert!(stream.facts().iter().any(|fact| {
            matches!(
                fact.payload,
                FactPayload::Reference { value, .. } if value != ValueId::UNKNOWN
            )
        }));
        assert!(stream.facts().iter().any(|fact| {
            matches!(
                fact.payload,
                FactPayload::Call { callee, .. } if callee != ValueId::UNKNOWN
            )
        }));
    }

    #[test]
    fn member_read_fact_captures_chain_info() {
        let src = "const x = document.body;";
        let stream = build_facts(src, "member-prov.js");
        let member_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| matches!(&f.payload, FactPayload::MemberRead { .. }))
            .collect();
        assert!(!member_facts.is_empty(), "should have member read facts");
        if let FactPayload::MemberRead { rooted_chain, .. } = &member_facts[0].payload {
            assert!(
                rooted_chain.is_some(),
                "document.body should have a rooted chain"
            );
        }
    }

    #[test]
    fn import_fact_is_emitted() {
        let src = r"import { x } from 'module';";
        let stream = build_facts(src, "import.js");
        let import_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| matches!(&f.payload, FactPayload::Import { .. }))
            .collect();
        assert_eq!(import_facts.len(), 1);
        if let FactPayload::Import { module } = &import_facts[0].payload {
            assert_eq!(module, "module");
        }
    }

    #[test]
    fn string_literal_fact_is_emitted() {
        let src = r#"const x = "hello";"#;
        let parsed = crate::parse(src, "str.js").expect("source should parse");
        let mut resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&mut resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let str_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| {
                matches!(&f.payload, FactPayload::Reference { value, .. }
                    if resolver.static_string_value(*value).is_some())
            })
            .collect();
        assert!(!str_facts.is_empty(), "should have string literal facts");
        assert!(
            str_facts
                .iter()
                .filter_map(|f| {
                    if let FactPayload::Reference { value, .. } = &f.payload {
                        resolver.static_string_value(*value)
                    } else {
                        None
                    }
                })
                .any(|value| value == "hello"),
            "should find 'hello' string literal"
        );
    }

    #[test]
    fn class_fact_is_emitted_for_class_declaration() {
        let src = r"class Foo extends Bar {}";
        let stream = build_facts(src, "class.js");
        let class_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| matches!(&f.payload, FactPayload::Class { .. }))
            .collect();
        assert!(!class_facts.is_empty(), "should have class facts");
        if let FactPayload::Class { name, .. } = &class_facts[0].payload {
            assert_eq!(name.as_deref(), Some("Foo"));
        }
    }

    #[test]
    fn instance_class_is_captured_for_this_calls() {
        let src = r"
            import { Base } from 'lib';
            class Foo extends Base {
                bar() { this.baz(); }
            }
        ";
        let stream = build_facts(src, "instance.js");
        let call_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| f.kind == FactKind::Call)
            .collect();
        let this_call = call_facts
            .iter()
            .find(|f| {
                if let FactPayload::Call { instance_class, .. } = &f.payload {
                    instance_class.is_some()
                } else {
                    false
                }
            })
            .expect("should find this.baz() call with instance_class");
        if let FactPayload::Call {
            instance_class,
            syntactic_path,
            ..
        } = &this_call.payload
        {
            assert!(
                instance_class.is_some(),
                "this.baz() inside a class with module superclass should capture instance_class"
            );
            assert!(
                syntactic_path.is_some(),
                "should have syntactic path for member call"
            );
        }
    }
}
