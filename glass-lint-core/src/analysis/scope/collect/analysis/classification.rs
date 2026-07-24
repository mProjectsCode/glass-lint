use glass_lint_datastructures::NamePath;
use smol_str::SmolStr;
use swc_ecma_ast::{Callee, Expr, Pat};

use crate::analysis::{
    scope::{BindingProvenance, collect::ScopeCollector},
    syntax::member_property_name,
};

pub enum DeclarationClassification {
    Binding {
        name: String,
        provenance: BindingProvenance,
    },
    Require {
        module: SmolStr,
    },
    ValueAlias {
        target: NamePath,
    },
    None,
}

impl std::fmt::Debug for DeclarationClassification {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Binding { name, provenance } => formatter
                .debug_struct("Binding")
                .field("name", name)
                .field("provenance", provenance)
                .finish(),
            Self::Require { module } => formatter
                .debug_struct("Require")
                .field("module", module)
                .finish(),
            Self::ValueAlias { target } => formatter
                .debug_struct("ValueAlias")
                .field("target", target)
                .finish(),
            Self::None => formatter.write_str("None"),
        }
    }
}

pub fn classify_declaration(
    collector: &ScopeCollector,
    expr: &Expr,
    pat: &Pat,
    derived_function_pattern: bool,
) -> DeclarationClassification {
    // Match over Expr variants dispatching to per-shape helpers. The outer
    // match stays in one function so the const-provenance fallback at the
    // bottom catches every variant uniformly.
    let name = match pat {
        Pat::Ident(ident) => Some(ident.id.sym.to_string()),
        _ => None,
    };

    match expr {
        Expr::Lit(_) => {
            if let (Some(name), Some(provenance)) = (name, collector.const_provenance(expr)) {
                return DeclarationClassification::Binding { name, provenance };
            }
            DeclarationClassification::None
        }
        Expr::Call(call) => klassify_call(collector, call, expr, name, derived_function_pattern),
        Expr::Member(_) => klassify_member(collector, expr, name, derived_function_pattern),
        Expr::Object(_) | Expr::Array(_) => {
            if let (Some(name), Some(provenance)) = (
                name,
                collector
                    .static_object_values(expr)
                    .or_else(|| collector.const_provenance(expr)),
            ) {
                return DeclarationClassification::Binding { name, provenance };
            }
            DeclarationClassification::None
        }
        Expr::Ident(_) => klassify_ident(collector, expr, name, derived_function_pattern),

        Expr::Await(await_expr) => {
            classify_declaration(collector, &await_expr.arg, pat, derived_function_pattern)
        }
        Expr::Paren(paren) => {
            classify_declaration(collector, &paren.expr, pat, derived_function_pattern)
        }
        Expr::Seq(seq) => seq
            .exprs
            .last()
            .map_or(DeclarationClassification::None, |last| {
                classify_declaration(collector, last, pat, derived_function_pattern)
            }),

        _ => {
            if let (Some(name), Some(provenance)) = (name, collector.const_provenance(expr)) {
                return DeclarationClassification::Binding { name, provenance };
            }
            if !derived_function_pattern && let Some(target) = collector.rooted_name_path(expr) {
                return DeclarationClassification::ValueAlias { target };
            }
            DeclarationClassification::None
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
fn klassify_call(
    collector: &ScopeCollector,
    call: &swc_ecma_ast::CallExpr,
    expr: &Expr,
    name: Option<String>,
    derived_function_pattern: bool,
) -> DeclarationClassification {
    if let Some(module) = collector.require_module_expr_name(expr) {
        return DeclarationClassification::Require { module };
    }

    if callee_is_bind_call(call) {
        if let Some(ref name) = name
            && let Some(provenance) = collector.bound_callable_provenance(expr)
        {
            return DeclarationClassification::Binding {
                name: name.clone(),
                provenance,
            };
        }
        return DeclarationClassification::None;
    }

    if let Some(provenance) = collector.bound_callable_provenance(expr)
        && let Some(name) = name
    {
        return DeclarationClassification::Binding { name, provenance };
    }

    if let Some(provenance) = collector.module_alias_provenance(expr) {
        if let Some(name) = name.clone() {
            return DeclarationClassification::Binding { name, provenance };
        }
        if let BindingProvenance::ModuleNamespace { module } = provenance {
            return DeclarationClassification::Require { module };
        }
    }

    if let (Some(name), Some(provenance)) = (name.clone(), collector.const_provenance(expr)) {
        return DeclarationClassification::Binding { name, provenance };
    }

    if let Some(ref n) = name
        && let Some(provenance) = collector.returned_object_provenance(expr)
    {
        let rooted_path = collector.rooted_name_path(expr);
        if rooted_path.as_ref().is_none_or(|target| !target.is_root()) {
            return DeclarationClassification::Binding {
                name: n.clone(),
                provenance,
            };
        }
    }

    if !derived_function_pattern && let Some(target) = collector.rooted_name_path(expr) {
        return DeclarationClassification::ValueAlias { target };
    }

    DeclarationClassification::None
}

#[allow(clippy::needless_pass_by_value)]
fn klassify_member(
    collector: &ScopeCollector,
    expr: &Expr,
    name: Option<String>,
    derived_function_pattern: bool,
) -> DeclarationClassification {
    if let Some(provenance) = collector.module_alias_provenance(expr) {
        if let Some(name) = name.clone() {
            return DeclarationClassification::Binding { name, provenance };
        }
        if let BindingProvenance::ModuleNamespace { module } = provenance {
            return DeclarationClassification::Require { module };
        }
    }

    if let Some(module) = collector.require_module_expr_name(expr) {
        return DeclarationClassification::Require { module };
    }

    let rooted_path = collector.rooted_name_path(expr);
    if rooted_path.as_ref().is_none_or(|target| !target.is_root())
        && let Some(ref n) = name
        && let Some(provenance) = collector.returned_object_provenance(expr)
    {
        return DeclarationClassification::Binding {
            name: n.clone(),
            provenance,
        };
    }

    if !derived_function_pattern && let Some(target) = rooted_path {
        return DeclarationClassification::ValueAlias { target };
    }

    DeclarationClassification::None
}

#[allow(clippy::needless_pass_by_value)]
fn klassify_ident(
    collector: &ScopeCollector,
    expr: &Expr,
    mut name: Option<String>,
    derived_function_pattern: bool,
) -> DeclarationClassification {
    if let Some(provenance) = collector.module_alias_provenance(expr) {
        if let Some(name) = name.clone() {
            return DeclarationClassification::Binding { name, provenance };
        }
        if let BindingProvenance::ModuleNamespace { module } = provenance {
            return DeclarationClassification::Require { module };
        }
    }

    if let Some(ref n) = name
        && let Some(provenance) = collector.returned_object_provenance(expr)
    {
        let rooted_path = collector.rooted_name_path(expr);
        if rooted_path.as_ref().is_none_or(|target| !target.is_root()) {
            return DeclarationClassification::Binding {
                name: n.clone(),
                provenance,
            };
        }
    }

    if let Some(n) = name.take()
        && let Some(provenance) = collector.const_provenance(expr)
    {
        return DeclarationClassification::Binding {
            name: n,
            provenance,
        };
    }

    if !derived_function_pattern && let Some(target) = collector.rooted_name_path(expr) {
        return DeclarationClassification::ValueAlias { target };
    }

    DeclarationClassification::None
}

fn callee_is_bind_call(call: &swc_ecma_ast::CallExpr) -> bool {
    matches!(&call.callee, Callee::Expr(callee) if matches!(
        &**callee,
        Expr::Member(member) if member_property_name(&member.prop).as_deref() == Some("bind")
    ))
}
