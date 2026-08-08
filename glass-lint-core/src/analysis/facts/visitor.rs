//! SWC visitor that turns syntax into the canonical semantic fact stream.
//!
//! Each visit method records semantic roles in evaluation order. Public
//! selection never reaches this visitor; all values, provenance, and control
//! regions are computed once for every file.
//!
//! Child traversal is suppressed where the parent already owns a semantic
//! role, such as an import source or a call callee; otherwise the same syntax
//! would produce duplicate facts and distort deterministic evidence order.

use glass_lint_datastructures::NamePath;
use smol_str::{SmolStr, ToSmolStr};
use swc_common::{Span, Spanned};
use swc_ecma_ast::ExportDefaultExpr;

use crate::analysis::facts::{
    ArrowExpr, AssignExpr, BinExpr, CallExpr, CondExpr, ControlKind, ControlRegionId, DoWhileStmt,
    ExportDecl, Expr, FactBuilder, FactPayload, FnDecl, ForInStmt, ForOfStmt, ForStmt, Function,
    Ident, IfStmt, ImportDecl, MemberExpr, NewExpr, OptChainBase, OptChainExpr, Pat, Str,
    SwitchStmt, SymbolCallProvenance, SymbolMemberProvenance, TargetProvenance, Tpl, TryStmt,
    UnaryExpr, UnaryOp, UpdateExpr, ValueId, VarDeclarator, Visit, VisitWith, WhileStmt,
    effective_callee_expr, literal_member_property_name,
};

struct ConstructionMetadata {
    callee_span: Span,
    callee_name: Option<SmolStr>,
    provenance: SymbolCallProvenance,
    rooted_chain: Option<NamePath>,
}

impl Visit for FactBuilder<'_, '_> {
    fn visit_ident(&mut self, ident: &Ident) {
        if self.resolver.budget.exhausted() {
            return;
        }
        // References are intentionally emitted even when the resolver cannot
        // prove their value. Unknown facts preserve source locations while
        // keeping downstream matchers fail-closed.
        let resolved = self.resolver.resolve_ident(ident);
        self.emit(
            ident.span(),
            FactPayload::Reference {
                value: resolved.id,
                provenance: resolved.call.clone(),
                static_string_origin: self.static_string_origin(resolved.id),
            },
        );
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        // A member expression is a read role at this node; its object and
        // property children are visited separately for their own references.
        let resolved = self.resolver.resolve_member(member);
        let chain = self.resolver.member_expression_chain(member);
        let syntactic_path = chain.as_ref().and_then(|path| self.name_path(path));
        self.emit(
            member.span(),
            FactPayload::MemberRead {
                syntactic_path,
                rooted_chain: self.rooted_path(resolved.rooted_chain.as_ref()),
                module_member: resolved.module_member.clone(),
                returned_member: self.returned_path(resolved.returned_member.as_ref()),
            },
        );
        member.visit_children_with(self);
    }

