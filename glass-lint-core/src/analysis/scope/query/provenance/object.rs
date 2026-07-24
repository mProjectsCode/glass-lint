use glass_lint_datastructures::SymbolPath;
use smol_str::{SmolStr, ToSmolStr};

use crate::analysis::scope::query::{
    BindingProvenance, Expr, FrozenScopeGraph, MemberExpr, MemberValueSeed,
};

impl FrozenScopeGraph {
    pub(in crate::analysis) fn member_value_seed(&self, member: &MemberExpr) -> MemberValueSeed {
        let syntactic_chain = self.member_expression_chain(member);
        let rooted_chain = syntactic_chain
            .as_ref()
            .and_then(|chain| self.resolve_member_chain(member, chain))
            .and_then(|path| self.name_path(&path));
        let module_member = syntactic_chain
            .as_ref()
            .and_then(|chain| self.member_call_provenance_for_chain(member, chain));
        let returned_member = self.returned_member(member);
        let binding = self
            .binding_key_for_expr(&member.obj)
            .or_else(|| self.global_key_for_expr(&member.obj))
            .and_then(|mut key| {
                key.append_segment(self.name_id(self.member_property_name(member)?.as_str())?);
                Some(key)
            });
        MemberValueSeed {
            syntactic_chain,
            rooted_chain,
            binding,
            module_member,
            returned_member,
        }
    }

    pub(super) fn module_member_for_member(
        &self,
        member: &MemberExpr,
    ) -> Option<(SmolStr, SmolStr)> {
        let mut members = vec![self.member_property_name(member)?];
        let (module, base) = self.collect_module_member(&member.obj, &mut members)?;
        Some((module, Self::join_module_member(base.as_ref(), &members)))
    }

    fn collect_module_member(
        &self,
        expr: &Expr,
        members: &mut Vec<SmolStr>,
    ) -> Option<(SmolStr, Option<SmolStr>)> {
        match expr {
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::ModuleExport { module, export } => {
                    Some((module.clone(), Some(export.clone())))
                }
                BindingProvenance::ModuleNamespace { module } => Some((module.clone(), None)),
                _ => None,
            },
            Expr::Member(member) => {
                members.push(self.member_property_name(member)?);
                self.collect_module_member(&member.obj, members)
            }
            Expr::Call(call) => {
                let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                let Expr::Ident(require) = &**callee else {
                    return None;
                };
                if require.sym != *"require"
                    || self
                        .binding_at(require.sym.as_ref(), require.span)
                        .is_some()
                {
                    return None;
                }
                let argument = call.args.first()?;
                let Expr::Lit(swc_ecma_ast::Lit::Str(module)) = &*argument.expr else {
                    return None;
                };
                Some((module.value.to_string_lossy().to_smolstr(), None))
            }
            Expr::Paren(paren) => self.collect_module_member(&paren.expr, members),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.collect_module_member(expr, members)),
            _ => None,
        }
    }

    fn join_module_member(base: Option<&SmolStr>, members: &[SmolStr]) -> SmolStr {
        let segment_count = members.len() + usize::from(base.is_some());
        let capacity = base.map_or(0, SmolStr::len)
            + members.iter().map(SmolStr::len).sum::<usize>()
            + segment_count.saturating_sub(1);
        let mut path = String::with_capacity(capacity);
        for segment in base.into_iter().chain(members.iter().rev()) {
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(segment);
        }
        path.into()
    }

    pub(in crate::analysis) fn returned_object_source(&self, expr: &Expr) -> Option<SymbolPath> {
        match expr {
            Expr::Call(call) => {
                let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                let source = self.rooted_expr_chain(callee)?;
                (!source.is_root()).then_some(source)
            }
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::ReturnedObject { source } => self.symbol_path(source),
                _ => None,
            },
            Expr::Member(member) => {
                if let Some(source) = self.returned_object_source(&member.obj) {
                    return Some(source);
                }
                self.rooted_member_chain(member)
            }
            Expr::Paren(paren) => self.returned_object_source(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.returned_object_source(expr)),
            _ => None,
        }
    }

    pub(in crate::analysis) fn returned_member(
        &self,
        member: &MemberExpr,
    ) -> Option<(
        glass_lint_datastructures::NamePath,
        glass_lint_datastructures::NamePath,
    )> {
        let source = self.returned_object_source(&member.obj)?;
        let property = self.member_property_name(member)?;
        Some((self.name_path(&source)?, self.name_path(&property.into())?))
    }
}
