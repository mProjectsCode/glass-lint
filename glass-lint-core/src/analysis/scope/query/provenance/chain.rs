use glass_lint_datastructures::{PathView, SymbolPath};

use crate::analysis::{
    scope::{
        FrozenScopeGraph,
        frozen_assignments::{BindingResolution, BindingResolutionStatus},
        query::{BindingKey, BindingProvenance, MemberExpr, Span, contains},
    },
    syntax::{expression_name, member_root_identifier},
};

impl FrozenScopeGraph {
    pub(in crate::analysis) fn global_callable_member_at(
        &self,
        chain: &SymbolPath,
        span: Span,
    ) -> Option<SymbolPath> {
        let view = chain.as_view();
        if view.len() != 2 {
            return None;
        }
        let root = view.first_segment()?;
        let member = view.last_segment()?;
        if !self.is_global_member(root, member) || !self.unshadowed_global_at(root, span) {
            return None;
        }

        let receiver = self.binding_key_for_name(root, span)?;
        let path = self.name_path(&SymbolPath::from_chain(member))?;
        let written = self.property_was_written_at(&receiver, &path, span);
        if written {
            return None;
        }
        if self.rooted_property_was_mutated_at(&root.as_str().into(), Some(member), span) {
            return None;
        }

        Some(member.as_str().into())
    }

    pub(in crate::analysis) fn rooted_member_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<SymbolPath> {
        let syntactic_chain = self.member_expression_chain(member).or_else(|| {
            let object = expression_name(&member.obj)?;
            let property = self.contextual_member_property_name(member)?;
            Some(object.append_chain(&property))
        })?;
        self.resolve_member_chain(member, &syntactic_chain)
    }

    /// Resolve an assignment target before the target write invalidates its
    /// own rooted property identity. The ordinary read resolver intentionally
    /// rejects a path mutated at the current span; a write occurrence needs
    /// the receiver identity at that same span so the assignment itself can
    /// be reported while later reads remain invalidated.
    pub(in crate::analysis) fn rooted_write_member_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<SymbolPath> {
        let property = self.contextual_member_property_name(member)?;
        // Resolve only the receiver. Resolving the complete member chain here
        // would consult writes to `property` at the current span and erase
        // the write occurrence itself. Ancestor/receiver mutations are still
        // checked by rooted_expr_chain before the property is appended.
        let receiver = self.rooted_expr_chain(&member.obj)?;
        Some(receiver.append_chain(&property))
    }

    pub(in crate::analysis) fn resolve_member_chain(
        &self,
        member: &MemberExpr,
        syntactic_chain: &SymbolPath,
    ) -> Option<SymbolPath> {
        // Prefix-match loop followed by a fallback match over binding
        // provenance variants. Extracting the fallback arm would move the
        // suffix-building logic away from the variant dispatch it mirrors.
        if self.has_dynamic_lookup_at(member.span) {
            return None;
        }

        let Some(root) = member_root_identifier(member) else {
            return syntactic_chain
                .first_segment()
                .is_some_and(|s| s == "this")
                .then(|| syntactic_chain.clone());
        };

        let receiver_key = self.binding_key_for_name(root.sym.as_ref(), root.span)?;
        let name_path = self.name_path(syntactic_chain)?;

        if let Some(path) =
            self.resolve_assigned_prefix(&receiver_key, member, syntactic_chain, &name_path)
        {
            return Some(path);
        }

        let suffix = syntactic_chain.suffix(1).unwrap_or_default();
        let resolution = self.binding_resolution_at(root.sym.as_ref(), root.span);
        self.resolve_provenance_alternatives(resolution, &suffix)
            .or_else(|| {
                self.resolve_global_fallback(
                    root.sym.as_ref(),
                    resolution,
                    syntactic_chain,
                    member.span,
                )
            })
    }

    fn resolve_assigned_prefix(
        &self,
        receiver: &BindingKey,
        member: &MemberExpr,
        syntactic_chain: &SymbolPath,
        name_path: &glass_lint_datastructures::NamePath,
    ) -> Option<SymbolPath> {
        let tail = name_path.as_view().tail_after(1)?;
        for prefix in tail.prefixes().rev() {
            let Some(assignments) = self.property_aliases(receiver, prefix) else {
                continue;
            };

            let prior_count =
                assignments.partition_point(|assignment| assignment.span().lo <= member.span.lo);
            let Some(assignment) = assignments[..prior_count].iter().rev().find(|assignment| {
                self.scope_span(assignment.scope())
                    .is_some_and(|scope| contains(scope, member.span))
            }) else {
                continue;
            };
            let target = assignment.target()?;
            let suffix = syntactic_chain.suffix(prefix.len() + 1).unwrap_or_default();
            return Some(target.append_path(&suffix));
        }
        None
    }

