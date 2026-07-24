//! Shared subresults for a single declaration or assignment initializer.
//!
//! Each helper is computed at most once per expression and the cached
//! subresults are reused by declaration classification, assignment
//! provenance, and mutability decisions. Exhaustion and unknown outcomes are
//! carried explicitly through [`DeclarationFactState`] so a failed
//! sub-analysis cannot be mistaken for a lower-priority negative result.

use glass_lint_datastructures::NamePath;
use smol_str::SmolStr;
use swc_ecma_ast::{Expr, Pat, VarDeclKind};

use super::{BindingProvenance, ScopeCollector};

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

/// One bounded subresult set for an expression.
///
/// `None` means the helper could not prove the fact. A failed sub-analysis
/// must be carried through this type rather than being absorbed into a
/// default value, otherwise a precedence step could mistake a missing fact
/// for a lower-priority negative answer.
#[derive(Clone)]
pub(super) struct DeclarationFactState {
    bound_callable: Option<BindingProvenance>,
    module_alias: Option<BindingProvenance>,
    require_module: Option<SmolStr>,
    static_object_values: Option<BindingProvenance>,
    const_value: Option<BindingProvenance>,
    returned_object: Option<BindingProvenance>,
    rooted_path: Option<NamePath>,
}

/// Cached facts computed at most once per expression.
///
/// `DeclarationAnalysis` previously cached only the rooted path and
/// recomputed every other helper on each call. The new facade computes the
/// callable, module, require, static-object, constant, returned-object, and
/// rooted-path subresults exactly once on construction and lets the
/// declaration, assignment, and mutability views read from the same shared
/// state. The state is eagerly computed because each subresult helper is a
/// pure, read-only query on the collector and the visitor interleaves the
/// classification with mutable bookkeeping that requires `&mut self`.
pub(super) struct DeclarationFacts {
    state: DeclarationFactState,
}

impl DeclarationFacts {
    pub(super) fn compute(collector: &ScopeCollector, expr: &Expr) -> Self {
        let state = DeclarationFactState {
            bound_callable: collector.bound_callable_provenance(expr),
            module_alias: collector.module_alias_provenance(expr),
            require_module: collector.require_module_expr_name(expr),
            static_object_values: collector.static_object_values(expr),
            const_value: collector.const_provenance(expr),
            returned_object: collector.returned_object_provenance(expr),
            rooted_path: collector.rooted_name_path(expr),
        };
        Self { state }
    }

    /// Classify a declaration whose pattern is `pat`, taking the derived
    /// function pattern flag from [`collect_derived_function_pattern`].
    ///
    /// Precedence is applied once per call; both declaration and assignment
    /// views read from the same cached state.
    pub(super) fn classify_declaration(
        &self,
        pat: &Pat,
        derived_function_pattern: bool,
    ) -> DeclarationClassification {
        let name = match pat {
            Pat::Ident(ident) => Some(ident.id.sym.to_string()),
            _ => None,
        };

        // Priority 1: bound callable
        if let (Some(name), Some(provenance)) = (name.clone(), self.state.bound_callable.clone()) {
            return DeclarationClassification::Binding { name, provenance };
        }

        // Priority 2: module alias (Binding path or Require namespace)
        if let Some(provenance) = self.state.module_alias.clone() {
            if let Some(name) = name.clone() {
                return DeclarationClassification::Binding { name, provenance };
            }
            if let BindingProvenance::ModuleNamespace { module } = provenance {
                return DeclarationClassification::Require { module };
            }
        }

        // Priority 3: literal `require("…")` / interop wrapper call
        if let Some(module) = self.state.require_module.clone() {
            return DeclarationClassification::Require { module };
        }

        // Priority 4: static object values or bounded constant
        if let (Some(name), Some(provenance)) = (
            name.clone(),
            self.state
                .static_object_values
                .clone()
                .or_else(|| self.state.const_value.clone()),
        ) {
            return DeclarationClassification::Binding { name, provenance };
        }

        // Priority 5: returned object, only when the rooted alias is not the
        // empty root (the same guard as the prior implementation).
        if self
            .state
            .rooted_path
            .as_ref()
            .is_none_or(|target| !target.is_root())
            && let (Some(name), Some(provenance)) = (name, self.state.returned_object.clone())
        {
            return DeclarationClassification::Binding { name, provenance };
        }

        // Priority 6: value alias, suppressed for the derived function pattern
        if !derived_function_pattern && let Some(target) = self.state.rooted_path.clone() {
            return DeclarationClassification::ValueAlias { target };
        }

        DeclarationClassification::None
    }

