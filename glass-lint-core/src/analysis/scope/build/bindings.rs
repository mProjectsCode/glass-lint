//! Shared binding helpers consumed by both scope-planning and
//! source-order collection passes.
//!
//! Each function is a pure extraction of duplicated declaration policy.
//! Pass-specific insertion (e.g. `intern_provenance_strings` in the
//! collector) stays in the caller.

use std::collections::BTreeSet;

use glass_lint_datastructures::NameTable;
use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{ImportDecl, ImportSpecifier, Pat};

use crate::analysis::{
    SemanticBudget,
    scope::{BindingProvenance, LexicalScopes, ScopeId, ScopeKind},
    syntax::{collect_pat_bindings, module_export_name},
};

pub(super) fn intern_checked(
    names: &mut NameTable,
    name_exhausted: &mut bool,
    budget: &SemanticBudget,
    name: &str,
) -> Option<glass_lint_datastructures::NameId> {
    budget.try_charge();
    names.intern(name).map_or_else(
        |_| {
            *name_exhausted = true;
            None
        },
        Some,
    )
}

/// Register one declaration binding: charge the semantic budget, intern the
/// name, fail closed on exhaustion, then insert into the owning scope.
///
/// Shared by the planner's declaration pass and the collector's source-order
/// registration so budget and exhaustion handling stays in one place.
pub(super) fn register_declaration_binding(
    scopes: &mut LexicalScopes,
    names: &mut NameTable,
    name_exhausted: &mut bool,
    budget: &SemanticBudget,
    scope: ScopeId,
    name: impl Into<SmolStr>,
    provenance: BindingProvenance,
) {
    let name = name.into();
    let Some(name_id) = intern_checked(names, name_exhausted, budget, name.as_str()) else {
        return;
    };
    if let Some(scope_data) = scopes.get_mut(scope) {
        scope_data.insert_binding(name_id, provenance);
    }
}

/// Yield every `(name, provenance)` pair introduced by an import declaration.
///
/// Both the scope planner and the source-order collector use the same
/// provenance construction for specifiers, then insert through the planner's
/// declaration-registration operation.
pub(super) fn for_each_import_binding(
    import: &ImportDecl,
    mut f: impl FnMut(SmolStr, BindingProvenance),
) {
    let module = import.src.value.to_string_lossy().to_smolstr();
    for specifier in &import.specifiers {
        match specifier {
            ImportSpecifier::Named(named) => {
                let local = named.local.sym.to_smolstr();
                let export = named
                    .imported
                    .as_ref()
                    .map_or_else(|| local.clone(), module_export_name);
                if export == "default" {
                    f(
                        local,
                        BindingProvenance::DefaultImport {
                            module: module.clone(),
                        },
                    );
                } else {
                    f(
                        local,
                        BindingProvenance::ModuleExport {
                            module: module.clone(),
                            export,
                        },
                    );
                }
            }
            ImportSpecifier::Namespace(namespace) => f(
                namespace.local.sym.to_smolstr(),
                BindingProvenance::ModuleNamespace {
                    module: module.clone(),
                },
            ),
            ImportSpecifier::Default(default) => f(
                default.local.sym.to_smolstr(),
                BindingProvenance::DefaultImport {
                    module: module.clone(),
                },
            ),
        }
    }
}

/// Find the enclosing function or program scope for a `var` declaration.
///
/// `var` bindings are hoisted to the nearest enclosing function or program
/// scope, skipping intermediate block scopes.
pub(super) fn var_binding_scope(stack: &[ScopeId], scopes: &LexicalScopes) -> Option<ScopeId> {
    stack
        .iter()
        .rev()
        .copied()
        .find(|scope_id| {
            scopes.get(*scope_id).is_some_and(|scope| {
                matches!(scope.kind(), ScopeKind::Program | ScopeKind::Function)
            })
        })
        .or_else(|| scopes.program_scope())
}

/// Invoke `f` with every binding name introduced by a destructuring pattern.
///
/// The planner registers every pattern-introduced binding as `Local`; the
/// collector reuses this helper when it must register its catch-only bindings
/// or reset a redeclaration before classifying its initializer.
pub(super) fn for_each_pat_binding(pat: &Pat, mut f: impl FnMut(SmolStr)) {
    let mut bindings = BTreeSet::new();
    collect_pat_bindings(pat, &mut bindings);
    for binding in bindings {
        f(binding);
    }
}
