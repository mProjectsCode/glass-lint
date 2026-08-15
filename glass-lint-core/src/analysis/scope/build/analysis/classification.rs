use glass_lint_datastructures::NamePath;
use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{Callee, Expr, Pat};

use crate::analysis::{
    module_request::ModuleRequestPolicy,
    scope::{BindingProvenance, build::ScopeCollector},
    syntax::literal_member_property_name,
};

pub enum DeclarationClassification {
    Binding {
        name: SmolStr,
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
    collector: &mut ScopeCollector,
    expr: &Expr,
    pat: &Pat,
    derived_function_pattern: bool,
) -> DeclarationClassification {
    let name = declaration_name(pat);

    match expr {
        Expr::Lit(_) => classify_candidates(
            collector,
            expr,
            name,
            derived_function_pattern,
            &[Candidate::Constant],
        ),
        Expr::Call(call) => classify_call(collector, call, expr, name, derived_function_pattern),
        Expr::OptChain(chain) if matches!(&*chain.base, swc_ecma_ast::OptChainBase::Call(_)) => {
            classify_candidates(
                collector,
                expr,
                name,
                derived_function_pattern,
                &[Candidate::ReturnedObject, Candidate::RootedAlias],
            )
        }
        Expr::Member(_) => classify_candidates(
            collector,
            expr,
            name,
            derived_function_pattern,
            &[
                Candidate::ModuleAlias,
                Candidate::Require,
                Candidate::ReturnedObject,
                Candidate::RootedAlias,
            ],
        ),
        Expr::Object(_) | Expr::Array(_) => classify_candidates(
            collector,
            expr,
            name,
            derived_function_pattern,
            &[Candidate::StaticObject, Candidate::Constant],
        ),
        Expr::Ident(_) => classify_candidates(
            collector,
            expr,
            name,
            derived_function_pattern,
            &[
                Candidate::ModuleAlias,
                Candidate::ReturnedObject,
                Candidate::Constant,
                Candidate::RootedAlias,
            ],
        ),

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

        _ => classify_candidates(
            collector,
            expr,
            name,
            derived_function_pattern,
            &[Candidate::Constant, Candidate::RootedAlias],
        ),
    }
}

#[derive(Clone, Copy)]
enum Candidate {
    BoundCallable,
    ModuleAlias,
    Require,
    Constant,
    StaticObject,
    ReturnedObject,
    RootedAlias,
}

fn declaration_name(pattern: &Pat) -> Option<&str> {
    match pattern {
        Pat::Ident(ident) => Some(ident.id.sym.as_ref()),
        _ => None,
    }
}

fn classify_call(
    collector: &mut ScopeCollector,
    call: &swc_ecma_ast::CallExpr,
    expr: &Expr,
    name: Option<&str>,
    derived_function_pattern: bool,
) -> DeclarationClassification {
    if let Some(module) = collector.module_request_name(expr, ModuleRequestPolicy::alias()) {
        return DeclarationClassification::Require { module };
    }

    if callee_is_bind_call(call) {
        return classify_candidates(
            collector,
            expr,
            name,
            derived_function_pattern,
            &[Candidate::BoundCallable],
        );
    }

    classify_candidates(
        collector,
        expr,
        name,
        derived_function_pattern,
        &[
            Candidate::BoundCallable,
            Candidate::ModuleAlias,
            Candidate::Constant,
            Candidate::ReturnedObject,
            Candidate::RootedAlias,
        ],
    )
}

fn classify_candidates(
    collector: &mut ScopeCollector,
    expr: &Expr,
    name: Option<&str>,
    derived_function_pattern: bool,
    candidates: &[Candidate],
) -> DeclarationClassification {
    for candidate in candidates {
        let classification = match candidate {
            Candidate::BoundCallable => collector
                .bound_callable_provenance(expr)
                .and_then(|provenance| binding(name, provenance)),
            Candidate::ModuleAlias => collector
                .module_alias_provenance(expr)
                .and_then(|provenance| module_alias(name, provenance)),
            Candidate::Require => collector
                .module_request_name(expr, ModuleRequestPolicy::alias())
                .map(|module| DeclarationClassification::Require { module }),
            Candidate::Constant => collector
                .const_provenance(expr)
                .and_then(|provenance| binding(name, provenance)),
            Candidate::StaticObject => collector
                .static_object_values(expr)
                .or_else(|| collector.const_provenance(expr))
                .and_then(|provenance| binding(name, provenance)),
            Candidate::ReturnedObject => {
                let rooted_path = collector.rooted_name_path(expr);
                (rooted_path.as_ref().is_none_or(|target| !target.is_root()))
                    .then(|| collector.returned_object_provenance(expr))
                    .flatten()
                    .and_then(|provenance| binding(name, provenance))
            }
            Candidate::RootedAlias if !derived_function_pattern => collector
                .rooted_name_path(expr)
                .map(|target| DeclarationClassification::ValueAlias { target }),
            Candidate::RootedAlias => None,
        };
        if let Some(classification) = classification {
            return classification;
        }
    }

    DeclarationClassification::None
}

fn binding(name: Option<&str>, provenance: BindingProvenance) -> Option<DeclarationClassification> {
    name.map_or_else(
        || None,
        |name| {
            Some(DeclarationClassification::Binding {
                name: name.to_smolstr(),
                provenance,
            })
        },
    )
}

fn module_alias(
    name: Option<&str>,
    provenance: BindingProvenance,
) -> Option<DeclarationClassification> {
    if let Some(classification) = binding(name, provenance.clone()) {
        return Some(classification);
    }
    match provenance {
        BindingProvenance::ModuleNamespace { module }
        | BindingProvenance::DefaultImport { module } => {
            Some(DeclarationClassification::Require { module })
        }
        _ => None,
    }
}

fn callee_is_bind_call(call: &swc_ecma_ast::CallExpr) -> bool {
    matches!(&call.callee, Callee::Expr(callee) if matches!(
        &**callee,
        Expr::Member(member)
            if literal_member_property_name(&member.prop).as_deref() == Some("bind")
    ))
}
