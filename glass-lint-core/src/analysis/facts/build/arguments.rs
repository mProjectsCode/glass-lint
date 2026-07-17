//! Argument projections used by call facts and interprocedural flow.
//!
//! Projections preserve both the whole argument and statically addressable
//! descendants; dynamic keys are intentionally represented as unknown.

use super::{
    BoundArgument, CallArgInfo, Expr, ExprOrSpread, FactBuilder, PathId, PathSegment, ValueId,
    ValueProjection, member_property_name,
};

impl FactBuilder<'_> {
    /// Resolve one argument into the scalar, rooted, and statically addressable
    /// views consumed by call matchers and parameter-path flow.
    pub(super) fn arg_info(&mut self, expr: &Expr) -> CallArgInfo {
        let value = self.resolver.resolve_expr(expr).id;
        let provenance = self.resolver.resolve_expr(expr).call;
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
            provenance,
        }
    }

    /// Return the value identity and static property path represented by an
    /// expression. A computed or otherwise unprovable key invalidates the
    /// projection instead of guessing which property was read.
    pub(super) fn expression_projection(&mut self, expr: &Expr) -> (ValueId, PathId) {
        match expr {
            Expr::Member(member) => {
                let (base, path) = self.expression_projection(&member.obj);
                let Some(property) = member_property_name(&member.prop) else {
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
            Expr::Seq(sequence) => sequence.exprs.last().map_or_else(
                || (self.resolver.resolve_expr(expr).id, PathId::EMPTY),
                |last| self.expression_projection(last),
            ),
            _ => (self.resolver.resolve_expr(expr).id, PathId::EMPTY),
        }
    }

    /// Flatten literal object and array descendants into bounded path/value
    /// projections while retaining the root projection at every level.
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
                    let Some(name) = crate::analysis::syntax::property_name(&property.key) else {
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

    /// Convert an argument already bound by a callable wrapper into the same
    /// representation as a source-level argument.
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
                provenance: crate::analysis::syntax::SymbolCallProvenance::Local,
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
                provenance: crate::analysis::syntax::SymbolCallProvenance::Local,
            },
        }
    }

    /// Resolve positional arguments, clearing their shape when a spread makes
    /// the number and ownership of downstream arguments unknowable.
    pub(super) fn args_info(&mut self, args: &[ExprOrSpread]) -> Vec<CallArgInfo> {
        // A spread has no bounded positional shape, so retaining projections
        // would falsely connect an individual property to a later parameter.
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

    /// Resolve the rooted identity of a call target without treating a raw
    /// local name as stronger provenance than the resolver can prove.
    pub(super) fn resolve_target_chain(&self, target: &Expr) -> Option<String> {
        use crate::analysis::syntax::effective_callee_expr;
        let effective = effective_callee_expr(target);
        match effective {
            Expr::Ident(ident) => self
                .resolver
                .resolve_ident(ident)
                .rooted_chain
                .or_else(|| Some(ident.sym.to_string())),
            Expr::Member(member) => self.resolver.resolve_member(member).rooted_chain,
            _ => self.resolver.rooted_expr_chain(effective),
        }
    }
}
