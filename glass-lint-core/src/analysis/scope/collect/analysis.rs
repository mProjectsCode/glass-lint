//! Syntax-directed declaration classification, assignment provenance, and
//! mutability checks.
//!
//! Each function inspects the expression shape to run only the relevant
//! semantic analyses, avoiding the eager seven-analysis walk of the previous
//! [`DeclarationFacts`] facade.

use glass_lint_datastructures::NamePath;
use smol_str::SmolStr;
use swc_ecma_ast::{Callee, Expr, Pat, VarDeclKind};

use super::{BindingProvenance, ScopeCollector};
use crate::analysis::syntax::member_property_name;

pub(super) enum DeclarationClassification {
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

/// Classify a declaration whose initializer is `expr` and pattern is `pat`.
///
/// Syntax-directed dispatch: only analyses relevant to the expression shape are
/// run, in the following precedence order:
///
///   1. `bound_callable_provenance` — `.bind(…)` call
///   2. `module_alias_provenance` — module export / namespace ident
///   3. `require_module_expr_name` — direct `require("…")` / interop wrapper
///   4. `static_object_values` / `const_provenance` — literal object/array
///   5. `returned_object_provenance` — call result or member of one
///   6. `rooted_name_path` — value alias through scope chain
///   7. `None`
pub(super) fn classify_declaration(
    collector: &ScopeCollector,
    expr: &Expr,
    pat: &Pat,
    derived_function_pattern: bool,
) -> DeclarationClassification {
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

        // Unwrap wrapper expressions so inner expression shape drives dispatch.
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
            // Check const_provenance for template literals and other expressions
            // that constant::evaluate can handle.
            if let (Some(name), Some(provenance)) = (name, collector.const_provenance(expr)) {
                return DeclarationClassification::Binding { name, provenance };
            }
            // Fallback: check rooted_name_path for this-expressions and any other
            // expression shape that rooted_expr_chain_with can resolve.
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
    // Direct require("…") or interop wrapper: require analysis only.
    if let Some(module) = collector.require_module_expr_name(expr) {
        return DeclarationClassification::Require { module };
    }

    // .bind(…) call: bound-callable analysis only.
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

    // Other calls: full precedence.
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

/// Whether declaring `expr` with `kind` should be tracked as a mutable static
/// object. Only checks `static_object_values` and `const_provenance`, and only
/// when `kind == VarDeclKind::Var`.
pub(super) fn expression_is_mutable_static_object(
    collector: &ScopeCollector,
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

/// Determine the binding provenance of an assignment right-hand side.
///
/// Precedence: bound callable > module alias > returned object > constant value
/// > rooted alias > local. Each analysis runs only when the previous one fails.
pub(super) fn assignment_provenance(collector: &ScopeCollector, expr: &Expr) -> BindingProvenance {
    collector
        .bound_callable_provenance(expr)
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

/// Whether the callee of a call expression is a `.bind(...)` member call.
fn callee_is_bind_call(call: &swc_ecma_ast::CallExpr) -> bool {
    matches!(&call.callee, Callee::Expr(callee) if matches!(
        &**callee,
        Expr::Member(member) if member_property_name(&member.prop).as_deref() == Some("bind")
    ))
}

#[cfg(test)]
mod tests {
    use swc_common::Spanned;
    use swc_ecma_ast::{AssignExpr, Expr, Pat, VarDecl, VarDeclKind};
    use swc_ecma_visit::VisitWith;

    use super::*;
    use crate::analysis::scope::collect::{
        ScopeCollector, plan::ScopePlanner, traversal::ScopeTraversal,
    };

    fn run(source: &str) -> ScopeCollector<'static> {
        let parsed = crate::parse(source, "facts.js").expect("source should parse");
        let names = glass_lint_datastructures::NameTable::default();
        let planner = ScopePlanner::new_for_test(parsed.program.span(), names);
        let mut plan_traversal = ScopeTraversal::new(planner);
        parsed.program.visit_children_with(&mut plan_traversal);
        let plan = plan_traversal.into_pass().finish();
        let collector = ScopeCollector::from_plan_for_test(plan);
        let mut collect_traversal = ScopeTraversal::new(collector);
        parsed.program.visit_children_with(&mut collect_traversal);
        collect_traversal.into_pass()
    }

    fn find_first_declarator(program: &swc_ecma_ast::Program) -> (Pat, Expr, VarDeclKind) {
        use swc_ecma_visit::Visit;
        struct Finder(Option<(Pat, Expr, VarDeclKind)>);
        impl Visit for Finder {
            fn visit_var_decl(&mut self, decl: &VarDecl) {
                if self.0.is_some() {
                    return;
                }
                for declarator in &decl.decls {
                    if let Some(init) = declarator.init.as_deref() {
                        self.0 = Some((declarator.name.clone(), init.clone(), decl.kind));
                        return;
                    }
                }
            }
        }
        let mut finder = Finder(None);
        program.visit_with(&mut finder);
        finder
            .0
            .expect("source should contain a var/let/const initializer")
    }

    fn find_first_assign(program: &swc_ecma_ast::Program) -> Expr {
        use swc_ecma_visit::Visit;
        struct Finder(Option<Expr>);
        impl Visit for Finder {
            fn visit_assign_expr(&mut self, assign: &AssignExpr) {
                if self.0.is_none() {
                    self.0 = Some((*assign.right).clone());
                }
            }
        }
        let mut finder = Finder(None);
        program.visit_with(&mut finder);
        finder
            .0
            .expect("source should contain an assignment expression")
    }

    fn declare_classify(
        collector: &ScopeCollector,
        source: &str,
        derived_function_pattern: bool,
    ) -> (DeclarationClassification, Expr, VarDeclKind) {
        let parsed = crate::parse(source, "facts.js").expect("source should parse");
        let (pattern, expr, kind) = find_first_declarator(&parsed.program);
        let classification =
            classify_declaration(collector, &expr, &pattern, derived_function_pattern);
        (classification, expr, kind)
    }

    fn assign_prov(collector: &ScopeCollector, source: &str) -> BindingProvenance {
        let parsed = crate::parse(source, "facts.js").expect("source should parse");
        let expr = find_first_assign(&parsed.program);
        assignment_provenance(collector, &expr)
    }

    #[test]
    fn caches_subresults_so_views_share_one_classification() {
        let source = "var config = { flag: host.value }; use(config);";
        let collector = run(source);
        let (classification, expr, kind) = declare_classify(&collector, source, false);
        assert!(expression_is_mutable_static_object(&collector, &expr, kind));
        assert!(
            matches!(
                classification,
                DeclarationClassification::Binding { ref provenance, .. } if matches!(
                    provenance,
                    BindingProvenance::StaticObjectValues(_)
                )
            ),
            "expected StaticObjectValues binding, got {classification:?}",
        );
    }

    #[test]
    fn classifies_direct_require_as_require_module() {
        let source = "const { send } = require('sdk');";
        let collector = run(source);
        let (classification, ..) = declare_classify(&collector, source, false);
        assert!(
            matches!(classification, DeclarationClassification::Require { .. }),
            "expected Require classification, got {classification:?}",
        );
    }

    #[test]
    fn root_member_alias_produces_returned_object_binding() {
        let source = "const api = host.files; use(api);";
        let collector = run(source);
        let (classification, ..) = declare_classify(&collector, source, false);
        assert!(
            matches!(
                classification,
                DeclarationClassification::Binding {
                    provenance: BindingProvenance::ReturnedObject { .. },
                    ..
                }
            ),
            "expected ReturnedObject binding, got {classification:?}",
        );
    }

    #[test]
    fn reassignment_provenance_uses_the_latest_visible_binding() {
        let source = "let api = host.files; api = host.cache; use(api);";
        let collector = run(source);
        let provenance = assign_prov(&collector, source);
        assert!(
            matches!(provenance, BindingProvenance::ReturnedObject { .. }),
            "expected ReturnedObject assignment provenance, got {provenance:?}",
        );
    }

    #[test]
    fn assignment_provenance_prefers_bound_callable_over_rooted_alias() {
        let source = "let open = null; open = host.open.bind(null, host.file); use(open);";
        let collector = run(source);
        let provenance = assign_prov(&collector, source);
        assert!(
            matches!(provenance, BindingProvenance::BoundCallable { .. }),
            "bound callable must outrank ValueAlias, got {provenance:?}",
        );
    }

    #[test]
    fn assignment_provenance_falls_through_to_local_for_dynamic_values() {
        let source = "let value = 0; value = dynamicThing(); use(value);";
        let collector = run(source);
        let provenance = assign_prov(&collector, source);
        assert!(
            !matches!(
                provenance,
                BindingProvenance::BoundCallable { .. }
                    | BindingProvenance::BoundModuleCallable { .. }
                    | BindingProvenance::ModuleExport { .. }
                    | BindingProvenance::ModuleNamespace { .. }
                    | BindingProvenance::StaticString(_)
                    | BindingProvenance::StaticNumber(_)
                    | BindingProvenance::StaticStringArray(_)
                    | BindingProvenance::StaticObjectKeys(_)
                    | BindingProvenance::StaticObjectValues(_)
            ),
            "dynamic call must not produce a strict provenance, got {provenance:?}",
        );
    }

    #[test]
    fn mutability_requires_var_declaration_kind() {
        let source = "const config = { flag: host.value }; use(config);";
        let collector = run(source);
        let parsed = crate::parse(source, "facts.js").expect("source should parse");
        let (_, expr, _) = find_first_declarator(&parsed.program);
        assert!(!expression_is_mutable_static_object(
            &collector,
            &expr,
            VarDeclKind::Const
        ));
        assert!(!expression_is_mutable_static_object(
            &collector,
            &expr,
            VarDeclKind::Let
        ));
    }

    #[test]
    fn returned_object_chain_does_not_become_a_constant() {
        let source = "const send = host.create().send; use(send);";
        let collector = run(source);
        let (classification, ..) = declare_classify(&collector, source, false);
        assert!(
            matches!(
                classification,
                DeclarationClassification::Binding {
                    provenance: BindingProvenance::ReturnedObject { .. },
                    ..
                }
            ),
            "returned-object chain should not be mistreated as constant, got {classification:?}",
        );
    }

    #[test]
    fn destructuring_pattern_classifies_its_outer_declarator() {
        let source = "const { read } = host.files; use(read);";
        let collector = run(source);
        let (classification, ..) = declare_classify(&collector, source, false);
        assert!(
            !matches!(classification, DeclarationClassification::Binding { .. }),
            "destructuring pattern must not produce a binding provenance, got {classification:?}",
        );
    }

    #[test]
    fn destructured_require_records_individual_named_exports() {
        let source = "const { read } = require('sdk'); use(read);";
        let collector = run(source);
        let (classification, ..) = declare_classify(&collector, source, false);
        assert!(
            matches!(classification, DeclarationClassification::Require { .. }),
            "expected Require classification for destructured require, got {classification:?}",
        );
    }

    #[test]
    fn precedence_picks_bound_callable_over_constant_for_aliased_calls() {
        let source = "let open = null; open = host.open.bind(null, 'GET'); use(open);";
        let collector = run(source);
        let provenance = assign_prov(&collector, source);
        assert!(
            matches!(provenance, BindingProvenance::BoundCallable { .. }),
            "bound callable must outrank literal constant, got {provenance:?}",
        );
    }
}