    fn resolve_provenance_alternatives(
        &self,
        resolution: BindingResolution<'_>,
        suffix: &SymbolPath,
    ) -> Option<SymbolPath> {
        let mut resolved = None;
        resolution.for_each_witness(|provenance| {
            let target = match provenance {
                BindingProvenance::ValueAlias { target }
                | BindingProvenance::BoundCallable { target, .. } => target,
                BindingProvenance::ReturnedObject { source } => source,
                BindingProvenance::Local
                | BindingProvenance::ModuleExport { .. }
                | BindingProvenance::DefaultImport { .. }
                | BindingProvenance::ModuleNamespace { .. }
                | BindingProvenance::ConstructedInstance { .. }
                | BindingProvenance::BoundModuleCallable { .. }
                | BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_) => return,
            };
            if resolved.is_none()
                && self.rooted_path_available(target)
                && let Some(path) = self.symbol_path(target)
            {
                resolved = Some(path.append_path(suffix));
            }
        });
        resolved
    }

    fn resolve_global_fallback(
        &self,
        root: &str,
        resolution: BindingResolution<'_>,
        syntactic_chain: &SymbolPath,
        span: Span,
    ) -> Option<SymbolPath> {
        if resolution.status() != BindingResolutionStatus::Absent || !self.is_global(root) {
            return None;
        }
        self.rooted_chain_available_at(syntactic_chain, span)
    }

    fn rooted_chain_available_at(&self, chain: &SymbolPath, span: Span) -> Option<SymbolPath> {
        let view = chain.as_view();
        if view.len() < 2 {
            return None;
        }
        let root = view.first_segment()?;
        let first = view.tail_after(1)?.first_segment()?;
        let rest = view.tail_after(2)?;

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

        let canonical = SymbolPath::from_ids(
            std::iter::once(first.clone()).chain(rest.segments().iter().cloned()),
        );
        if self.rooted_chain_mutated_at(&canonical, span) {
            return None;
        }
        Some(canonical)
    }

    fn rooted_chain_mutated_at(&self, chain: &SymbolPath, span: Span) -> bool {
        let Some(path) = self.name_path(chain) else {
            return false;
        };
        let view = path.as_view();
        if view.len() < 2 {
            return false;
        }

        let Some(first) = view.first_segment().copied() else {
            return false;
        };
        if self.resolve_name_id(first).is_some_and(|first_name| {
            self.global_objects()
                .filter(|root| self.is_global_member(root, &first_name))
                .filter_map(|root| self.name_id(root))
                .any(|root| {
                    self.rooted_property_ids_were_mutated_at(
                        PathView::new(std::slice::from_ref(&root)),
                        Some(first),
                        span,
                    )
                })
        }) {
            return true;
        }

        view.segments()
            .iter()
            .enumerate()
            .skip(1)
            .any(|(end, property)| {
                view.prefix_at(end).is_some_and(|prefix| {
                    self.rooted_property_ids_were_mutated_at(prefix, Some(*property), span)
                })
            })
    }

    pub(in crate::analysis) fn instance_member_available_at(&self, member: &MemberExpr) -> bool {
        let Some(property) = self.contextual_member_property_name(member) else {
            return false;
        };
        !self.rooted_property_was_mutated_at(&"this".into(), Some(&property), member.span)
    }

    pub(super) fn rooted_path_available(&self, path: &glass_lint_datastructures::NamePath) -> bool {
        self.symbol_path(path).is_some_and(|path| {
            path.first_segment().is_some_and(|s| s == "this")
                || path
                    .first_segment()
                    .is_some_and(|root| self.is_global(root))
        })
    }

    fn property_was_written_at(
        &self,
        receiver: &BindingKey,
        path: &glass_lint_datastructures::NamePath,
        span: Span,
    ) -> bool {
        self.property_aliases(receiver, path.as_view())
            .is_some_and(|assignments| {
                assignments.iter().any(|assignment| {
                    assignment.span().lo <= span.lo
                        && self
                            .scope_span(assignment.scope())
                            .is_some_and(|scope| contains(scope, span))
                })
            })
    }

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
        self.rooted_property_ids_were_mutated_at(root.as_view(), property, span)
    }

    fn rooted_property_ids_were_mutated_at(
        &self,
        root: PathView<'_, glass_lint_datastructures::NameId>,
        property: Option<glass_lint_datastructures::NameId>,
        span: Span,
    ) -> bool {
        self.rooted_mutations(root).is_some_and(|mutations| {
            mutations.iter().any(|mutation| {
                mutation.span().lo <= span.lo
                    && mutation
                        .property()
                        .is_none_or(|written| property.is_none_or(|expected| written == expected))
                    && self
                        .scope_span(mutation.scope())
                        .is_some_and(|scope| contains(scope, span))
            })
        })
    }
}
