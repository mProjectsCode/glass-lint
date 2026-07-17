//! SWC visitor that turns syntax into the canonical semantic fact stream.
//!
//! Each visit method records semantic roles in evaluation order. Public
//! selection never reaches this visitor; all values, provenance, and control
//! regions are computed once for every file.
//!
//! Child traversal is suppressed where the parent already owns a semantic
//! role, such as an import source or a call callee; otherwise the same syntax
//! would produce duplicate facts and distort deterministic evidence order.

use swc_ecma_ast::ExportDefaultExpr;

use super::{
    ArrowExpr, AssignExpr, BinExpr, CallExpr, CondExpr, ControlKind, DoWhileStmt, ExportDecl, Expr,
    FactBuilder, FactKind, FactPayload, FnDecl, ForInStmt, ForOfStmt, ForStmt, Function, Ident,
    IfStmt, ImportDecl, MemberExpr, NewExpr, OptChainBase, OptChainExpr, Spanned, Str, SwitchStmt,
    SymbolCallProvenance, SymbolMemberProvenance, Tpl, TryStmt, UnaryExpr, UnaryOp, UpdateExpr,
    ValueId, VarDeclarator, Visit, VisitWith, WhileStmt, effective_callee_expr, member_prop_name,
};
use crate::{
    analysis::module::{ImportedBinding, ModuleRequestRole},
    project::ResolutionRequestKind,
};