    fn visit_var_declarator(&mut self, declarator: &VarDeclarator) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_pattern_locals(&declarator.name);
        let source = self.declaration_source(declarator);
        declarator.name.visit_with(self);
        let targets = self.declaration_targets(&declarator.name);
        self.replace_declaration_provenance(
            &declarator.name,
            declarator.init.as_deref(),
            source,
            &targets,
        );
        self.emit_declarations(declarator.span(), source, targets);
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_assignment(assignment);
    }

    fn visit_update_expr(&mut self, update: &UpdateExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        update.arg.visit_with(self);
        let target = self.resolver.resolve_expr_id(&update.arg);
        let receiver = match &*update.arg {
            Expr::Member(member) => Some(self.resolver.resolve_expr_id(&member.obj)),
            _ => None,
        };
        self.emit(
            update.span(),
            FactPayload::Assignment {
                target,
                source: ValueId::UNKNOWN,
                receiver,
            },
        );
    }

    fn visit_unary_expr(&mut self, unary: &UnaryExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        unary.arg.visit_with(self);
        if unary.op == UnaryOp::Delete {
            let target = self.resolver.resolve_expr_id(&unary.arg);
            let receiver = match &*unary.arg {
                Expr::Member(member) => Some(self.resolver.resolve_expr_id(&member.obj)),
                _ => None,
            };
            self.emit(
                unary.span(),
                FactPayload::Assignment {
                    target,
                    source: ValueId::UNKNOWN,
                    receiver,
                },
            );
        }
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_call_expr(call);
    }

    fn visit_opt_chain_expr(&mut self, chain: &OptChainExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        match &*chain.base {
            OptChainBase::Call(call) => {
                let callee_expr = &call.callee;
                // Optional chaining has the same effective-call semantics as
                // ordinary calls, but its callee can itself be another chain.
                let optional_member = match effective_callee_expr(callee_expr) {
                    Expr::Member(member) => Some(member),
                    Expr::OptChain(inner) => match &*inner.base {
                        OptChainBase::Member(member) => Some(member),
                        OptChainBase::Call(_) => None,
                    },
                    _ => None,
                };
                if let Some(member) = optional_member
                    && matches!(
                        literal_member_property_name(&member.prop).as_deref(),
                        Some("call" | "apply")
                    )
                {
                    self.visit_callee_children(callee_expr);
                    call.args.visit_with(self);
                    self.try_emit_callable_wrapper_opt(member, call);
                } else {
                    let Some(resolved) = self.resolve_call_callee(callee_expr) else {
                        return;
                    };
                    self.visit_callee_children(callee_expr);
                    call.args.visit_with(self);
                    self.emit_call(chain.span(), resolved, &call.args, None);
                }
            }
            OptChainBase::Member(member) => {
                let resolved = self.resolver.resolve_member(member);
                let chain = self.resolver.member_expression_chain(member);
                let syntactic_path = chain.as_ref().and_then(|path| self.name_path(path));
                self.emit(
                    member.span(),
                    FactPayload::MemberRead {
                        syntactic_path,
                        rooted_chain: self.rooted_path(resolved.rooted_chain.as_ref()),
                        module_member: resolved.module_member.clone(),
                        returned_member: self.returned_path(resolved.returned_member.as_ref()),
                    },
                );
                member.visit_children_with(self);
            }
        }
    }

    fn visit_new_expr(&mut self, new_expr: &NewExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        let metadata = self.resolve_construction_metadata(new_expr);
        self.visit_construction_children(new_expr);
        self.emit_construction_fact(new_expr, metadata);
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_static_import(import);
        // Do not visit children: the source string is already captured in the
        // Import fact, and visiting it would emit a duplicate static reference.
    }

    fn visit_str(&mut self, value: &Str) {
        if self.resolver.budget.exhausted() {
            return;
        }
        let id = self
            .resolver
            .resolve_expr(&Expr::Lit(swc_ecma_ast::Lit::Str(value.clone())))
            .id;
        self.emit(
            value.span(),
            FactPayload::Reference {
                value: id,
                provenance: SymbolCallProvenance::Local,
                static_string_origin: None,
            },
        );
        if let Some(terminal_id) = self.resolver.static_string_terminal_id(id)
            && let Ok(span) = self.resolver.normalize_span(value.span())
        {
            self.provenance
                .record_static_string_origin(terminal_id, span);
        }
    }

    fn visit_tpl(&mut self, template: &Tpl) {
        if self.resolver.budget.exhausted() {
            return;
        }
        let complete = self.resolver.resolve_expr(&Expr::Tpl(template.clone())).id;
        if self.resolver.static_string_value(complete).is_none() {
            for quasi in &template.quasis {
                let literal = quasi.cooked.as_ref().map_or_else(
                    || quasi.raw.to_string(),
                    |value| value.to_string_lossy().to_string(),
                );
                let resolved = self.resolver.static_string(literal);
                self.emit(
                    quasi.span,
                    FactPayload::Reference {
                        value: resolved.id,
                        provenance: SymbolCallProvenance::Local,
                        static_string_origin: None,
                    },
                );
            }
        } else {
            self.emit(
                template.span,
                FactPayload::Reference {
                    value: complete,
                    provenance: SymbolCallProvenance::Local,
                    static_string_origin: None,
                },
            );
        }
        template.visit_children_with(self);
    }

    fn visit_class_decl(&mut self, class_decl: &swc_ecma_ast::ClassDecl) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_class_decl(class_decl);
    }

    fn visit_class_expr(&mut self, class_expr: &swc_ecma_ast::ClassExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_class_expr(class_expr);
    }

    fn visit_bin_expr(&mut self, binary: &BinExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        if binary.op != swc_ecma_ast::BinaryOp::InstanceOf {
            let complete = self.resolver.resolve_expr(&Expr::Bin(binary.clone())).id;
            if self.resolver.static_string_value(complete).is_some() {
                self.emit(
                    binary.span(),
                    FactPayload::Reference {
                        value: complete,
                        provenance: SymbolCallProvenance::Local,
                        static_string_origin: None,
                    },
                );
            }
        }
        self.record_instanceof(binary);
    }

    fn visit_fn_decl(&mut self, function: &FnDecl) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_function_decl(function);
    }

    fn visit_function(&mut self, function: &Function) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_function(function);
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_arrow(arrow);
    }

    fn visit_class_method(&mut self, method: &swc_ecma_ast::ClassMethod) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_class_method(method);
    }

    fn visit_if_stmt(&mut self, stmt: &IfStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_if(stmt);
    }

    fn visit_for_stmt(&mut self, stmt: &ForStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_for(stmt);
    }

    fn visit_for_in_stmt(&mut self, stmt: &ForInStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_for_in(stmt);
    }

    fn visit_for_of_stmt(&mut self, stmt: &ForOfStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_for_of(stmt);
    }

    fn visit_while_stmt(&mut self, stmt: &WhileStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_while(stmt);
    }

    fn visit_do_while_stmt(&mut self, stmt: &DoWhileStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_do_while(stmt);
    }

    fn visit_switch_stmt(&mut self, stmt: &SwitchStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_switch(stmt);
    }

    fn visit_try_stmt(&mut self, stmt: &TryStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_try(stmt);
    }

    fn visit_cond_expr(&mut self, expr: &CondExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_conditional(expr);
    }

    fn visit_break_stmt(&mut self, stmt: &swc_ecma_ast::BreakStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.emit_control(stmt.span(), ControlKind::Break, ControlRegionId::new(0));
    }

    fn visit_continue_stmt(&mut self, stmt: &swc_ecma_ast::ContinueStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.emit_control(stmt.span(), ControlKind::Continue, ControlRegionId::new(0));
    }

    fn visit_return_stmt(&mut self, stmt: &swc_ecma_ast::ReturnStmt) {
        if self.resolver.budget.exhausted() {
            return;
        }
        stmt.arg.visit_with(self);
        let value = stmt
            .arg
            .as_deref()
            .map_or(crate::analysis::model::value::ValueId::UNKNOWN, |expr| {
                self.resolver.resolve_expr_id(expr)
            });
        self.emit(
            stmt.span(),
            FactPayload::Return {
                region: ControlRegionId::new(0),
                value,
            },
        );
    }

    fn visit_export_decl(&mut self, export: &ExportDecl) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_export_decl(&export.decl);
        export.decl.visit_with(self);
    }

    fn visit_named_export(&mut self, export: &swc_ecma_ast::NamedExport) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_named_export(export);
    }

    fn visit_export_all(&mut self, export: &swc_ecma_ast::ExportAll) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_export_all(export);
    }

    fn visit_export_default_expr(&mut self, export: &ExportDefaultExpr) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_default_expr(export);
        export.expr.visit_with(self);
    }

    fn visit_export_default_decl(&mut self, export: &swc_ecma_ast::ExportDefaultDecl) {
        if self.resolver.budget.exhausted() {
            return;
        }
        self.record_default_decl(export);
        export.decl.visit_with(self);
    }
}

