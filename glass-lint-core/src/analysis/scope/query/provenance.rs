//! Provenance queries over the lexical scope graph.
//!
//! These methods deliberately keep identity, shadowing, and mutation checks
//! together. A rooted spelling is useful only when every relevant binding and
//! property write remains proven at the use position.

#![allow(clippy::match_same_arms)]

use super::{
    BindingKey, BindingProvenance, Expr, Ident, IdentValueSeed, MemberExpr, MemberValueSeed,
    ScopeGraph, Span, SymbolCallProvenance, SymbolMemberProvenance, SymbolPath, constant, contains,
    member_prefix_ends, member_root_identifier,
};

impl ScopeGraph {
    /// Resolve a direct member of a recognized host global object to the same
    /// callable identity as its bare global binding. This is deliberately
    /// limited to one property segment: `window.fetch` is the global `fetch`,
    /// while deeper host paths remain ordinary rooted members.
    pub(in crate::analysis) fn global_callable_member_at(
        &self,
        chain: &str,
        span: Span,
    ) -> Option<String> {
        let (root, member) = chain.split_once('.')?;
        if member.contains('.')
            || !self.is_global_member(root, member)
            || !self.unshadowed_global_at(root, span)
        {
            return None;
        }

        let receiver = self.binding_key_for_name(root, span)?;
        if self.property_was_written_at(&receiver, &[member.to_string()], span) {
            return None;
        }
        if self.rooted_property_was_mutated_at(root, Some(member), span) {
            return None;
        }

        Some(member.to_string())
    }

    /// Resolve a member expression after applying alias and mutation checks.
    pub(in crate::analysis) fn rooted_member_chain(&self, member: &MemberExpr) -> Option<String> {
        let syntactic_chain = self.member_expression_chain(member).or_else(|| {
            let object = crate::analysis::syntax::expression_name(&member.obj)?;
            let property = self.member_property_name(member)?;
            Some(format!("{object}.{property}"))
        })?;
        self.resolve_member_chain(member, &syntactic_chain)
    }

    /// Resolve a syntactic member chain to a proven rooted identity.
    pub fn resolve_member_chain(
        &self,
        member: &MemberExpr,
        syntactic_chain: &str,
    ) -> Option<String> {
        if self.has_dynamic_lookup_at(member.span) {
            return None;
        }
        let Some(root) = member_root_identifier(member) else {
            return syntactic_chain
                .starts_with("this.")
                .then(|| syntactic_chain.to_string());
        };
        let receiver_key = self.binding_key_for_name(root.sym.as_ref(), root.span)?;
        for prefix_end in member_prefix_ends(syntactic_chain) {
            let property = &syntactic_chain[..prefix_end];
            let Some(path) = property
                .strip_prefix(root.sym.as_ref())
                .and_then(|path| path.strip_prefix('.'))
                .map(|path| path.split('.').map(str::to_string).collect::<Vec<_>>())
            else {
                continue;
            };
            let Some(assignments) = self.property_aliases(&(receiver_key.clone(), path)) else {
                continue;
            };
            let prior_count =
                assignments.partition_point(|assignment| assignment.span.lo <= member.span.lo);
            if let Some(assignment) = assignments[..prior_count].iter().rev().find(|assignment| {
                self.scope_span(assignment.scope)
                    .is_some_and(|scope| contains(scope, member.span))
            }) {
                let target = assignment.target.as_ref()?;
                return Some(
                    target
                        .append_chain(&syntactic_chain[prefix_end..])
                        .to_string(),
                );
            }
        }
        let suffix = syntactic_chain.strip_prefix(root.sym.as_ref())?;
        match self.binding_at(root.sym.as_ref(), root.span) {
            Some(BindingProvenance::ValueAlias { target })
                if self.rooted_path_available(target) =>
            {
                Some(target.append_chain(suffix).to_string())
            }
            Some(BindingProvenance::BoundCallable { target, .. })
                if self.rooted_path_available(target) =>
            {
                Some(target.append_chain(suffix).to_string())
            }
            Some(BindingProvenance::ReturnedObject { source })
                if self.rooted_path_available(source) =>
            {
                Some(source.append_chain(suffix).to_string())
            }
            Some(
                BindingProvenance::ValueAlias { .. }
                | BindingProvenance::BoundCallable { .. }
                | BindingProvenance::ReturnedObject { .. },
            ) => None,
            Some(
                BindingProvenance::Local
                | BindingProvenance::ModuleExport { .. }
                | BindingProvenance::ModuleNamespace { .. }
                | BindingProvenance::BoundModuleCallable { .. }
                | BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_),
            ) => None,
            None if self.is_global(root.sym.as_ref()) => Some(syntactic_chain.to_string()),
            None => None,
        }
    }

    /// Whether a path starts at an allowed stable root.
    fn rooted_path_available(&self, path: &SymbolPath) -> bool {
        let value = path.to_string();
        let root = value.split('.').next().unwrap_or_default();
        root == "this" || self.is_global(root)
    }

    /// Whether a receiver path was assigned before this use in its scope.
    fn property_was_written_at(&self, receiver: &BindingKey, path: &[String], span: Span) -> bool {
        self.property_aliases(&(receiver.clone(), path.to_vec()))
            .is_some_and(|assignments| {
                assignments.iter().any(|assignment| {
                    assignment.span.lo <= span.lo
                        && self
                            .scope_span(assignment.scope)
                            .is_some_and(|scope| contains(scope, span))
                })
            })
    }

    /// Whether a rooted global/member property was invalidated before use.
    fn rooted_property_was_mutated_at(
        &self,
        root: &str,
        property: Option<&str>,
        span: Span,
    ) -> bool {
        self.rooted_mutations(root).is_some_and(|mutations| {
            mutations.iter().any(|mutation| {
                mutation.span.lo <= span.lo
                    && mutation
                        .property
                        .as_deref()
                        .is_none_or(|written| property.is_none_or(|expected| written == expected))
                    && self
                        .scope_span(mutation.scope)
                        .is_some_and(|scope| contains(scope, span))
            })
        })
    }
}