impl Visit for FactBuilder<'_> {
    fn visit_ident(&mut self, ident: &Ident) {
        // References are intentionally emitted even when the resolver cannot
        // prove their value. Unknown facts preserve source locations while
        // keeping downstream matchers fail-closed.
        let resolved = self.resolver.resolve_ident(ident);
        self.emit(
            FactKind::Reference,
            ident.span(),
            FactPayload::Reference {
                value: resolved.id,
                static_string: None,
                provenance: resolved.call,
            },
        );
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        // A member expression is a read role at this node; its object and
        // property children are visited separately for their own references.
        let resolved = self.resolver.resolve_member(member);
        let syntactic_chain = self.resolver.member_chain(member);
        self.emit(
            FactKind::MemberRead,
            member.span(),
            FactPayload::MemberRead {
                syntactic_chain,
                rooted_chain: resolved.rooted_chain.clone(),
                module_member: resolved.module_member.clone(),
                returned_member: resolved.returned_member,
            },
        );
        member.visit_children_with(self);
    }

    fn visit_var_declarator(&mut self, declarator: &VarDeclarator) {
        self.record_pattern_locals(&declarator.name);
        let mut source = declarator
            .init
            .as_ref()
            .map_or(ValueId::UNKNOWN, |init| self.value_for_expr(init));
        // Initializers are evaluated before the declaration becomes visible.
        // Emit the declaration after visiting the initializer so fact order is
        // an evaluation order, not merely an AST preorder.
        if let Some(init) = &declarator.init {
            init.visit_with(self);
        }
        declarator.name.visit_with(self);
        let mut targets = Vec::new();
        self.pattern_values(&declarator.name, &mut targets);
        if targets.is_empty() {
            targets.push(ValueId::UNKNOWN);
        }
        if !Self::is_simple_pattern(&declarator.name) {
            source = ValueId::UNKNOWN;
        }
        for target in targets {
            self.emit(
                FactKind::Declaration,
                declarator.span(),
                FactPayload::Declaration { target, source },
            );
        }
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        self.record_assignment(assignment);
    }

    fn visit_update_expr(&mut self, update: &UpdateExpr) {
        update.arg.visit_with(self);
        let target = self.resolver.resolve_expr(&update.arg).id;
        self.emit(
            FactKind::Assignment,
            update.span(),
            FactPayload::Assignment {
                target,
                source: ValueId::UNKNOWN,
                receiver: match &*update.arg {
                    Expr::Member(member) => Some(self.resolver.resolve_expr(&member.obj).id),
                    _ => None,
                },
            },
        );
    }

    fn visit_unary_expr(&mut self, unary: &UnaryExpr) {
        unary.arg.visit_with(self);
        if unary.op == UnaryOp::Delete {
            let target = self.resolver.resolve_expr(&unary.arg).id;
            self.emit(
                FactKind::Assignment,
                unary.span(),
                FactPayload::Assignment {
                    target,
                    source: ValueId::UNKNOWN,
                    receiver: match &*unary.arg {
                        Expr::Member(member) => Some(self.resolver.resolve_expr(&member.obj).id),
                        _ => None,
                    },
                },
            );
        }
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        self.record_call_expr(call);
    }

    fn visit_opt_chain_expr(&mut self, chain: &OptChainExpr) {
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
                        member_prop_name(&member.prop).as_deref(),
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
                let syntactic_chain = self.resolver.member_chain(member);
                self.emit(
                    FactKind::MemberRead,
                    member.span(),
                    FactPayload::MemberRead {
                        syntactic_chain,
                        rooted_chain: resolved.rooted_chain.clone(),
                        module_member: resolved.module_member.clone(),
                        returned_member: resolved.returned_member,
                    },
                );
                member.visit_children_with(self);
            }
        }
    }

    fn visit_new_expr(&mut self, new_expr: &NewExpr) {
        let resolved = self.resolver.resolve_expr(&new_expr.callee);
        let callee_span = new_expr.callee.span();

        // Resolve callee name and provenance for member expression callees
        // like `new globalThis.URL(...)` or `new mod.Foo(...)`.
        let (callee_name, provenance) = match &*new_expr.callee {
            Expr::Ident(ident) => {
                let p = resolved.call;
                (
                    Some(
                        resolved
                            .rooted_chain
                            .unwrap_or_else(|| ident.sym.to_string()),
                    ),
                    p,
                )
            }
            Expr::Member(member) => {
                let member_resolved = self.resolver.resolve_member(member);
                let global_name = member_resolved.rooted_chain.as_deref().and_then(|chain| {
                    chain
                        .strip_prefix("globalThis.")
                        .filter(|_| {
                            matches!(
                                self.resolver.resolve_expr(&member.obj).call,
                                SymbolCallProvenance::Global { ref name } if name == "globalThis"
                            )
                        })
                        .or_else(|| (chain == "Function").then_some(chain))
                });
                if let Some(name) = global_name {
                    let name = name.to_string();
                    (Some(name.clone()), SymbolCallProvenance::Global { name })
                } else if let Some(SymbolMemberProvenance::ModuleNamespace {
                    module,
                    member: member_name,
                }) = member_resolved.module_member
                {
                    (
                        Some(member_name.clone()),
                        SymbolCallProvenance::ModuleExport {
                            module,
                            export: member_name,
                        },
                    )
                } else {
                    (None, resolved.call)
                }
            }
            _ => (None, resolved.call),
        };

        new_expr.visit_children_with(self);
        let Some(callee_span) = self.byte_range(callee_span) else {
            return;
        };
        self.emit(
            FactKind::Construction,
            new_expr.span(),
            FactPayload::Construction {
                callee_span,
                callee_name,
                provenance,
            },
        );
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        let module = import.src.value.to_string_lossy().to_string();
        if import.type_only {
            return;
        }
        let bindings = import
            .specifiers
            .iter()
            .filter(|specifier| !specifier.is_type_only())
            .map(|specifier| match specifier {
                swc_ecma_ast::ImportSpecifier::Named(named) => ImportedBinding::new(
                    Some(named.imported.as_ref().map_or_else(
                        || named.local.sym.to_string(),
                        crate::analysis::syntax::module_export_name,
                    )),
                    named.local.sym.to_string(),
                    false,
                ),
                swc_ecma_ast::ImportSpecifier::Default(default) => ImportedBinding::new(
                    Some("default".into()),
                    default.local.sym.to_string(),
                    false,
                ),
                swc_ecma_ast::ImportSpecifier::Namespace(namespace) => {
                    ImportedBinding::new(None, namespace.local.sym.to_string(), true)
                }
            })
            .collect();
        self.record_local_imports(import);
        let Some(span) = self.byte_range(import.src.span) else {
            return;
        };
        self.interface.add_request(
            span,
            ResolutionRequestKind::Import,
            module.clone(),
            ModuleRequestRole::Import { bindings },
        );
        self.emit(
            FactKind::Declaration,
            import.src.span,
            FactPayload::Import { module },
        );
        // Do not visit children: the source string is already captured in the
        // Import fact, and visiting it would emit a duplicate static reference.
    }

    fn visit_str(&mut self, value: &Str) {
        let literal = value.value.to_string_lossy().to_string();
        self.emit(
            FactKind::Reference,
            value.span(),
            FactPayload::Reference {
                value: self
                    .resolver
                    .resolve_expr(&Expr::Lit(swc_ecma_ast::Lit::Str(value.clone())))
                    .id,
                static_string: Some(literal),
                provenance: SymbolCallProvenance::Local,
            },
        );
    }

    fn visit_tpl(&mut self, template: &Tpl) {
        for quasi in &template.quasis {
            let literal = quasi.cooked.as_ref().map_or_else(
                || quasi.raw.to_string(),
                |value| value.to_string_lossy().to_string(),
            );
            self.emit(
                FactKind::Reference,
                quasi.span,
                FactPayload::Reference {
                    value: ValueId::UNKNOWN,
                    static_string: Some(literal),
                    provenance: SymbolCallProvenance::Local,
                },
            );
        }
        template.visit_children_with(self);
    }

    fn visit_class_decl(&mut self, class_decl: &swc_ecma_ast::ClassDecl) {
        self.record_class_decl(class_decl);
    }

    fn visit_class_expr(&mut self, class_expr: &swc_ecma_ast::ClassExpr) {
        self.record_class_expr(class_expr);
    }

    fn visit_bin_expr(&mut self, binary: &BinExpr) {
        self.record_instanceof(binary);
    }

    fn visit_fn_decl(&mut self, function: &FnDecl) {
        self.record_function_decl(function);
    }

    fn visit_function(&mut self, function: &Function) {
        self.record_function(function);
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        self.record_arrow(arrow);
    }

    fn visit_class_method(&mut self, method: &swc_ecma_ast::ClassMethod) {
        self.record_class_method(method);
    }

    fn visit_if_stmt(&mut self, stmt: &IfStmt) {
        self.record_if(stmt);
    }

    fn visit_for_stmt(&mut self, stmt: &ForStmt) {
        self.record_for(stmt);
    }

    fn visit_for_in_stmt(&mut self, stmt: &ForInStmt) {
        self.record_for_in(stmt);
    }

    fn visit_for_of_stmt(&mut self, stmt: &ForOfStmt) {
        self.record_for_of(stmt);
    }

    fn visit_while_stmt(&mut self, stmt: &WhileStmt) {
        self.record_while(stmt);
    }

    fn visit_do_while_stmt(&mut self, stmt: &DoWhileStmt) {
        self.record_do_while(stmt);
    }

    fn visit_switch_stmt(&mut self, stmt: &SwitchStmt) {
        self.record_switch(stmt);
    }

    fn visit_try_stmt(&mut self, stmt: &TryStmt) {
        self.record_try(stmt);
    }

    fn visit_cond_expr(&mut self, expr: &CondExpr) {
        self.record_conditional(expr);
    }

    fn visit_break_stmt(&mut self, stmt: &swc_ecma_ast::BreakStmt) {
        self.emit_control(stmt.span(), ControlKind::Break, super::ControlRegionId(0));
    }

    fn visit_continue_stmt(&mut self, stmt: &swc_ecma_ast::ContinueStmt) {
        self.emit_control(
            stmt.span(),
            ControlKind::Continue,
            super::ControlRegionId(0),
        );
    }

    fn visit_return_stmt(&mut self, stmt: &swc_ecma_ast::ReturnStmt) {
        stmt.arg.visit_with(self);
        let value = stmt
            .arg
            .as_deref()
            .map_or(crate::analysis::value::ValueId::UNKNOWN, |expr| {
                self.resolver.resolve_expr(expr).id
            });
        self.emit(
            FactKind::Control,
            stmt.span(),
            FactPayload::Control {
                kind: ControlKind::Return,
                region: super::ControlRegionId(0),
                value,
            },
        );
    }

    fn visit_export_decl(&mut self, export: &ExportDecl) {
        self.record_export_decl(&export.decl);
        export.decl.visit_with(self);
    }

    fn visit_named_export(&mut self, export: &swc_ecma_ast::NamedExport) {
        self.record_named_export(export);
    }

    fn visit_export_all(&mut self, export: &swc_ecma_ast::ExportAll) {
        self.record_export_all(export);
    }

    fn visit_export_default_expr(&mut self, export: &ExportDefaultExpr) {
        self.record_default_expr(export);
    }

    fn visit_export_default_decl(&mut self, export: &swc_ecma_ast::ExportDefaultDecl) {
        self.record_default_decl(export);
    }
}
