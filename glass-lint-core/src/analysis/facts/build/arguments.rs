use super::{
    BoundArgument, CallArgInfo, Expr, ExprOrSpread, FactBuilder, PathId, PathSegment, ValueId,
    ValueProjection, member_prop_name,
};

impl FactBuilder<'_> {
    pub(super) fn arg_info(&mut self, expr: &Expr) -> CallArgInfo {
        let value = self.resolver.resolve_expr(expr).id;
        let (base_value, base_path) = self.expression_projection(expr);
        let mut projections = Vec::new();
        self.collect_value_projections(expr, PathId::EMPTY, &mut projections);
        if projections.is_empty() {
            projections.push(ValueProjection {
                path: PathId::EMPTY,
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

    pub(super) fn expression_projection(&mut self, expr: &Expr) -> (ValueId, PathId) {
        match expr {
            Expr::Member(member) => {
                let (base, path) = self.expression_projection(&member.obj);
                let Some(property) = member_prop_name(&member.prop) else {
                    return (ValueId::UNKNOWN, PathId::EMPTY);
                };
                let path = if let Ok(index) = property.parse::<usize>() {
                    let Ok(index) = u32::try_from(index) else {
                        return (ValueId::UNKNOWN, PathId::EMPTY);
                    };
                    self.append_path(path, PathSegment::Index(index))
                } else {
                    self.append_path(path, PathSegment::Property(property))
                };
                (base, path)
            }
            Expr::Paren(paren) => self.expression_projection(&paren.expr),
            Expr::Seq(sequence) => sequence.exprs.last().map_or(
                (self.resolver.resolve_expr(expr).id, PathId::EMPTY),
                |last| self.expression_projection(last),
            ),
            _ => (self.resolver.resolve_expr(expr).id, PathId::EMPTY),
        }
    }

    pub(super) fn collect_value_projections(
        &mut self,
        expr: &Expr,
        path: PathId,
        output: &mut Vec<ValueProjection>,
    ) {
        output.push(ValueProjection {
            path,
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
                    let path = self.append_path(path, PathSegment::Property(name));
                    self.collect_value_projections(&property.value, path, output);
                }
            }
            Expr::Array(array) => {
                for (index, element) in array.elems.iter().enumerate() {
                    let Some(element) = element else { continue };
                    let Ok(index) = u32::try_from(index) else {
                        continue;
                    };
                    let path = self.append_path(path, PathSegment::Index(index));
                    self.collect_value_projections(&element.expr, path, output);
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

    pub(super) fn bound_arg_info(argument: &BoundArgument) -> CallArgInfo {
        match argument {
            BoundArgument::StaticString(value) => CallArgInfo {
                value: ValueId::UNKNOWN,
                base_value: ValueId::UNKNOWN,
                base_path: PathId::EMPTY,
                static_string: Some(value.clone()),
                object_keys: None,
                rooted_chain: None,
                projections: vec![ValueProjection {
                    path: PathId::EMPTY,
                    value: ValueId::UNKNOWN,
                }],
                spread: false,
            },
            BoundArgument::RootedExpression(chain) => CallArgInfo {
                value: ValueId::UNKNOWN,
                base_value: ValueId::UNKNOWN,
                base_path: PathId::EMPTY,
                static_string: None,
                object_keys: None,
                rooted_chain: Some(chain.to_string()),
                projections: vec![ValueProjection {
                    path: PathId::EMPTY,
                    value: ValueId::UNKNOWN,
                }],
                spread: false,
            },
        }
    }

    pub(super) fn args_info(&mut self, args: &[ExprOrSpread]) -> Vec<CallArgInfo> {
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
}
