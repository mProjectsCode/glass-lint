//! Provenance queries over the lexical scope graph.
//!
//! These methods deliberately keep identity, shadowing, and mutation checks
//! together. A rooted spelling is useful only when every relevant binding and
//! property write remains proven at the use position.

#![allow(clippy::match_same_arms)]

use smol_str::{SmolStr, ToSmolStr};

use crate::analysis::{
    scope::query::{
        BindingKey, BindingProvenance, Expr, Ident, IdentValueSeed, MemberExpr, MemberValueSeed,
        ScopeGraph, Span, SymbolCallProvenance, SymbolMemberProvenance, SymbolPath, constant,
        contains, member_root_identifier,
    },
    syntax::{constant::Lookup, expression_name},
    value::BindingRoot,
};

impl ScopeGraph {
    /// Resolve a direct member of a recognized host global object to the same
    /// callable identity as its bare global binding. This is deliberately
    /// limited to one property segment: `window.fetch` is the global `fetch`,
    /// while deeper host paths remain ordinary rooted members.
    pub(in crate::analysis) fn global_callable_member_at(
        &self,
        chain: &SymbolPath,
        span: Span,
    ) -> Option<SymbolPath> {
        let [root, member] = chain.segments() else {
            return None;
        };
        if !self.is_global_member(root, member) || !self.unshadowed_global_at(root, span) {
            return None;
        }

        let receiver = self.binding_key_for_name(root, span)?;
        let written = self.property_was_written_at(
            &receiver,
            self.name_path(&SymbolPath::from_chain(member))?,
            span,
        );
        if written {
            return None;
        }
        if self.rooted_property_was_mutated_at(&root.as_str().into(), Some(member), span) {
            return None;
        }

        Some(member.as_str().into())
    }

    /// Resolve a member expression after applying alias and mutation checks.
    pub(in crate::analysis) fn rooted_member_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<SymbolPath> {
        let syntactic_chain = self.member_expression_chain(member).or_else(|| {
            let object = crate::analysis::syntax::expression_name(&member.obj)?;
            let property = self.member_property_name(member)?;
            Some(object.append_chain(&property))
        })?;
        self.resolve_member_chain(member, &syntactic_chain)
    }

