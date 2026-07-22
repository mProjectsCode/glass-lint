use std::cell::RefCell;

use smol_str::SmolStr;
use swc_ecma_ast::{Expr, Pat};

use super::{BindingProvenance, LexicalScopeCollector};
use crate::analysis::value::NamePath;

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

#[derive(Clone)]
enum CachedPath {
    Unresolved,
    Resolved(Option<NamePath>),
}

pub(super) struct DeclarationAnalysis<'c> {
    collector: &'c LexicalScopeCollector<'c>,
    expr: &'c Expr,
    rooted_path: RefCell<CachedPath>,
}

impl<'c> DeclarationAnalysis<'c> {
    pub(super) fn new(collector: &'c LexicalScopeCollector<'c>, expr: &'c Expr) -> Self {
        Self {
            collector,
            expr,
            rooted_path: RefCell::new(CachedPath::Unresolved),
        }
    }

    fn rooted_path(&self) -> Option<NamePath> {
        let mut cell = self.rooted_path.borrow_mut();
        if matches!(*cell, CachedPath::Unresolved) {
            *cell = CachedPath::Resolved(self.collector.rooted_name_path(self.expr));
        }
        match &*cell {
            CachedPath::Resolved(path) => path.clone(),
            CachedPath::Unresolved => unreachable!(),
        }
    }

    pub(super) fn assignment_provenance(&self) -> BindingProvenance {
        self.collector
            .bound_callable_provenance(self.expr)
            .or_else(|| self.collector.module_alias_provenance(self.expr))
            .or_else(|| self.collector.returned_object_provenance(self.expr))
            .or_else(|| self.collector.const_provenance(self.expr))
            .or_else(|| {
                self.rooted_path()
                    .map(|target| BindingProvenance::ValueAlias { target })
            })
            .unwrap_or(BindingProvenance::Local)
    }

    pub(super) fn classify_declaration(
        &self,
        pattern: &Pat,
        derived_function_pattern: bool,
    ) -> DeclarationClassification {
        let name = || match pattern {
            Pat::Ident(ident) => Some(ident.id.sym.to_string()),
            _ => None,
        };

        // Priority 1: bound_callable_provenance
        if let (Some(name), Some(provenance)) =
            (name(), self.collector.bound_callable_provenance(self.expr))
        {
            return DeclarationClassification::Binding { name, provenance };
        }

        // Priority 2: module_alias_provenance (Binding path)
        if let Some(provenance) = self.collector.module_alias_provenance(self.expr) {
            if let Some(name) = name() {
                return DeclarationClassification::Binding { name, provenance };
            }
            if let BindingProvenance::ModuleNamespace { module } = provenance {
                return DeclarationClassification::Require { module };
            }
        }

        // Priority 3: require_module_expr_name
        if let Some(module) = self.collector.require_module_expr_name(self.expr) {
            return DeclarationClassification::Require { module };
        }

        // Priority 4: const_value (static_object_values then const_provenance)
        if let (Some(name), Some(provenance)) = (
            name(),
            self.collector
                .static_object_values(self.expr)
                .or_else(|| self.collector.const_provenance(self.expr)),
        ) {
            return DeclarationClassification::Binding { name, provenance };
        }

        // Priorities 5 and 6 both need rooted_path, so compute it once here.
        let rooted_path = self.rooted_path();

        // Priority 5: returned_object_provenance (only if value_alias is not root)
        if rooted_path.as_ref().is_none_or(|target| !target.is_root())
            && let (Some(name), Some(provenance)) =
                (name(), self.collector.returned_object_provenance(self.expr))
        {
            return DeclarationClassification::Binding { name, provenance };
        }

        // Priority 6: value_alias (only if not derived_function_pattern)
        if !derived_function_pattern && let Some(target) = rooted_path {
            return DeclarationClassification::ValueAlias { target };
        }

        DeclarationClassification::None
    }
}
