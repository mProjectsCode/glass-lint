use super::*;

impl<'a> FactBuilder<'a> {
    pub(super) fn arg_info(&self, expr: &Expr) -> CallArgInfo {
        let value = self.resolver.resolve_expr(expr).id;
        let (base_value, base_path) = self.expression_projection(expr);
        let mut projections = Vec::new();
        self.collect_value_projections(expr, &mut Vec::new(), &mut projections);
        if projections.is_empty() {
            projections.push(ValueProjection {
                path: Vec::new(),
                value,
            });
        }
        CallArgInfo {
            value,
            base_value,
            base_path,
            static_string: self.resolver.static_string_expr(expr),
            object_keys: self.resolver.object_keys_expr(expr),
            rooted_chain: self.resolver.rooted_expr_chain(expr),
            projections,
            spread: false,
        }
    }

    pub(super) fn expression_projection(&self, expr: &Expr) -> (ValueId, Vec<ProjectionSegment>) {
        match expr {
            Expr::Member(member) => {
                let (base, mut path) = self.expression_projection(&member.obj);
                let Some(property) = member_prop_name(&member.prop) else {
                    return (ValueId::UNKNOWN, Vec::new());
                };
                if let Ok(index) = property.parse::<usize>() {
                    path.push(ProjectionSegment::Index(index));
                } else {
                    path.push(ProjectionSegment::Property(property));
                }
                (base, path)
            }
            Expr::Paren(paren) => self.expression_projection(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .map_or((self.resolver.resolve_expr(expr).id, Vec::new()), |last| {
                    self.expression_projection(last)
                }),
            _ => (self.resolver.resolve_expr(expr).id, Vec::new()),
        }
    }

    pub(super) fn collect_value_projections(
        &self,
        expr: &Expr,
        path: &mut Vec<ProjectionSegment>,
        output: &mut Vec<ValueProjection>,
    ) {
        output.push(ValueProjection {
            path: path.clone(),
            value: self.resolver.resolve_expr(expr).id,
        });
        match expr {
            Expr::Object(object) => {
                for property in &object.props {
                    let swc_ecma_ast::PropOrSpread::Prop(property) = property else {
                        continue;
                    };
                    let swc_ecma_ast::Prop::KeyValue(property) = &**property else {
                        continue;
                    };
                    let Some(name) = crate::analysis::syntax::prop_name(&property.key) else {
                        continue;
                    };
                    path.push(ProjectionSegment::Property(name));
                    self.collect_value_projections(&property.value, path, output);
                    path.pop();
                }
            }
            Expr::Array(array) => {
                for (index, element) in array.elems.iter().enumerate() {
                    let Some(element) = element else { continue };
                    path.push(ProjectionSegment::Index(index));
                    self.collect_value_projections(&element.expr, path, output);
                    path.pop();
                }
            }
            Expr::Paren(paren) => self.collect_value_projections(&paren.expr, path, output),
            Expr::Seq(sequence) => {
                if let Some(last) = sequence.exprs.last() {
                    self.collect_value_projections(last, path, output);
                }
            }
            _ => {}
        }
    }

    pub(super) fn bound_arg_info(&self, argument: &BoundArgument) -> CallArgInfo {
        match argument {
            BoundArgument::StaticString(value) => CallArgInfo {
                value: ValueId::UNKNOWN,
                base_value: ValueId::UNKNOWN,
                base_path: Vec::new(),
                static_string: Some(value.clone()),
                object_keys: None,
                rooted_chain: None,
                projections: vec![ValueProjection {
                    path: Vec::new(),
                    value: ValueId::UNKNOWN,
                }],
                spread: false,
            },
            BoundArgument::RootedExpression(chain) => CallArgInfo {
                value: ValueId::UNKNOWN,
                base_value: ValueId::UNKNOWN,
                base_path: Vec::new(),
                static_string: None,
                object_keys: None,
                rooted_chain: Some(chain.to_string()),
                projections: vec![ValueProjection {
                    path: Vec::new(),
                    value: ValueId::UNKNOWN,
                }],
                spread: false,
            },
        }
    }

    pub(super) fn args_info(&self, args: &[ExprOrSpread]) -> Vec<CallArgInfo> {
        args.iter()
            .map(|arg| {
                let mut info = self.arg_info(&arg.expr);
                info.spread = arg.spread.is_some();
                if info.spread {
                    info.projections.clear();
                }
                info
            })
            .collect()
    }

    pub(super) fn resolve_target_chain(&self, target: &Expr) -> Option<String> {
        use crate::analysis::syntax::effective_callee_expr;
        let effective = effective_callee_expr(target);
        match effective {
            Expr::Ident(ident) => self
                .resolver
                .resolve_ident(ident)
                .rooted_chain
                .clone()
                .or_else(|| Some(ident.sym.to_string())),
            Expr::Member(member) => self.resolver.resolve_member(member).rooted_chain.clone(),
            _ => self.resolver.rooted_expr_chain(effective),
        }
    }

    pub(super) fn receiver_chain(&self, expr: &Expr) -> Option<String> {
        use crate::analysis::syntax::effective_callee_expr;
        let effective = effective_callee_expr(expr);
        match effective {
            Expr::Ident(ident) => self
                .resolver
                .resolve_ident(ident)
                .rooted_chain
                .clone()
                .or_else(|| Some(ident.sym.to_string())),
            Expr::Member(member) => self.resolver.resolve_member(member).rooted_chain.clone(),
            _ => self.resolver.rooted_expr_chain(effective),
        }
    }
}
