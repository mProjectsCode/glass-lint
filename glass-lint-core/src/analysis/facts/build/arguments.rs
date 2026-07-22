//! Argument projections used by call facts and interprocedural flow.
//!
//! Projections preserve both the whole argument and statically addressable
//! descendants; dynamic keys are intentionally represented as unknown.

use crate::analysis::{
    SymbolPath,
    facts::build::{
        BoundArgument, CallArgInfo, Expr, ExprOrSpread, FactBuilder, PathId, PathSegmentInput,
        ValueId, member_property_name,
    },
    syntax::constant as syntax_constant,
    value::Value,
};

/// The one result produced for each argument walk. The expression value is
/// retained in the frozen arena; the base pair is the only extra information
/// needed to connect a member/parameter path during flow projection.
struct ArgumentProjection {
    value: ValueId,
    base_value: ValueId,
    base_path: PathId,
}

impl ArgumentProjection {
    fn from_value(value: ValueId, path: PathId) -> Self {
        Self {
            value,
            base_value: value,
            base_path: path,
        }
    }

    fn unknown() -> Self {
        Self::from_value(ValueId::UNKNOWN, PathId::EMPTY)
    }
}

impl FactBuilder<'_> {
    /// Resolve one argument into the scalar, rooted, and statically addressable
    /// views consumed by call matchers and parameter-path flow.
    ///
    /// One bounded resolution and one constant evaluation are performed; every
    /// derived view (projections, keys, strings, provenance, rooted chain)
    /// comes from the same two sources under one budget outcome.
    pub(super) fn arg_info(&mut self, expr: &Expr) -> CallArgInfo {
        let resolved = self.resolver.resolve_expr(expr);
        let mut value = resolved.id;
        let provenance = resolved.call.clone();

        // Template literals and other expressions that the resolver does not
        // intern as static strings are evaluated here and interned into the
        // value table so frozen-table lookups find them.
        if value == ValueId::UNKNOWN {
            let const_value = syntax_constant::evaluate(expr, self.resolver);
            if let Some(s) = const_value.string() {
                let static_val = self
                    .resolver
                    .static_value(Value::StaticString(s.to_owned()));
                value = static_val.id;
            }
        }

        let projection = self.walk_argument_projections(expr, PathId::EMPTY, Some(value));

        CallArgInfo {
            value: projection.value,
            base_value: projection.base_value,
            base_path: projection.base_path,
            spread: false,
            provenance,
        }
    }

    /// Unified walk that simultaneously collects every descendant projection
    /// and the outermost member-chain base projection in one bounded traversal.
    ///
    /// Returns `(base_value, base_path)` for the deepest non-member identity
    /// (e.g., for `a.b.c` returns the identity of `a` and path `["b", "c"]`).
    fn walk_argument_projections(
        &mut self,
        expr: &Expr,
        path: PathId,
        known_value: Option<ValueId>,
    ) -> ArgumentProjection {
        match expr {
            Expr::Member(member) => {
                let value = known_value.unwrap_or_else(|| self.resolver.resolve_expr_id(expr));
                let base = self.walk_argument_projections(&member.obj, path, None);

                let Some(property) = member_property_name(&member.prop) else {
                    return ArgumentProjection::unknown();
                };
                let extended = if let Ok(index) = property.parse::<usize>() {
                    let Ok(index) = u32::try_from(index) else {
                        return ArgumentProjection::unknown();
                    };
                    self.append_path(base.base_path, PathSegmentInput::Index(index))
                } else {
                    self.append_path(
                        base.base_path,
                        PathSegmentInput::Property(property.as_str()),
                    )
                };
                ArgumentProjection {
                    value,
                    base_value: base.base_value,
                    base_path: extended,
                }
            }
            Expr::Object(object) => {
                let mut entries = Vec::new();
                for property in &object.props {
                    let swc_ecma_ast::PropOrSpread::Prop(property) = property else {
                        return ArgumentProjection::from_value(
                            known_value.unwrap_or_else(|| self.resolver.resolve_expr_id(expr)),
                            path,
                        );
                    };
                    let swc_ecma_ast::Prop::KeyValue(property) = &**property else {
                        return ArgumentProjection::from_value(
                            known_value.unwrap_or_else(|| self.resolver.resolve_expr_id(expr)),
                            path,
                        );
                    };
                    let Some(name) = crate::analysis::syntax::property_name(&property.key) else {
                        return ArgumentProjection::from_value(
                            known_value.unwrap_or_else(|| self.resolver.resolve_expr_id(expr)),
                            path,
                        );
                    };
                    let child_path =
                        self.append_path(path, PathSegmentInput::Property(name.as_str()));
                    let child = self.walk_argument_projections(&property.value, child_path, None);
                    let Some(name) = self.intern_name(Some(name.as_str())) else {
                        return ArgumentProjection::unknown();
                    };
                    entries.push((name, child.value));
                }
                let value = self.resolver.static_value(Value::StaticObject(entries)).id;
                ArgumentProjection::from_value(value, path)
            }
            Expr::Array(array) => {
                let mut elements = Vec::with_capacity(array.elems.len());
                for (index, element) in array.elems.iter().enumerate() {
                    let Some(element) = element else {
                        elements.push(ValueId::UNKNOWN);
                        continue;
                    };
                    let Ok(index) = u32::try_from(index) else {
                        return ArgumentProjection::unknown();
                    };
                    let child_path = self.append_path(path, PathSegmentInput::Index(index));
                    elements.push(
                        self.walk_argument_projections(&element.expr, child_path, None)
                            .value,
                    );
                }
                let value = self.resolver.static_value(Value::StaticArray(elements)).id;
                ArgumentProjection::from_value(value, path)
            }
            Expr::Paren(paren) => self.walk_argument_projections(&paren.expr, path, known_value),
            Expr::Seq(sequence) => {
                if let Some(last) = sequence.exprs.last() {
                    self.walk_argument_projections(last, path, known_value)
                } else {
                    let value = known_value.unwrap_or_else(|| self.resolver.resolve_expr_id(expr));
                    ArgumentProjection::from_value(value, path)
                }
            }
            _ => {
                let value = known_value.unwrap_or_else(|| self.resolver.resolve_expr_id(expr));
                ArgumentProjection::from_value(value, path)
            }
        }
    }

    /// Convert an argument already bound by a callable wrapper into the same
    /// representation as a source-level argument.
    pub(super) fn bound_arg_info(&self, argument: &BoundArgument) -> CallArgInfo {
        match argument {
            BoundArgument::StaticString(value) => {
                let resolved = self
                    .resolver
                    .static_value(crate::analysis::value::Value::StaticString(value.clone()));
                CallArgInfo {
                    value: resolved.id,
                    ..CallArgInfo::unknown()
                }
            }
            BoundArgument::RootedExpression(chain) => {
                let resolved =
                    self.resolver
                        .static_value(crate::analysis::value::Value::RootedMember {
                            path: chain.clone(),
                        });
                CallArgInfo {
                    value: resolved.id,
                    ..CallArgInfo::unknown()
                }
            }
        }
    }

    /// Resolve positional arguments.
    pub(super) fn args_info(&mut self, args: &[ExprOrSpread]) -> Vec<CallArgInfo> {
        args.iter()
            .map(|arg| {
                let mut info = self.arg_info(&arg.expr);
                info.spread = arg.spread.is_some();
                info
            })
            .collect()
    }

    /// Resolve the rooted identity of a call target without treating a raw
    /// local name as stronger provenance than the resolver can prove.
    pub(super) fn resolve_target_chain(&self, target: &Expr) -> Option<SymbolPath> {
        use crate::analysis::syntax::effective_callee_expr;
        let effective = effective_callee_expr(target);
        match effective {
            Expr::Ident(ident) => self
                .resolver
                .resolve_ident(ident)
                .rooted_chain
                .clone()
                .or_else(|| Some(SymbolPath::from(ident.sym.as_ref()))),
            Expr::Member(member) => self.resolver.resolve_member(member).rooted_chain.clone(),
            _ => self.resolver.rooted_expr_chain(effective),
        }
    }
}
