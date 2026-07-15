use super::{
    BindingKey, BindingProvenance, BindingRoot, BindingVersion, BoundArgument, Expr, Ident,
    ScopeGraph, ScopeKind, Span, contains,
};

impl ScopeGraph {
    pub(in crate::analysis) fn is_configured_global(&self, name: &str) -> bool {
        self.environment.is_global(name)
    }

    pub(in crate::analysis) fn binding_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<&BindingProvenance> {
        let (scope, declaration) = self.binding_with_scope_at(name, span)?;
        // A declaration is the fallback. The last assignment at or before the
        // read wins, which is why assignments are sorted once during collection
        // and selected with `partition_point` here.
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .and_then(|assignments| {
                assignments
                    .partition_point(|assignment| assignment.span.lo <= span.lo)
                    .checked_sub(1)
                    .map(|index| &assignments[index].provenance)
            })
            .or_else(|| {
                self.function_ids
                    .get(&scope)
                    .and_then(|function| self.parameter_aliases.get(&(*function, name.to_string())))
            })
            .or(Some(declaration))
    }

    /// Resolve an expression to a stable lexical identity.  Semantic clients
    /// use this instead of rebuilding identity from the expression's printed
    /// member chain.
    pub(in crate::analysis) fn binding_key_for_expr(&self, expr: &Expr) -> Option<BindingKey> {
        match expr {
            Expr::Ident(ident) => {
                let (scope, _) = self.binding_with_scope_at(ident.sym.as_ref(), ident.span)?;
                let binding = *self.binding_ids.get(&(scope, ident.sym.to_string()))?;
                Some(BindingKey {
                    root: BindingRoot::Binding {
                        function: self.function_scope_at(scope),
                        binding,
                        version: self.binding_version_at(scope, ident.sym.as_ref(), ident.span),
                    },
                    path: Vec::new(),
                })
            }
            Expr::Member(member) => {
                let mut key = self
                    .binding_key_for_expr(&member.obj)
                    .or_else(|| self.global_key_for_expr(&member.obj))?;
                key.path.push(self.member_prop_name(member)?);
                Some(key)
            }
            Expr::This(_) => Some(BindingKey {
                root: BindingRoot::Global("this".into()),
                path: Vec::new(),
            }),
            Expr::Paren(paren) => self.binding_key_for_expr(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.binding_key_for_expr(expr)),
            _ => None,
        }
    }

    pub(in crate::analysis) fn global_key_for_expr(&self, expr: &Expr) -> Option<BindingKey> {
        match expr {
            Expr::Ident(ident) => self
                .binding_at(ident.sym.as_ref(), ident.span)
                .is_none()
                .then(|| BindingKey {
                    root: BindingRoot::Global(ident.sym.to_string()),
                    path: Vec::new(),
                }),
            Expr::Member(member) => {
                let mut key = self.global_key_for_expr(&member.obj)?;
                key.path.push(self.member_prop_name(member)?);
                Some(key)
            }
            Expr::This(_) => Some(BindingKey {
                root: BindingRoot::Global("this".into()),
                path: Vec::new(),
            }),
            Expr::Paren(paren) => self.global_key_for_expr(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.global_key_for_expr(expr)),
            _ => None,
        }
    }

    pub(in crate::analysis) fn binding_version_at(
        &self,
        scope: usize,
        name: &str,
        span: Span,
    ) -> BindingVersion {
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .map_or(BindingVersion(0), |assignments| {
                assignments
                    .partition_point(|assignment| assignment.span.lo <= span.lo)
                    .checked_sub(1)
                    .and_then(|index| assignments.get(index))
                    .map_or(BindingVersion(0), |assignment| assignment.version)
            })
    }

    pub(in crate::analysis) fn binding_key_for_name(
        &self,
        name: &str,
        span: Span,
    ) -> Option<BindingKey> {
        if let Some((scope, _)) = self.binding_with_scope_at(name, span) {
            return Some(BindingKey {
                root: BindingRoot::Binding {
                    function: self.function_scope_at(scope),
                    binding: *self.binding_ids.get(&(scope, name.to_string()))?,
                    version: self.binding_version_at(scope, name, span),
                },
                path: Vec::new(),
            });
        }
        Some(BindingKey {
            root: BindingRoot::Global(name.to_string()),
            path: Vec::new(),
        })
    }

    pub(in crate::analysis) fn binding_with_scope_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<(usize, &BindingProvenance)> {
        let mut scope = self.scope_at(span);
        loop {
            if let Some(binding) = self.scopes[scope].bindings.get(name) {
                return Some((scope, binding));
            }
            scope = self.scopes[scope].parent?;
        }
    }

    pub(in crate::analysis) fn has_dynamic_lookup_at(&self, span: Span) -> bool {
        let scope = self.scope_at(span);
        self.scope_or_ancestor_has_kind(scope, ScopeKind::Dynamic)
            || self.dynamic_evals.iter().any(|(eval_scope, eval_span)| {
                span.lo > eval_span.hi && self.scope_is_within(scope, *eval_scope)
            })
    }

    pub(in crate::analysis) fn scope_or_ancestor_has_kind(
        &self,
        mut scope: usize,
        kind: ScopeKind,
    ) -> bool {
        loop {
            if self.scopes[scope].kind == kind {
                return true;
            }
            let Some(parent) = self.scopes[scope].parent else {
                return false;
            };
            scope = parent;
        }
    }

    pub(in crate::analysis) fn scope_is_within(&self, mut scope: usize, ancestor: usize) -> bool {
        loop {
            if scope == ancestor {
                return true;
            }
            let Some(parent) = self.scopes[scope].parent else {
                return false;
            };
            scope = parent;
        }
    }

    pub(in crate::analysis) fn scope_at(&self, span: Span) -> usize {
        let position = self
            .scopes_by_start
            .partition_point(|index| self.scopes[*index].span.lo <= span.lo);
        let Some(mut scope) = position
            .checked_sub(1)
            .map(|index| self.scopes_by_start[index])
        else {
            return 0;
        };

        // Source ranges can overlap in non-nesting ways for synthetic nodes;
        // walking parents makes containment, rather than start position alone,
        // the final authority.
        while !contains(self.scopes[scope].span, span) {
            let Some(parent) = self.scopes[scope].parent else {
                return 0;
            };
            scope = parent;
        }
        scope
    }

    pub(in crate::analysis) fn bound_arguments(
        &self,
        ident: &Ident,
    ) -> Option<Vec<Option<BoundArgument>>> {
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::BoundCallable {
                bound_arguments, ..
            }
            | BindingProvenance::BoundModuleCallable {
                bound_arguments, ..
            } => Some(bound_arguments.clone()),
            _ => None,
        }
    }

    pub(in crate::analysis) fn scope_chain_at(&self, span: Span) -> Vec<usize> {
        let mut scopes = Vec::new();
        let mut scope = self.scope_at(span);
        loop {
            scopes.push(scope);
            let Some(parent) = self.scopes[scope].parent else {
                break;
            };
            scope = parent;
        }
        scopes
    }

    pub(in crate::analysis) fn unshadowed_global_at(&self, name: &str, span: Span) -> bool {
        self.environment.is_global(name)
            && !self.has_dynamic_lookup_at(span)
            && self.binding_at(name, span).is_none()
    }

    pub(in crate::analysis) fn unshadowed_unbound_at(&self, name: &str, span: Span) -> bool {
        !self.has_dynamic_lookup_at(span) && self.binding_at(name, span).is_none()
    }
}