impl ScopeGraph {
    /// Resolve callable provenance while rejecting dynamic or shadowed uses.
    pub(in crate::analysis) fn call_provenance(
        &self,
        name: &str,
        span: Span,
    ) -> SymbolCallProvenance {
        if self.has_dynamic_lookup_at(span) {
            return SymbolCallProvenance::Local;
        }
        match self.binding_at(name, span) {
            Some(BindingProvenance::ModuleExport { module, export }) => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.clone(),
                }
            }
            Some(BindingProvenance::ValueAlias { target })
                if target.is_root() && self.is_global(&target.to_string()) =>
            {
                SymbolCallProvenance::Global {
                    name: target.to_string(),
                }
            }
            Some(BindingProvenance::ValueAlias { target })
                if target.without_bind_suffix().as_ref().is_some_and(|target| {
                    target.is_root() && self.is_global(&target.to_string())
                }) =>
            {
                SymbolCallProvenance::Global {
                    name: target
                        .without_bind_suffix()
                        .map_or_else(|| target.to_string(), |root| root.to_string()),
                }
            }
            Some(BindingProvenance::ValueAlias { target }) => self
                .module_export_for_chain(&target.to_string(), span)
                .unwrap_or(SymbolCallProvenance::Local),
            Some(BindingProvenance::BoundCallable { target, .. })
                if target.is_root() && self.is_global(&target.to_string()) =>
            {
                SymbolCallProvenance::Global {
                    name: target.to_string(),
                }
            }
            Some(BindingProvenance::BoundCallable { target, .. }) => self
                .module_export_for_chain(&target.to_string(), span)
                .unwrap_or(SymbolCallProvenance::Local),
            Some(BindingProvenance::BoundModuleCallable { module, export, .. }) => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.clone(),
                }
            }
            Some(
                BindingProvenance::Local
                | BindingProvenance::ModuleNamespace { .. }
                | BindingProvenance::ReturnedObject { .. }
                | BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_),
            ) => SymbolCallProvenance::Local,
            None if self.is_global(name) => SymbolCallProvenance::Global {
                name: name.to_string(),
            },
            None => SymbolCallProvenance::Local,
        }
    }

    /// Produce the immutable resolver seed for an identifier occurrence.
    pub(in crate::analysis) fn ident_value_seed(&self, ident: &Ident) -> IdentValueSeed {
        let expr = Expr::Ident(ident.clone());
        IdentValueSeed {
            call: self.call_provenance(ident.sym.as_ref(), ident.span),
            rooted_chain: self.callable_member_chain(ident).map(Into::into),
            binding: self.binding_key_for_expr(&expr),
            constant: self.constant_value(&expr),
            bound_arguments: self.bound_arguments(ident),
        }
    }

    /// Extract a statically evaluable member property name.
    pub(in crate::analysis) fn member_property_name(&self, member: &MemberExpr) -> Option<String> {
        constant::property_name(&member.prop, self)
    }

    /// Return the syntax-level dotted chain for a member expression.
    pub(in crate::analysis) fn member_expression_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<String> {
        let object = super::syntax::expression_name(&member.obj)?;
        Some(format!("{object}.{}", self.member_property_name(member)?))
    }

    /// Return a callable rooted chain for a proven identifier binding.
    pub(in crate::analysis) fn callable_member_chain(&self, ident: &Ident) -> Option<String> {
        if self.has_dynamic_lookup_at(ident.span) {
            return None;
        }
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::ValueAlias { target } if self.rooted_path_available(target) => Some(
                target
                    .without_bind_suffix()
                    .map_or_else(|| target.to_string(), |root| root.to_string()),
            ),
            BindingProvenance::BoundCallable { target, .. }
                if self.rooted_path_available(target) =>
            {
                Some(target.to_string())
            }
            BindingProvenance::BoundModuleCallable { .. } => None,
            BindingProvenance::ReturnedObject { source } if self.rooted_path_available(source) => {
                Some(source.to_string())
            }
            _ => None,
        }
    }

    /// Convert a namespace-rooted chain into module export provenance.
    pub(in crate::analysis) fn module_export_for_chain(
        &self,
        chain: &str,
        span: Span,
    ) -> Option<SymbolCallProvenance> {
        let (root, export) = chain.split_once('.')?;
        match self.binding_at(root, span)? {
            BindingProvenance::ModuleNamespace { module } => {
                Some(SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.to_string(),
                })
            }
            _ => None,
        }
    }

    /// Resolve the module namespace provenance of a member call chain.
    pub(in crate::analysis) fn member_call_provenance_for_chain(
        &self,
        member: &MemberExpr,
        chain: &str,
    ) -> Option<SymbolMemberProvenance> {
        if self.has_dynamic_lookup_at(member.span) {
            return None;
        }
        if let Some((module, prefix)) = self.module_member_for_expr(&member.obj) {
            let property = self.member_property_name(member)?;
            return Some(SymbolMemberProvenance::ModuleNamespace {
                module,
                member: if prefix.is_empty() {
                    property
                } else {
                    format!("{prefix}.{property}")
                },
            });
        }
        let root = member_root_identifier(member)?;
        let member = chain.strip_prefix(root.sym.as_ref())?.strip_prefix('.')?;
        match self.binding_at(root.sym.as_ref(), root.span) {
            Some(BindingProvenance::ModuleNamespace { module }) => {
                Some(SymbolMemberProvenance::ModuleNamespace {
                    module: module.clone(),
                    member: member.to_string(),
                })
            }
            _ => None,
        }
    }

    /// Produce the immutable resolver seed for a member occurrence.
    pub(in crate::analysis) fn member_value_seed(&self, member: &MemberExpr) -> MemberValueSeed {
        let syntactic_chain = self.member_expression_chain(member).map(SymbolPath::from);
        let rooted_chain = syntactic_chain
            .as_ref()
            .and_then(|chain| self.resolve_member_chain(member, &chain.to_string()))
            .or_else(|| self.rooted_member_chain(member))
            .map(SymbolPath::from);
        let module_member = syntactic_chain
            .as_ref()
            .and_then(|chain| self.member_call_provenance_for_chain(member, &chain.to_string()));
        let returned_member = self
            .returned_member(member)
            .map(|(source, member)| (SymbolPath::from(source), member));
        MemberValueSeed {
            syntactic_chain,
            rooted_chain,
            binding: self.binding_key_for_expr(&Expr::Member(member.clone())),
            module_member,
            returned_member,
        }
    }

    /// Resolve supported import/require expressions to module/member paths.
    pub(in crate::analysis) fn module_member_for_expr(
        &self,
        expr: &Expr,
    ) -> Option<(String, String)> {
        match expr {
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::ModuleExport { module, export } => {
                    Some((module.clone(), export.clone()))
                }
                BindingProvenance::ModuleNamespace { module } => {
                    Some((module.clone(), String::new()))
                }
                _ => None,
            },
            Expr::Member(member) => {
                let (module, prefix) = self.module_member_for_expr(&member.obj)?;
                let property = self.member_property_name(member)?;
                Some((
                    module,
                    if prefix.is_empty() {
                        property
                    } else {
                        format!("{prefix}.{property}")
                    },
                ))
            }
            Expr::Call(call) => {
                let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                let Expr::Ident(require) = &**callee else {
                    return None;
                };
                if require.sym != *"require"
                    || self
                        .binding_at(require.sym.as_ref(), require.span)
                        .is_some()
                {
                    return None;
                }
                let argument = call.args.first()?;
                let Expr::Lit(swc_ecma_ast::Lit::Str(module)) = &*argument.expr else {
                    return None;
                };
                Some((module.value.to_string_lossy().to_string(), String::new()))
            }
            Expr::Paren(paren) => self.module_member_for_expr(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.module_member_for_expr(expr)),
            _ => None,
        }
    }

    /// Returns the proven source call or rooted object that produced `expr`.
    /// Rooted member objects are retained as bounded provenance so callers can
    /// follow plugin instances obtained from a keyed collection without
    /// treating arbitrary `.load()`/`.unload()` spellings as APIs.
    pub(in crate::analysis) fn returned_object_source(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call(call) => {
                let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                let source = self.rooted_expr_chain(callee)?;
                source.contains('.').then_some(source)
            }
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::ReturnedObject { source } => Some(source.to_string()),
                _ => None,
            },
            Expr::Member(member) => {
                if let Some(source) = self.returned_object_source(&member.obj) {
                    return Some(source);
                }
                self.rooted_member_chain(member)
            }
            Expr::Paren(paren) => self.returned_object_source(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.returned_object_source(expr)),
            _ => None,
        }
    }

    /// Return the source and property for a proven returned object member.
    pub(in crate::analysis) fn returned_member(
        &self,
        member: &MemberExpr,
    ) -> Option<(String, String)> {
        let source = self.returned_object_source(&member.obj)?;
        let property = self.member_property_name(member)?;
        Some((source, property))
    }
}