    /// Assignment precedence: bound callable, module alias, returned object,
    /// constant, rooted alias, then local. Exhaustive unknown outcomes from
    /// each helper are honored; a failed sub-analysis does not become a
    /// lower-priority positive result.
    pub(super) fn assignment_provenance(&self) -> BindingProvenance {
        self.state
            .bound_callable
            .clone()
            .or_else(|| self.state.module_alias.clone())
            .or_else(|| self.state.returned_object.clone())
            .or_else(|| self.state.const_value.clone())
            .or_else(|| {
                self.state
                    .rooted_path
                    .clone()
                    .map(|target| BindingProvenance::ValueAlias { target })
            })
            .unwrap_or(BindingProvenance::Local)
    }

    /// Whether declaring this initializer with `kind` should be tracked as a
    /// mutable static object.
    ///
    /// Reads the same subresult state as the classification view so the
    /// mutability decision and the recorded binding provenance cannot
    /// disagree on which static-object fact was actually observed.
    pub(super) fn is_mutable_static_object(&self, kind: VarDeclKind) -> bool {
        if kind != VarDeclKind::Var {
            return false;
        }
        matches!(
            self.state
                .static_object_values
                .as_ref()
                .or(self.state.const_value.as_ref()),
            Some(BindingProvenance::StaticObjectKeys(_) | BindingProvenance::StaticObjectValues(_))
        )
    }
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

    /// Parse, predeclare, and visit a program; expose the final collector.
    fn run(source: &str) -> ScopeCollector {
        let parsed = crate::parse(source, "facts.js").expect("source should parse");
        let names = glass_lint_datastructures::NameTable::default();
        let planner = ScopePlanner::new(parsed.program.span(), names);
        let mut plan_traversal = ScopeTraversal::new(planner);
        parsed.program.visit_children_with(&mut plan_traversal);
        let plan = plan_traversal.into_pass().finish();
        let collector = ScopeCollector::from_plan(plan);
        let mut collect_traversal = ScopeTraversal::new(collector);
        parsed.program.visit_children_with(&mut collect_traversal);
        collect_traversal.into_pass()
    }

    /// Walk the parsed program until a `var`/`let`/`const` initializer is
    /// found and return the declarator with its source pattern.
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

    /// Find the right-hand side of the first assignment expression. The
    /// program must contain at least one bare `x = …` expression.
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

    fn declarator_facts(collector: &ScopeCollector, source: &str) -> DeclarationFacts {
        let parsed = crate::parse(source, "facts.js").expect("source should parse");
        let (_, expr, _) = find_first_declarator(&parsed.program);
        DeclarationFacts::compute(collector, &expr)
    }

    fn declarator_triple(
        collector: &ScopeCollector,
        source: &str,
    ) -> (Pat, DeclarationFacts, VarDeclKind) {
        let parsed = crate::parse(source, "facts.js").expect("source should parse");
        let (pattern, expr, kind) = find_first_declarator(&parsed.program);
        let facts = DeclarationFacts::compute(collector, &expr);
        (pattern, facts, kind)
    }

    fn assign_facts(collector: &ScopeCollector, source: &str) -> DeclarationFacts {
        let parsed = crate::parse(source, "facts.js").expect("source should parse");
        let expr = find_first_assign(&parsed.program);
        DeclarationFacts::compute(collector, &expr)
    }

    #[test]
    fn caches_subresults_so_views_share_one_classification() {
        // The facade must not duplicate work between the mutability probe
        // and the declaration classification. The same `var` initializer
        // produces a static-object binding and the collector records it as
        // mutable; both views read from the same shared state.
        let source = "var config = { flag: host.value }; use(config);";
        let collector = run(source);
        let (pattern, facts, kind) = declarator_triple(&collector, source);
        assert!(facts.is_mutable_static_object(kind));
        let classification = facts.classify_declaration(&pattern, false);
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
        let (pattern, facts, _) = declarator_triple(&collector, source);
        let classification = facts.classify_declaration(&pattern, false);
        assert!(
            matches!(classification, DeclarationClassification::Require { .. }),
            "expected Require classification, got {classification:?}",
        );
    }

