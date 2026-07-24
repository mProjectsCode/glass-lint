use swc_common::Spanned;
use swc_ecma_ast::{AssignExpr, Expr, Pat, VarDecl, VarDeclKind};
use swc_ecma_visit::VisitWith;

use super::*;
use crate::analysis::scope::{
    collect::{ScopeCollector, plan::ScopePlanner, traversal::ScopeTraversal},
    model::BindingProvenance,
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
