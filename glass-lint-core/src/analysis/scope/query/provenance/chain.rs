use glass_lint_datastructures::SymbolPath;

use crate::analysis::{
    scope::query::{BindingKey, BindingProvenance, FrozenScopeGraph, MemberExpr, Span, contains},
    syntax::{expression_name, member_root_identifier},
};

impl FrozenScopeGraph {
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
            let property = self.member_property_name(member)?;
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
        let syntactic_chain = self.member_expression_chain(member).or_else(|| {
            let object = expression_name(&member.obj)?;
            let property = self.member_property_name(member)?;
            Some(object.append_chain(&property))
        })?;
        let root = member_root_identifier(member)?;
        if self
            .binding_alternatives_at(root.sym.as_ref(), root.span)
            .is_empty()
            && self.is_global(root.sym.as_ref())
        {
            return Some(syntactic_chain);
        }
        self.resolve_member_chain(member, &syntactic_chain)
    }

    // Kept as a single linear algorithm: the prefix-backtracking loop and
    // closing match over provenance share mutable symbol-path state.
    pub fn resolve_member_chain(
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
        let segments = syntactic_chain.segments();
        let name_path = self.name_path(syntactic_chain)?;
        let name_segments = name_path.segments();

        for prefix_end in (2..=segments.len()).rev() {
            let Some(assignments) =
                self.property_aliases(&receiver_key, &name_segments[1..prefix_end])
            else {
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
        let alternatives = self.binding_alternatives_at(root.sym.as_ref(), root.span);
        for provenance in &alternatives {
            let target = match provenance {
                BindingProvenance::ValueAlias { target }
                | BindingProvenance::BoundCallable { target, .. } => target,
                BindingProvenance::ReturnedObject { source } => source,
                BindingProvenance::Local
                | BindingProvenance::ModuleExport { .. }
                | BindingProvenance::ModuleNamespace { .. }
                | BindingProvenance::ConstructedInstance { .. }
                | BindingProvenance::BoundModuleCallable { .. }
                | BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_) => continue,
            };
            if self.rooted_path_available(target)
                && let Some(path) = self.symbol_path(target)
            {
                return Some(path.append_path(&suffix));
            }
        }
        if alternatives.is_empty() && self.is_global(root.sym.as_ref()) {
            self.rooted_chain_available_at(syntactic_chain, member.span)
        } else {
            None
        }
    }

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

    fn rooted_chain_mutated_at(&self, chain: &SymbolPath, span: Span) -> bool {
        let Some(path) = self.name_path(chain) else {
            return false;
        };
        let segments = path.segments();
        if segments.len() < 2 {
            return false;
        }

        let first = segments[0];
        if self.resolve_name_id(first).is_some_and(|first_name| {
            self.global_objects()
                .filter(|root| self.is_global_member(root, &first_name))
                .filter_map(|root| self.name_id(root))
                .any(|root| {
                    self.rooted_property_ids_were_mutated_at(
                        std::slice::from_ref(&root),
                        Some(first),
                        span,
                    )
                })
        }) {
            return true;
        }

        (1..segments.len()).any(|end| {
            self.rooted_property_ids_were_mutated_at(&segments[..end], Some(segments[end]), span)
        })
    }

    pub(in crate::analysis) fn instance_member_available_at(&self, member: &MemberExpr) -> bool {
        let Some(property) = self.member_property_name(member) else {
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
        self.property_aliases(receiver, path.segments())
            .is_some_and(|assignments| {
                assignments.iter().any(|assignment| {
                    assignment.span.lo <= span.lo
                        && self
                            .scope_span(assignment.scope)
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
        self.rooted_property_ids_were_mutated_at(root.segments(), property, span)
    }

    fn rooted_property_ids_were_mutated_at(
        &self,
        root: &[glass_lint_datastructures::NameId],
        property: Option<glass_lint_datastructures::NameId>,
        span: Span,
    ) -> bool {
        self.rooted_mutations(root).is_some_and(|mutations| {
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