    /// Resolve a syntactic member chain to a proven rooted identity.
    pub fn resolve_member_chain(
        &self,
        member: &MemberExpr,
        syntactic_chain: &SymbolPath,
    ) -> Option<SymbolPath> {
        if self.has_dynamic_lookup_at(member.span) {
            return None;
        }

        let Some(root) = member_root_identifier(member) else {
            return (syntactic_chain.first_segment() == Some("this"))
                .then(|| syntactic_chain.clone());
        };

        let receiver_key = self.binding_key_for_name(root.sym.as_ref(), root.span)?;
        let segments = syntactic_chain.segments();

        for prefix_end in (2..=segments.len()).rev() {
            let path = segments[1..prefix_end].to_vec();
            let Some(path) = self.name_path(&SymbolPath::from_segments(path)) else {
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
                let suffix = SymbolPath::from_segments(segments[prefix_end..].to_vec());
                return Some(target.append_path(&suffix));
            }
        }

        let suffix = SymbolPath::from_segments(segments[1..].to_vec());
        match self.binding_at(root.sym.as_ref(), root.span) {
            Some(BindingProvenance::ValueAlias { target })
                if self.rooted_path_available(target) =>
            {
                Some(self.symbol_path(target)?.append_path(&suffix))
            }
            Some(BindingProvenance::BoundCallable { target, .. })
                if self.rooted_path_available(target) =>
            {
                Some(self.symbol_path(target)?.append_path(&suffix))
            }
            Some(BindingProvenance::ReturnedObject { source })
                if self.rooted_path_available(source) =>
            {
                Some(self.symbol_path(source)?.append_path(&suffix))
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
            None if self.is_global(root.sym.as_ref()) => {
                self.rooted_chain_available_at(syntactic_chain, member.span)
            }
            None => None,
        }
    }

    /// Return the canonical identity for a rooted member expression.
    ///
    /// A configured global-object alias contributes no semantic path segment:
    /// `window.navigator.sendBeacon` and `navigator.sendBeacon` are the same
    /// identity when `window.navigator` is an allowed promotion. The original
    /// chain remains available to fact locations and evidence; this method is
    /// only used for the identity stored in semantic facts.
    fn rooted_chain_available_at(&self, chain: &SymbolPath, span: Span) -> Option<SymbolPath> {
        let segments = chain.segments();
        let [root, first, rest @ ..] = segments else {
            return None;
        };

        let promoted = self.is_global_member(root, first);
        if self.is_global(root)
            && self
                .global_objects()
                .filter(|alias| self.is_global_member(alias, root))
                .any(|alias| self.rooted_property_was_mutated_at(&alias.into(), Some(root), span))
        {
            return None;
        }
        if !promoted {
            return Some(chain.clone());
        }
        if self.rooted_chain_mutated_at(chain, span) {
            return None;
        }

        let canonical = SymbolPath::from_segments(
            std::iter::once(first.clone())
                .chain(rest.iter().cloned())
                .collect(),
        );
        if self.rooted_chain_mutated_at(&canonical, span) {
            return None;
        }
        Some(canonical)
    }

    /// Check writes through both a canonical root and any global-object alias.
    /// A write to an earlier segment invalidates every deeper rooted path.
    fn rooted_chain_mutated_at(&self, chain: &SymbolPath, span: Span) -> bool {
        let segments = chain.segments();
        if segments.len() < 2 {
            return false;
        }

        // A write through a configured realm alias changes the same promoted
        // first segment as a write through the bare global binding.
        if self
            .global_objects()
            .filter(|root| self.is_global_member(root, &segments[0]))
            .any(|root| {
                self.rooted_property_was_mutated_at(&(*root).into(), Some(&segments[0]), span)
            })
        {
            return true;
        }

        (1..segments.len()).any(|end| {
            let receiver = SymbolPath::from_segments(segments[..end].to_vec());
            let property = &segments[end];
            self.rooted_property_was_mutated_at(&receiver, Some(property.as_str()), span)
        })
    }

    pub(in crate::analysis) fn instance_member_available_at(&self, member: &MemberExpr) -> bool {
        let Some(property) = self.member_property_name(member) else {
            return false;
        };
        !self.rooted_property_was_mutated_at(&"this".into(), Some(&property), member.span)
    }

    /// Whether a path starts at an allowed stable root.
    fn rooted_path_available(&self, path: &crate::analysis::value::NamePath) -> bool {
        self.symbol_path(path).is_some_and(|path| {
            path.first_segment() == Some("this")
                || path
                    .first_segment()
                    .is_some_and(|root| self.is_global(root))
        })
    }

    /// Whether a receiver path was assigned before this use in its scope.
    fn property_was_written_at(
        &self,
        receiver: &BindingKey,
        path: crate::analysis::value::NamePath,
        span: Span,
    ) -> bool {
        self.property_aliases(&(receiver.clone(), path))
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
        root: &SymbolPath,
        property: Option<&str>,
        span: Span,
    ) -> bool {
        let Some(root) = self.name_path(root) else {
            return false;
        };
        let property = property.and_then(|property| self.name_id(property));
        self.rooted_mutations(&root).is_some_and(|mutations| {
            mutations.iter().any(|mutation| {
                mutation.span.lo <= span.lo
                    && mutation
                        .property
                        .is_none_or(|written| property.is_none_or(|expected| written == expected))
                    && self
                        .scope_span(mutation.scope)
                        .is_some_and(|scope| contains(scope, span))
            })
        })
    }
}

impl ScopeGraph {
    /// Derived global or module-export provenance from a symbol path, falling
    /// back to [`SymbolCallProvenance::Local`].
    fn symbol_path_provenance(
        &self,
        target: &SymbolPath,
        check_path: &SymbolPath,
        span: Span,
    ) -> SymbolCallProvenance {
        if check_path.is_root()
            && let Some(root_segment) = check_path.first_segment()
            && self.is_global(root_segment)
        {
            SymbolCallProvenance::Global {
                name: root_segment.to_smolstr(),
            }
        } else {
            self.module_export_for_chain(&target.to_string(), span)
                .unwrap_or(SymbolCallProvenance::Local)
        }
    }

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
            Some(BindingProvenance::ValueAlias { target }) => {
                let Some(path) = self.symbol_path(target) else {
                    return SymbolCallProvenance::Local;
                };
                let root = path.without_bind_suffix().unwrap_or_else(|| path.clone());
                self.symbol_path_provenance(&path, &root, span)
            }
            Some(BindingProvenance::BoundCallable { target, .. }) => {
                let Some(path) = self.symbol_path(target) else {
                    return SymbolCallProvenance::Local;
                };
                self.symbol_path_provenance(&path, &path, span)
            }
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
                name: name.to_smolstr(),
            },
            None => SymbolCallProvenance::Local,
        }
    }

    /// Produce the immutable resolver seed for an identifier occurrence.
    pub(in crate::analysis) fn ident_value_seed(&self, ident: &Ident) -> IdentValueSeed {
        let binding = self
            .binding_with_scope_at(ident.sym.as_ref(), ident.span)
            .and_then(|(scope, _)| {
                Some(BindingKey::new(BindingRoot::Binding {
                    function: self.function_scope_at(scope),
                    binding: self.binding_id_at(scope, ident.sym.as_ref())?,
                    version: self.binding_version_at(scope, ident.sym.as_ref(), ident.span),
                }))
            });
        let constant = if self.has_dynamic_lookup_at(ident.span) {
            constant::ConstValue::Unknown
        } else {
            self.ident(ident, &mut constant::EvalState::default())
        };
        IdentValueSeed {
            call: self.call_provenance(ident.sym.as_ref(), ident.span),
            rooted_chain: self.callable_member_chain(ident),
            binding,
            constant,
            bound_arguments: self.bound_arguments(ident),
        }
    }

    /// Extract a statically evaluable member property name.
    pub(in crate::analysis) fn member_property_name(&self, member: &MemberExpr) -> Option<SmolStr> {
        constant::property_name(&member.prop, self)
    }

    /// Return the syntax-level dotted chain for a member expression.
    pub(in crate::analysis) fn member_expression_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<SymbolPath> {
        let object = expression_name(&member.obj)?;
        Some(object.append_chain(&self.member_property_name(member)?))
    }

    /// Return a callable rooted chain for a proven identifier binding.
    pub(in crate::analysis) fn callable_member_chain(&self, ident: &Ident) -> Option<SymbolPath> {
        if self.has_dynamic_lookup_at(ident.span) {
            return None;
        }
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::ValueAlias { target } if self.rooted_path_available(target) => self
                .symbol_path(target)
                .and_then(|path| path.without_bind_suffix().or(Some(path))),
            BindingProvenance::BoundCallable { target, .. }
                if self.rooted_path_available(target) =>
            {
                self.symbol_path(target)
            }
            BindingProvenance::BoundModuleCallable { .. } => None,
            BindingProvenance::ReturnedObject { source } if self.rooted_path_available(source) => {
                self.symbol_path(source)
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
                    export: export.to_smolstr(),
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
                    format!("{prefix}.{property}").to_smolstr()
                },
            });
        }
        let root = member_root_identifier(member)?;
        let member = chain.strip_prefix(root.sym.as_ref())?.strip_prefix('.')?;
        match self.binding_at(root.sym.as_ref(), root.span) {
            Some(BindingProvenance::ModuleNamespace { module }) => {
                Some(SymbolMemberProvenance::ModuleNamespace {
                    module: module.clone(),
                    member: member.to_smolstr(),
                })
            }
            _ => None,
        }
    }

    /// Produce the immutable resolver seed for a member occurrence.
    pub(in crate::analysis) fn member_value_seed(&self, member: &MemberExpr) -> MemberValueSeed {
        let syntactic_chain = self.member_expression_chain(member);
        let rooted_chain = syntactic_chain
            .as_ref()
            .and_then(|chain| self.resolve_member_chain(member, chain))
            .or_else(|| self.rooted_member_chain(member))
            .and_then(|path| self.name_path(&path));
        let module_member = syntactic_chain
            .as_ref()
            .and_then(|chain| self.member_call_provenance_for_chain(member, &chain.to_string()));
        let returned_member = self.returned_member(member);
        let binding = self
            .binding_key_for_expr(&member.obj)
            .or_else(|| self.global_key_for_expr(&member.obj))
            .and_then(|mut key| {
                key.append_segment(self.name_id(self.member_property_name(member)?.as_str())?);
                Some(key)
            });
        MemberValueSeed {
            syntactic_chain,
            rooted_chain,
            binding,
            module_member,
            returned_member,
        }
    }

    /// Resolve supported import/require expressions to module/member paths.
    pub(in crate::analysis) fn module_member_for_expr(
        &self,
        expr: &Expr,
    ) -> Option<(SmolStr, SmolStr)> {
        match expr {
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::ModuleExport { module, export } => {
                    Some((module.clone(), export.clone()))
                }
                BindingProvenance::ModuleNamespace { module } => {
                    Some((module.clone(), SmolStr::new("")))
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
                        format!("{prefix}.{property}").to_smolstr()
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
                Some((
                    module.value.to_string_lossy().to_smolstr(),
                    SmolStr::new(""),
                ))
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
    pub(in crate::analysis) fn returned_object_source(&self, expr: &Expr) -> Option<SymbolPath> {
        match expr {
            Expr::Call(call) => {
                let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                let source = self.rooted_expr_chain(callee)?;
                (!source.is_root()).then_some(source)
            }
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::ReturnedObject { source } => self.symbol_path(source),
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
    ) -> Option<(
        crate::analysis::value::NamePath,
        crate::analysis::value::NamePath,
    )> {
        let source = self.returned_object_source(&member.obj)?;
        let property = self.member_property_name(member)?;
        Some((self.name_path(&source)?, self.name_path(&property.into())?))
    }
}
