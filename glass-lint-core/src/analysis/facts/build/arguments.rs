//! Argument projections used by call facts and interprocedural flow.

use glass_lint_datastructures::SymbolPath;

use crate::analysis::{
    facts::build::{
        BoundArgument, CallArgInfo, Expr, ExprOrSpread, FactBuilder, PathId, PathSegmentInput,
        ValueId, member_property_name,
    },
    syntax::constant as syntax_constant,
    value::Value,
};

impl FactBuilder<'_> {
    /// Resolve one argument into the scalar, rooted, and statically addressable
    /// views consumed by call matchers and parameter-path flow.
    ///
    /// One bounded traversal constructs the value identity, member-chain
    /// projection, and static object/array shapes.  Constant evaluation is
    /// consulted only as a fallback when the resolver cannot produce a string
    /// identity (template literals, concatenation, etc.).
    ///
    /// Object and array literals are walked by this method directly rather
    /// than through the resolver because the resolver's constant-evaluation
    /// path (`syntax_constant::evaluate`) converts runtime value identities to
    /// `Unknown`; the direct walk preserves the resolved `ValueId` for every
    /// child expression.
    pub(super) fn arg_info(&mut self, expr: &Expr) -> CallArgInfo {
        match expr {
            Expr::Member(member) => {
                let resolved = self.resolver.resolve_member(member);
                let value = Self::resolve_or_eval(expr, resolved.id, self.resolver);
                let (base_value, base_path) = self.member_chain_projection(expr);
                CallArgInfo {
                    value,
                    base_value,
                    base_path,
                    spread: false,
                    provenance: resolved.call.clone(),
                }
            }
            Expr::Object(_) | Expr::Array(_) => {
                let (value, base_value, base_path) =
                    self.analyze_argument_tree(expr, PathId::EMPTY);
                CallArgInfo {
                    value,
                    base_value,
                    base_path,
                    spread: false,
                    provenance: crate::analysis::syntax::SymbolCallProvenance::Local,
                }
            }
            Expr::Paren(paren) => self.arg_info(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .map_or_else(CallArgInfo::unknown, |last| self.arg_info(last)),
            _ => {
                let resolved = self.resolver.resolve_expr(expr);
                let value = Self::resolve_or_eval(expr, resolved.id, self.resolver);
                CallArgInfo {
                    value,
                    base_value: value,
                    base_path: PathId::EMPTY,
                    spread: false,
                    provenance: resolved.call.clone(),
                }
            }
        }
    }

    fn resolve_or_eval(
        expr: &Expr,
        value: ValueId,
        resolver: &mut crate::analysis::resolution::Resolver,
    ) -> ValueId {
        if value == ValueId::UNKNOWN {
            let const_value = syntax_constant::evaluate(expr, resolver);
            if let Some(s) = const_value.string() {
                return resolver.static_value(Value::StaticString(s.to_owned())).id;
            }
        }
        value
    }

    /// Compute the base projection for a member chain.
    ///
    /// For `a.b.c`, returns the value of `a` and path `["b", "c"]`
    /// by walking the member chain to the deepest non-member identity.
    fn member_chain_projection(&mut self, expr: &Expr) -> (ValueId, PathId) {
        match expr {
            Expr::Member(member) => {
                let (base_val, base_path) = self.member_chain_projection(&member.obj);
                let Some(property) = member_property_name(&member.prop) else {
                    return (ValueId::UNKNOWN, PathId::EMPTY);
                };
                let extended = if let Ok(index) = property.parse::<usize>() {
                    let Ok(index) = u32::try_from(index) else {
                        return (ValueId::UNKNOWN, PathId::EMPTY);
                    };
                    self.append_path(base_path, PathSegmentInput::Index(index))
                } else {
                    self.append_path(base_path, PathSegmentInput::Property(property.as_str()))
                };
                (base_val, extended)
            }
            Expr::Paren(paren) => self.member_chain_projection(&paren.expr),
            Expr::Seq(sequence) => sequence.exprs.last().map_or_else(
                || (ValueId::UNKNOWN, PathId::EMPTY),
                |last| self.member_chain_projection(last),
            ),
            _ => {
                let value = self.resolver.resolve_expr_id(expr);
                (value, PathId::EMPTY)
            }
        }
    }

    /// Walk an object or array literal, resolving every child via the
    /// resolver's identity query and producing one `StaticObject` or
    /// `StaticArray` value that preserves runtime `ValueId` for every
    /// descendant.
    ///
    /// This is the sole traversal for object/array argument expressions; the
    /// resolver's constant-evaluation path is intentionally not consulted so
    /// that non-constant children (variables, calls, etc.) keep their arena
    /// identity.
    fn analyze_argument_tree(&mut self, expr: &Expr, path: PathId) -> (ValueId, ValueId, PathId) {
        match expr {
            Expr::Object(object) => {
                let mut entries = Vec::new();
                for property in &object.props {
                    let swc_ecma_ast::PropOrSpread::Prop(property) = property else {
                        let value = self.resolver.resolve_expr_id(expr);
                        return (value, value, path);
                    };
                    let swc_ecma_ast::Prop::KeyValue(property) = &**property else {
                        let value = self.resolver.resolve_expr_id(expr);
                        return (value, value, path);
                    };
                    let Some(name) = crate::analysis::syntax::property_name(&property.key) else {
                        let value = self.resolver.resolve_expr_id(expr);
                        return (value, value, path);
                    };
                    let child_path =
                        self.append_path(path, PathSegmentInput::Property(name.as_str()));
                    let (child_value, _, _) =
                        self.analyze_argument_tree(&property.value, child_path);
                    let Some(name) = self.intern_name(Some(name.as_str())) else {
                        return (ValueId::UNKNOWN, ValueId::UNKNOWN, path);
                    };
                    entries.push((name, child_value));
                }
                let value = self.resolver.static_value(Value::StaticObject(entries)).id;
                (value, value, path)
            }
            Expr::Array(array) => {
                let mut elements = Vec::with_capacity(array.elems.len());
                for (index, element) in array.elems.iter().enumerate() {
                    let Some(element) = element else {
                        elements.push(ValueId::UNKNOWN);
                        continue;
                    };
                    let Ok(index) = u32::try_from(index) else {
                        return (ValueId::UNKNOWN, ValueId::UNKNOWN, path);
                    };
                    let child_path = self.append_path(path, PathSegmentInput::Index(index));
                    let (child_value, _, _) = self.analyze_argument_tree(&element.expr, child_path);
                    elements.push(child_value);
                }
                let value = self.resolver.static_value(Value::StaticArray(elements)).id;
                (value, value, path)
            }
            Expr::Member(member) => {
                let value = self.resolver.resolve_expr_id(expr);
                let (base_value, base_path) = self.member_chain_projection(&member.obj);
                let property = member_property_name(&member.prop);
                let extended = match property {
                    Some(p) if let Ok(index) = p.parse::<usize>() => match u32::try_from(index) {
                        Ok(index) => self.append_path(base_path, PathSegmentInput::Index(index)),
                        Err(_) => return (value, value, path),
                    },
                    Some(p) => self.append_path(base_path, PathSegmentInput::Property(p.as_str())),
                    None => return (value, value, path),
                };
                (value, base_value, extended)
            }
            Expr::Paren(paren) => self.analyze_argument_tree(&paren.expr, path),
            Expr::Seq(sequence) => {
                if let Some(last) = sequence.exprs.last() {
                    self.analyze_argument_tree(last, path)
                } else {
                    let value = self.resolver.resolve_expr_id(expr);
                    (value, value, path)
                }
            }
            _ => {
                let value = self.resolver.resolve_expr_id(expr);
                (value, value, path)
            }
        }
    }

    /// Convert an argument already bound by a callable wrapper into the same
    /// representation as a source-level argument.
    pub(super) fn bound_arg_info(&mut self, argument: &BoundArgument) -> CallArgInfo {
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
    pub(super) fn resolve_target_chain(&mut self, target: &Expr) -> Option<SymbolPath> {
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