impl FactBuilder<'_, '_> {
    fn resolve_construction_metadata(&mut self, new_expr: &NewExpr) -> ConstructionMetadata {
        let result = self.resolver.fresh_object_value_at(new_expr.span).id;
        if let Some(instance_class) = self.instance_origin_for_constructor(&new_expr.callee) {
            self.provenance
                .record_instance_origin(result, instance_class, self.resolver.budget);
        }
        let effective_callee = effective_callee_expr(&new_expr.callee);
        let resolved = self.resolver.resolve_expr(effective_callee);
        let rooted_chain = resolved
            .rooted_chain
            .as_ref()
            .and_then(|path| self.name_path(path));
        let callee_span = effective_callee.span();

        // Resolve callee name and provenance for member expression callees
        // like `new globalThis.URL(...)` or `new mod.Foo(...)`.
        let (callee_name, provenance) = match effective_callee {
            Expr::Ident(ident) => {
                let p = resolved.provenance.call;
                (Some(ident.sym.to_smolstr()), p)
            }
            Expr::Member(member) => {
                let member_resolved = self.resolver.resolve_member(member);
                if let Some(SymbolMemberProvenance::ModuleNamespace {
                    ref module,
                    member: ref member_name,
                }) = member_resolved.module_member
                {
                    (
                        Some(member_name.clone()),
                        SymbolCallProvenance::ModuleExport {
                            module: module.clone(),
                            export: member_name.clone(),
                        },
                    )
                } else {
                    (
                        literal_member_property_name(&member.prop),
                        resolved.provenance.call,
                    )
                }
            }
            _ => (None, resolved.provenance.call),
        };

        ConstructionMetadata {
            callee_span,
            callee_name,
            provenance,
            rooted_chain,
        }
    }

