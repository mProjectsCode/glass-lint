use glass_lint_datastructures::SymbolPath;
use smol_str::{SmolStr, ToSmolStr};

use crate::analysis::{
    scope::query::{
        BindingKey, BindingProvenance, FrozenScopeGraph, Ident, IdentValueSeed, Lookup, MemberExpr,
        Span, SymbolCallProvenance, SymbolMemberProvenance, constant,
    },
    syntax::{expression_name, member_root_identifier},
    value::BindingRoot,
};

impl FrozenScopeGraph {
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
            Some(BindingProvenance::DefaultImport { module }) => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: "default".into(),
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
                | BindingProvenance::ConstructedInstance { .. }
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

    pub(in crate::analysis) fn ident_value_seed(&self, ident: &Ident) -> IdentValueSeed {
        let binding = self
            .binding_with_scope_at(ident.sym.as_ref(), ident.span)
            .and_then(|(scope, _)| {
                Some(BindingKey::new(BindingRoot::Binding {
                    function: self.function_scope_at(scope),
                    binding: self.binding_id_at(scope, self.name_id(ident.sym.as_ref())?)?,
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

    pub(in crate::analysis) fn contextual_member_property_name(
        &self,
        member: &MemberExpr,
    ) -> Option<SmolStr> {
        constant::contextual_member_property_name(&member.prop, self)
    }

    pub(in crate::analysis) fn member_expression_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<SymbolPath> {
        let object = expression_name(&member.obj)?;
        Some(object.append_chain(&self.contextual_member_property_name(member)?))
    }

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

    pub(in crate::analysis) fn module_export_for_chain(
        &self,
        chain: &str,
        span: Span,
    ) -> Option<SymbolCallProvenance> {
        let (root, export) = chain.split_once('.')?;
        match self.binding_at(root, span)? {
            BindingProvenance::DefaultImport { module }
            | BindingProvenance::ModuleNamespace { module } => {
                Some(SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.to_smolstr(),
                })
            }
            _ => None,
        }
    }

    pub(in crate::analysis) fn member_call_provenance_for_chain(
        &self,
        member: &MemberExpr,
        chain: &SymbolPath,
    ) -> Option<SymbolMemberProvenance> {
        if self.has_dynamic_lookup_at(member.span) {
            return None;
        }
        if let Some((module, member)) = self.module_member_for_member(member) {
            return Some(SymbolMemberProvenance::ModuleNamespace { module, member });
        }
        let root = member_root_identifier(member)?;
        if chain.first_segment().is_none_or(|s| s != root.sym.as_ref()) {
            return None;
        }
        let member = chain
            .as_view()
            .tail_after(1)?
            .segments()
            .iter()
            .map(SmolStr::as_str)
            .collect::<Vec<_>>()
            .join(".");
        match self.binding_at(root.sym.as_ref(), root.span) {
            Some(
                BindingProvenance::DefaultImport { module }
                | BindingProvenance::ModuleNamespace { module },
            ) => Some(SymbolMemberProvenance::ModuleNamespace {
                module: module.clone(),
                member: member.to_smolstr(),
            }),
            _ => None,
        }
    }
}
