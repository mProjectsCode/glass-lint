//! SWC visitor that turns syntax into the canonical semantic fact stream.
//!
//! Each visit method records semantic roles in evaluation order. Matcher
//! selection never reaches this visitor; all values, provenance, and control
//! regions are computed once for every file.

use super::*;

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
                value: resolved.id,
                syntactic_chain,
                rooted_chain: resolved.rooted_chain.clone(),
                module_member: resolved.module_member.clone(),
                returned_member: resolved.returned_member.clone(),
            },
        );
        member.visit_children_with(self);
    }

    fn visit_var_declarator(&mut self, declarator: &VarDeclarator) {
        let mut source = declarator
            .init
            .as_ref()
            .map(|init| self.value_for_expr(init))
            .unwrap_or(ValueId::UNKNOWN);
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
        let source = self.value_for_expr(&assignment.right);
        match &assignment.left {
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(ident)) => {
                assignment.right.visit_with(self);
                let target = self.resolver.resolve_ident(&ident.id).id;
                self.emit(
                    FactKind::Assignment,
                    assignment.span(),
                    FactPayload::Assignment {
                        target,
                        source,
                        receiver: None,
                    },
                );
            }
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(
                member,
            )) => {
                // Evaluate the member reference (including computed keys) and
                // the RHS before emitting the write/kill fact.
                member.obj.visit_with(self);
                member.prop.visit_with(self);
                let resolved_member = self.resolver.resolve_member(member);
                self.emit(
                    FactKind::MemberRead,
                    member.span(),
                    FactPayload::MemberRead {
                        value: resolved_member.id,
                        syntactic_chain: self.resolver.member_chain(member),
                        rooted_chain: resolved_member.rooted_chain.clone(),
                        module_member: resolved_member.module_member.clone(),
                        returned_member: resolved_member.returned_member.clone(),
                    },
                );
                assignment.right.visit_with(self);
                let target = resolved_member.id;
                let property = member_prop_name(&member.prop);
                if assignment.op == swc_ecma_ast::AssignOp::Assign {
                    self.emit(
                        FactKind::PropertyWrite,
                        assignment.span(),
                        FactPayload::PropertyWrite {
                            target,
                            receiver: self.resolver.resolve_expr(&member.obj).id,
                            source,
                            property,
                            static_value: self.resolver.static_string_expr(&assignment.right),
                        },
                    );
                } else {
                    self.emit(
                        FactKind::Assignment,
                        assignment.span(),
                        FactPayload::Assignment {
                            target,
                            source: ValueId::UNKNOWN,
                            receiver: Some(self.resolver.resolve_expr(&member.obj).id),
                        },
                    );
                }
            }
            swc_ecma_ast::AssignTarget::Pat(pattern) => {
                // Destructuring targets do not have one value identity. Emit
                // conservative writes for each proven target so flow state is
                // invalidated without inventing a shared source value.
                assignment.right.visit_with(self);
                let pattern: Pat = pattern.clone().into();
                let mut targets = Vec::new();
                self.pattern_write_targets(&pattern, &mut targets);
                for (target, receiver) in targets {
                    self.emit(
                        FactKind::Assignment,
                        assignment.span(),
                        FactPayload::Assignment {
                            target,
                            source: ValueId::UNKNOWN,
                            receiver,
                        },
                    );
                }
            }
            _ => {}
        }
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
        let Callee::Expr(callee_expr) = &call.callee else {
            let result = self.call_result(call.span());
            let args = self.args_info(&call.args);
            self.emit(
                FactKind::Call,
                call.span(),
                FactPayload::Call {
                    callee: ValueId::UNKNOWN,
                    receiver: None,
                    result,
                    callee_span: call.span,
                    callee_name: None,
                    call_provenance: SymbolCallProvenance::Local,
                    syntactic_chain: None,
                    rooted_chain: None,
                    module_member: None,
                    returned_member: None,
                    instance_class: None,
                    target_function: None,
                    args,
                    unwrap: None,
                },
            );
            return;
        };

        // Detect .call()/.apply() wrapper patterns before ordinary call
        // resolution. The wrapper fact retains the effective target and
        // arguments so all consumers agree on the same invocation shape.
        if let Expr::Member(member) = effective_callee_expr(callee_expr)
            && matches!(
                member_prop_name(&member.prop).as_deref(),
                Some("call" | "apply")
            )
        {
            self.visit_callee_children(callee_expr);
            call.args.visit_with(self);
            self.try_emit_callable_wrapper(member, call);
            self.emit_require_import(call);
            return;
        }

        let resolved = self.resolve_call_callee(callee_expr);
        self.visit_callee_children(callee_expr);
        call.args.visit_with(self);
        self.emit_call(call.span, resolved, &call.args, None);
        self.emit_require_import(call);
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
                        _ => None,
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
                    let resolved = self.resolve_call_callee(callee_expr);
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
                        value: resolved.id,
                        syntactic_chain,
                        rooted_chain: resolved.rooted_chain.clone(),
                        module_member: resolved.module_member.clone(),
                        returned_member: resolved.returned_member.clone(),
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
                let p = resolved.call.clone();
                (
                    Some(
                        resolved
                            .rooted_chain
                            .clone()
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
                        .or((chain == "Function").then_some(chain))
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
                    (None, resolved.call.clone())
                }
            }
            _ => (None, resolved.call.clone()),
        };

        new_expr.visit_children_with(self);
        let result = self.resolver.fresh_object_value_at(new_expr.span).id;
        self.emit(
            FactKind::Construction,
            new_expr.span(),
            FactPayload::Construction {
                callee: resolved.id,
                result,
                callee_span,
                callee_name,
                provenance,
            },
        );
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        let module = import.src.value.to_string_lossy().to_string();
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
            },
        );
    }

    fn visit_tpl(&mut self, template: &Tpl) {
        for quasi in &template.quasis {
            let literal = quasi
                .cooked
                .as_ref()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| quasi.raw.to_string());
            self.emit(
                FactKind::Reference,
                quasi.span,
                FactPayload::Reference {
                    value: ValueId::UNKNOWN,
                    static_string: Some(literal),
                },
            );
        }
        template.visit_children_with(self);
    }

    fn visit_class_decl(&mut self, class_decl: &ClassDecl) {
        let name = class_decl.ident.sym.to_string();
        let provenance = class_decl
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr));
        self.emit(
            FactKind::Declaration,
            class_decl.ident.span(),
            FactPayload::Class {
                name,
                provenance: provenance.clone(),
            },
        );
        self.class_stack.push(provenance);
        class_decl.visit_children_with(self);
        self.class_stack.pop();
    }

    fn visit_class_expr(&mut self, class_expr: &ClassExpr) {
        let provenance = class_expr
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr));
        if let Some(ident) = &class_expr.ident {
            self.emit(
                FactKind::Declaration,
                ident.span(),
                FactPayload::Class {
                    name: ident.sym.to_string(),
                    provenance: provenance.clone(),
                },
            );
        }
        self.class_stack.push(provenance);
        class_expr.visit_children_with(self);
        self.class_stack.pop();
    }

    fn visit_bin_expr(&mut self, binary: &BinExpr) {
        if binary.op == BinaryOp::InstanceOf {
            let provenance = self.resolver.class_provenance(&binary.right);
            self.emit(
                FactKind::Reference,
                binary.right.span(),
                FactPayload::Class {
                    name: String::new(),
                    provenance,
                },
            );
        }
        binary.visit_children_with(self);
    }

    fn visit_fn_decl(&mut self, function: &FnDecl) {
        self.function_depth += 1;
        function.visit_children_with(self);
        self.function_depth -= 1;
    }

    fn visit_function(&mut self, function: &Function) {
        // Function boundaries let flow analysis save and restore caller state;
        // parameters are captured on the enter and exit markers themselves.
        self.emit_function_fact(
            function.span(),
            function
                .params
                .iter()
                .enumerate()
                .map(|(index, p)| (index, p.pat.clone())),
            FunctionBoundary::Enter,
        );
        self.function_depth += 1;
        function.visit_children_with(self);
        self.function_depth -= 1;
        self.emit_function_fact(
            function.span(),
            function
                .params
                .iter()
                .enumerate()
                .map(|(index, p)| (index, p.pat.clone())),
            FunctionBoundary::Exit,
        );
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        self.emit_function_fact(
            arrow.span(),
            arrow.params.iter().cloned().enumerate(),
            FunctionBoundary::Enter,
        );
        arrow.body.visit_with(self);
        self.emit_function_fact(
            arrow.span(),
            arrow.params.iter().cloned().enumerate(),
            FunctionBoundary::Exit,
        );
    }

    fn visit_class_method(&mut self, method: &swc_ecma_ast::ClassMethod) {
        self.emit_function_fact(
            method.function.span(),
            method
                .function
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| (index, parameter.pat.clone())),
            FunctionBoundary::Enter,
        );
        if method.is_static {
            self.static_method_depth += 1;
        }
        // Visit only the method body so the method gets one function boundary
        // pair, rather than a nested duplicate Function walk.
        if let Some(body) = method.function.body.as_ref() {
            body.visit_with(self);
        }
        self.emit_function_fact(
            method.function.span(),
            method
                .function
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| (index, parameter.pat.clone())),
            FunctionBoundary::Exit,
        );
        if method.is_static {
            self.static_method_depth -= 1;
        }
    }

    fn visit_if_stmt(&mut self, stmt: &IfStmt) {
        // Control markers are balanced around child traversal. The projector
        // uses their order to join only environments that reach each exit.
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::BranchStart, region);
        stmt.test.visit_with(self);
        self.emit_control(stmt.cons.span(), ControlKind::BranchThen, region);
        stmt.cons.visit_with(self);
        if let Some(alt) = &stmt.alt {
            self.emit_control(alt.span(), ControlKind::BranchElse, region);
            alt.visit_with(self);
        }
        self.emit_control(stmt.span(), ControlKind::BranchEnd, region);
    }

    fn visit_for_stmt(&mut self, stmt: &ForStmt) {
        if let Some(init) = &stmt.init {
            init.visit_with(self);
        }
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: false },
            region,
        );
        // The test is evaluated before the first iteration. The update is
        // evaluated after the body, matching JavaScript evaluation order.
        if let Some(test) = &stmt.test {
            test.visit_with(self);
        }
        stmt.body.visit_with(self);
        if let Some(update) = &stmt.update {
            self.emit_control(stmt.span(), ControlKind::LoopUpdate, region);
            update.visit_with(self);
        }
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
    }

    fn visit_for_in_stmt(&mut self, stmt: &ForInStmt) {
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: false },
            region,
        );
        stmt.left.visit_with(self);
        stmt.right.visit_with(self);
        stmt.body.visit_with(self);
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
    }

    fn visit_for_of_stmt(&mut self, stmt: &ForOfStmt) {
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: false },
            region,
        );
        stmt.left.visit_with(self);
        stmt.right.visit_with(self);
        stmt.body.visit_with(self);
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
    }

    fn visit_while_stmt(&mut self, stmt: &WhileStmt) {
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: false },
            region,
        );
        stmt.test.visit_with(self);
        stmt.body.visit_with(self);
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
    }

    fn visit_do_while_stmt(&mut self, stmt: &DoWhileStmt) {
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: true },
            region,
        );
        stmt.body.visit_with(self);
        stmt.test.visit_with(self);
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
    }

    fn visit_switch_stmt(&mut self, stmt: &SwitchStmt) {
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::SwitchStart, region);
        stmt.discriminant.visit_with(self);
        for case in &stmt.cases {
            self.emit_control(
                case.span(),
                ControlKind::SwitchCase {
                    is_default: case.test.is_none(),
                },
                region,
            );
            case.visit_with(self);
        }
        self.emit_control(stmt.span(), ControlKind::SwitchEnd, region);
    }

    fn visit_try_stmt(&mut self, stmt: &TryStmt) {
        // Try/catch/finally markers preserve separate normal and abrupt exits;
        // do not collapse them into one linear region here.
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::TryStart, region);
        stmt.block.visit_with(self);
        if let Some(handler) = &stmt.handler {
            self.emit_control(handler.span(), ControlKind::CatchStart, region);
            handler.visit_with(self);
        }
        if let Some(finalizer) = &stmt.finalizer {
            self.emit_control(finalizer.span(), ControlKind::FinallyStart, region);
            finalizer.visit_with(self);
        }
        self.emit_control(stmt.span(), ControlKind::TryEnd, region);
    }

    fn visit_cond_expr(&mut self, expr: &CondExpr) {
        let region = self.next_control_region();
        self.emit_control(expr.span(), ControlKind::BranchStart, region);
        expr.test.visit_with(self);
        self.emit_control(expr.cons.span(), ControlKind::BranchThen, region);
        expr.cons.visit_with(self);
        self.emit_control(expr.alt.span(), ControlKind::BranchElse, region);
        expr.alt.visit_with(self);
        self.emit_control(expr.span(), ControlKind::BranchEnd, region);
    }

    fn visit_break_stmt(&mut self, stmt: &swc_ecma_ast::BreakStmt) {
        self.emit_control(stmt.span(), ControlKind::Break, 0);
    }

    fn visit_continue_stmt(&mut self, stmt: &swc_ecma_ast::ContinueStmt) {
        self.emit_control(stmt.span(), ControlKind::Continue, 0);
    }

    fn visit_return_stmt(&mut self, stmt: &swc_ecma_ast::ReturnStmt) {
        stmt.arg.visit_with(self);
        self.emit_control(stmt.span(), ControlKind::Return, 0);
    }

    fn visit_export_decl(&mut self, export: &ExportDecl) {
        export.decl.visit_with(self);
    }
}
