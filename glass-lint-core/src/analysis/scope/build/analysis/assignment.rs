use swc_ecma_ast::{Expr, VarDeclKind};

use crate::analysis::scope::{BindingProvenance, build::ScopeCollector};

pub fn expression_is_mutable_static_object(
    collector: &mut ScopeCollector,
    expr: &Expr,
    kind: VarDeclKind,
) -> bool {
    if kind != VarDeclKind::Var {
        return false;
    }
    matches!(
        collector
            .static_object_values(expr)
            .or_else(|| collector.const_provenance(expr)),
        Some(BindingProvenance::StaticObjectKeys(_) | BindingProvenance::StaticObjectValues(_))
    )
}

pub fn assignment_provenance(collector: &mut ScopeCollector, expr: &Expr) -> BindingProvenance {
    collector
        .constructed_instance_provenance(expr)
        .or_else(|| collector.bound_callable_provenance(expr))
        .or_else(|| collector.module_alias_provenance(expr))
        .or_else(|| collector.returned_object_provenance(expr))
        .or_else(|| collector.const_provenance(expr))
        .or_else(|| {
            collector
                .rooted_name_path(expr)
                .map(|target| BindingProvenance::ValueAlias { target })
        })
        .unwrap_or(BindingProvenance::Local)
}
