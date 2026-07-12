//! Member-chain resolution and property-alias identity.

use swc_ecma_ast::MemberExpr;

use super::super::ast::member_root_ident;
use super::collector_helpers::{contains, member_prefix_ends};
use super::{BindingProvenance, ScopeGraph};

impl ScopeGraph {
    pub(crate) fn rooted_member_chain(&self, member: &MemberExpr) -> Option<String> {
        let syntactic_chain = self.member_chain(member).or_else(|| {
            let object = super::super::ast::expr_name(&member.obj)?;
            let property = self.member_prop_name(member)?;
            Some(format!("{object}.{property}"))
        })?;
        self.resolve_member_chain(member, &syntactic_chain)
    }

    pub(crate) fn resolve_member_chain(
        &self,
        member: &MemberExpr,
        syntactic_chain: &str,
    ) -> Option<String> {
        if self.has_dynamic_lookup_at(member.span) {
            return None;
        }
        let Some(root) = member_root_ident(member) else {
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
            let Some(assignments) = self.property_assignments.get(&(receiver_key.clone(), path))
            else {
                continue;
            };
            let prior_count =
                assignments.partition_point(|assignment| assignment.span.lo <= member.span.lo);
            if let Some(assignment) = assignments[..prior_count]
                .iter()
                .rev()
                .find(|assignment| contains(self.scopes[assignment.scope].span, member.span))
            {
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
            Some(BindingProvenance::ValueAlias { target }) => {
                Some(target.append_chain(suffix).to_string())
            }
            Some(BindingProvenance::BoundCallable { target, .. }) => {
                Some(target.append_chain(suffix).to_string())
            }
            Some(BindingProvenance::ReturnedObject { source }) => {
                Some(source.append_chain(suffix).to_string())
            }
            Some(
                BindingProvenance::Local
                | BindingProvenance::ModuleExport { .. }
                | BindingProvenance::ModuleNamespace { .. }
                | BindingProvenance::BoundModuleCallable { .. },
            )
            | Some(
                BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_),
            ) => None,
            None => Some(syntactic_chain.to_string()),
        }
    }
}
