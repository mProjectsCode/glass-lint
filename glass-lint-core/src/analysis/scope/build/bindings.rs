//! Shared binding helpers consumed by both scope-planning and
//! source-order collection passes.
//!
//! Each function is a pure extraction of duplicated declaration policy.
//! Pass-specific insertion (e.g. `intern_provenance_strings` in the
//! collector) stays in the caller.

use std::collections::BTreeSet;

use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{ImportDecl, ImportSpecifier, Pat};

use crate::analysis::{
    scope::{BindingProvenance, LexicalScope, ScopeId, ScopeKind},
    syntax::{collect_pat_bindings, module_export_name},
};

/// Yield every `(name, provenance)` pair introduced by an import declaration.
///
/// Both the scope planner and the source-order collector use the same
/// provenance construction for specifiers, then insert through their own
/// `insert` methods (which may differ in intern behaviour).
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
                f(
                    local,
                    BindingProvenance::ModuleExport {
                        module: module.clone(),
                        export,
                    },
                );
            }
            ImportSpecifier::Namespace(namespace) => f(
                namespace.local.sym.to_smolstr(),
                BindingProvenance::ModuleNamespace {
                    module: module.clone(),
                },
            ),
            ImportSpecifier::Default(default) => f(
                default.local.sym.to_smolstr(),
                BindingProvenance::ModuleNamespace {
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
pub(super) fn var_binding_scope(stack: &[usize], scopes: &[LexicalScope]) -> ScopeId {
    stack
        .iter()
        .rev()
        .copied()
        .find(|index| {
            matches!(
                scopes[*index].kind,
                ScopeKind::Program | ScopeKind::Function
            )
        })
        .map_or_else(|| ScopeId::from(0), ScopeId::from)
}

/// Invoke `f` with every binding name introduced by a destructuring pattern.
///
/// Both passes mark every pattern-introduced binding as `Local`; this helper
/// avoids duplicating the collection loop.
pub(super) fn for_each_pat_binding(pat: &Pat, mut f: impl FnMut(SmolStr)) {
    let mut bindings = BTreeSet::new();
    collect_pat_bindings(pat, &mut bindings);
    for binding in bindings {
        f(binding);
    }
}