    fn visit_construction_children(&mut self, new_expr: &NewExpr) {
        new_expr.visit_children_with(self);
    }

    fn emit_construction_fact(&mut self, new_expr: &NewExpr, metadata: ConstructionMetadata) {
        let Some(callee_span) = self.byte_range(metadata.callee_span) else {
            return;
        };
        let callee_name = self.intern_name(metadata.callee_name.as_deref());
        self.emit(
            new_expr.span(),
            FactPayload::Construction {
                callee_span,
                callee_name,
                provenance: metadata.provenance,
                rooted_chain: metadata.rooted_chain,
            },
        );
    }

    fn declaration_source(&mut self, declarator: &VarDeclarator) -> ValueId {
        let source = declarator
            .init
            .as_ref()
            .map_or(ValueId::UNKNOWN, |init| self.value_for_expr(init));
        // Initializers are evaluated before the declaration becomes visible.
        // Emit the declaration after visiting the initializer so fact order is
        // an evaluation order, not merely an AST preorder.
        if let Some(init) = &declarator.init {
            init.visit_with(self);
        }
        if Self::is_simple_pattern(&declarator.name) {
            source
        } else {
            ValueId::UNKNOWN
        }
    }

    fn declaration_targets(&mut self, pattern: &Pat) -> Vec<ValueId> {
        let mut targets = Vec::new();
        self.pattern_values(pattern, &mut targets);
        targets
    }

    fn replace_declaration_provenance(
        &mut self,
        pattern: &Pat,
        init: Option<&Expr>,
        source: ValueId,
        targets: &[ValueId],
    ) {
        let replacement = if Self::is_simple_pattern(pattern) {
            init.map_or_else(TargetProvenance::default, |init| {
                self.target_provenance(init, source)
            })
        } else {
            TargetProvenance::default()
        };
        self.provenance
            .replace_targets(targets, &replacement, self.resolver.budget);
    }

    fn emit_declarations(&mut self, span: Span, source: ValueId, mut targets: Vec<ValueId>) {
        if targets.is_empty() {
            targets.push(ValueId::UNKNOWN);
        }
        for target in targets {
            self.emit(span, FactPayload::Declaration { target, source });
        }
    }
}