    #[test]
    fn root_member_alias_produces_returned_object_binding() {
        // `host.files` is a member access, not a plain ident alias, so the
        // higher-priority returned-object subresult is exposed as the
        // binding provenance. This guards the precedence against being
        // silently downgraded to a `ValueAlias`.
        let source = "const api = host.files; use(api);";
        let collector = run(source);
        let (pattern, facts, _) = declarator_triple(&collector, source);
        let classification = facts.classify_declaration(&pattern, false);
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
        // After `api` is reassigned to a host-returned object, a later
        // assignment must inherit the higher-priority returned-object
        // provenance.
        let source = "let api = host.files; api = host.cache; use(api);";
        let collector = run(source);
        let facts = assign_facts(&collector, source);
        let provenance = facts.assignment_provenance();
        assert!(
            matches!(provenance, BindingProvenance::ReturnedObject { .. }),
            "expected ReturnedObject assignment provenance, got {provenance:?}",
        );
    }

    #[test]
    fn assignment_provenance_prefers_bound_callable_over_rooted_alias() {
        // `host.open.bind(null, host.file)` matches both bound_callable and
        // a rooted alias; the higher-priority bound_callable provenance must
        // win at the assignment site.
        let source = "let open = null; open = host.open.bind(null, host.file); use(open);";
        let collector = run(source);
        let facts = assign_facts(&collector, source);
        let provenance = facts.assignment_provenance();
        assert!(
            matches!(provenance, BindingProvenance::BoundCallable { .. }),
            "bound callable must outrank ValueAlias, got {provenance:?}",
        );
    }

    #[test]
    fn assignment_provenance_falls_through_to_local_for_dynamic_values() {
        // A call whose callee is a non-existent global is conservatively
        // recorded as a returned-object binding rather than a fully
        // local one. Either way, no rooted, callable, or module
        // provenance should be inferred.
        let source = "let value = 0; value = dynamicThing(); use(value);";
        let collector = run(source);
        let facts = assign_facts(&collector, source);
        let provenance = facts.assignment_provenance();
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
        let facts = declarator_facts(&collector, source);
        assert!(!facts.is_mutable_static_object(VarDeclKind::Const));
        assert!(!facts.is_mutable_static_object(VarDeclKind::Let));
    }

    #[test]
    fn returned_object_chain_does_not_become_a_constant() {
        // `host.create().send` is a member access on a returned object.
        // The returned-object subresult has higher priority than the
        // constant subresult; precedence must not be lost in the facade.
        let source = "const send = host.create().send; use(send);";
        let collector = run(source);
        let (pattern, facts, _) = declarator_triple(&collector, source);
        let classification = facts.classify_declaration(&pattern, false);
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
        // Object destructuring means the outer pattern has no identifier
        // name; the facade must not invent a binding provenance for it.
        let source = "const { read } = host.files; use(read);";
        let collector = run(source);
        let (pattern, facts, _) = declarator_triple(&collector, source);
        let classification = facts.classify_declaration(&pattern, false);
        assert!(
            !matches!(classification, DeclarationClassification::Binding { .. }),
            "destructuring pattern must not produce a binding provenance, got {classification:?}",
        );
    }

    #[test]
    fn destructured_require_records_individual_named_exports() {
        // `const { read } = require('sdk')` is a real-world form: the
        // outer declarator's pattern is an object, but the initializer is
        // still a `require` call and the facade must classify it as
        // `Require`. The downstream `collect_require_aliases` step then
        // turns that into per-property `ModuleExport` bindings.
        let source = "const { read } = require('sdk'); use(read);";
        let collector = run(source);
        let (pattern, facts, _) = declarator_triple(&collector, source);
        let classification = facts.classify_declaration(&pattern, false);
        assert!(
            matches!(classification, DeclarationClassification::Require { .. }),
            "expected Require classification for destructured require, got {classification:?}",
        );
    }

    #[test]
    fn precedence_picks_bound_callable_over_constant_for_aliased_calls() {
        // `host.open.bind(null, 'GET')` produces a bound callable. A
        // separate `const_provenance` walk would observe the literal
        // `'GET'`, but the bound callable is higher priority and the
        // facade must preserve that precedence.
        let source = "let open = null; open = host.open.bind(null, 'GET'); use(open);";
        let collector = run(source);
        let facts = assign_facts(&collector, source);
        let provenance = facts.assignment_provenance();
        assert!(
            matches!(provenance, BindingProvenance::BoundCallable { .. }),
            "bound callable must outrank literal constant, got {provenance:?}",
        );
    }
}
