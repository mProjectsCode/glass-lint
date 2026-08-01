//! Provenance inference for bindings and call-site expressions.
//!
//! Each helper returns `None` when syntax, lexical identity, or value flow is
//! not provable. The visitor can then record a local/unknown binding instead
//! of widening a strict match from a name-only resemblance.

use std::collections::BTreeMap;

use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{Callee, Expr, OptChainBase};

use crate::analysis::{
    module_request::{
        ModuleRequestContext, ModuleRequestPolicy, is_interop_wrapper, recognize_module_expression,
    },
    scope::{
        BoundArgument,
        build::{BindingProvenance, ScopeCollector},
        query::rooted::RootedExprContext,
    },
    syntax::{
        constant::{self, ConstValue},
        member_property_name, property_name,
    },
};

impl ScopeCollector<'_> {
    pub(super) fn constructed_instance_provenance(
        &mut self,
        expr: &Expr,
    ) -> Option<BindingProvenance> {
        let Expr::New(new_expr) = expr else {
            return None;
        };
        match self.module_alias_provenance(&new_expr.callee)? {
            BindingProvenance::ModuleExport { module, export } => {
                Some(BindingProvenance::ConstructedInstance { module, export })
            }
            _ => None,
        }
    }

    /// Resolve a module export, namespace member, dynamic import, or require
    /// expression while preserving lexical shadowing checks.
    // Kept as a single recursive match: each Expr arm follows a distinct
    // provenance rule, and extracting individual arms would scatter the logic.
    pub(super) fn module_alias_provenance(&mut self, expr: &Expr) -> Option<BindingProvenance> {
        // Recursive dispatch over Expr variants. Each arm has a distinct
        // resolution strategy; keeping them together makes the full recursion
        // visible in one place.
        match expr {
            Expr::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                provenance @ (BindingProvenance::ModuleExport { .. }
                | BindingProvenance::DefaultImport { .. }
                | BindingProvenance::ModuleNamespace { .. }) => Some(provenance.clone()),
                _ => None,
            },
            Expr::Member(member) => {
                let property = member_property_name(&member.prop)?;
                let provenance = self.module_alias_provenance(&member.obj)?;
                match provenance {
                    BindingProvenance::DefaultImport { module }
                    | BindingProvenance::ModuleNamespace { module } => {
                        Some(BindingProvenance::ModuleExport {
                            module,
                            export: property,
                        })
                    }
                    BindingProvenance::ModuleExport { module, export } if property == "bind" => {
                        Some(BindingProvenance::ModuleExport { module, export })
                    }
                    BindingProvenance::ModuleExport { module, export } => {
                        Some(BindingProvenance::ModuleExport {
                            module,
                            export: format!("{export}.{property}").into(),
                        })
                    }
                    _ => None,
                }
            }
            Expr::Call(call) => self
                .module_request_name(expr, ModuleRequestPolicy::alias_with_dynamic_import())
                .map(|module| BindingProvenance::ModuleNamespace { module })
                .or_else(|| {
                    let Callee::Expr(callee) = &call.callee else {
                        return None;
                    };
                    let Expr::Member(member) = &**callee else {
                        return None;
                    };
                    (member_property_name(&member.prop).as_deref() == Some("bind"))
                        .then(|| self.module_alias_provenance(&member.obj))
                        .flatten()
                }),
            Expr::Await(await_expr) => self.module_alias_provenance(&await_expr.arg),
            Expr::Paren(paren) => self.module_alias_provenance(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.module_alias_provenance(expr)),
            _ => None,
        }
    }

    /// Resolve literal CommonJS/interop-loader module names only.
    fn module_request_name(&mut self, expr: &Expr, policy: ModuleRequestPolicy) -> Option<SmolStr> {
        let request = recognize_module_expression(expr, self, policy)?;
        Some(request.module().to_smolstr())
    }

    /// Find a literal CommonJS module name through the supported alias shapes.
    pub(super) fn require_module_expr_name(&mut self, expr: &Expr) -> Option<SmolStr> {
        self.module_request_name(expr, ModuleRequestPolicy::alias())
    }

    /// Convert a bounded constant result into collector provenance.
    pub(super) fn const_provenance(&mut self, init: &Expr) -> Option<BindingProvenance> {
        match constant::evaluate(init, self) {
            ConstValue::String(value) => Some(BindingProvenance::StaticString(value)),
            ConstValue::NonNegativeInteger(value) => Some(BindingProvenance::StaticNumber(value)),
            ConstValue::Array(values) => Some(BindingProvenance::StaticStringArray(
                values
                    .into_iter()
                    .map(|value| value.string().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()?,
            )),
            ConstValue::Object(values) => Some(BindingProvenance::StaticObjectKeys(
                values
                    .keys()
                    .map(|key| self.lookup_or_intern_name(key).ok_or(()))
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?,
            )),
            ConstValue::Unknown => None,
        }
    }

    /// Resolve the strict provenance forms accepted for a call argument.
    pub(super) fn argument_provenance(&mut self, expr: &Expr) -> Option<BindingProvenance> {
        self.constructed_instance_provenance(expr)
            .or_else(|| self.module_alias_provenance(expr))
            .or_else(|| self.returned_object_provenance(expr))
            .or_else(|| match expr {
                Expr::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                    provenance @ BindingProvenance::StaticObjectValues(_) => {
                        Some(provenance.clone())
                    }
                    _ => None,
                },
                _ => None,
            })
            .or_else(|| self.static_object_values(expr))
            .or_else(|| self.const_provenance(expr))
            .or_else(|| {
                self.rooted_name_path(expr)
                    .map(|target| BindingProvenance::ValueAlias { target })
            })
    }

    /// Preserve a callable identity and supported static `.bind` arguments.
    pub(super) fn bound_callable_provenance(&mut self, expr: &Expr) -> Option<BindingProvenance> {
        let Expr::Call(call) = expr else {
            return None;
        };
        let Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let Expr::Member(member) = &**callee else {
            return None;
        };
        if member_property_name(&member.prop).as_deref() != Some("bind") {
            return None;
        }
        let module_provenance = self.module_alias_provenance(&member.obj);
        let target = if module_provenance.is_none() {
            Some(self.rooted_name_path(&member.obj)?)
        } else {
            None
        };
        let bound_arguments = call
            .args
            .iter()
            .skip(1)
            .map(|argument| {
                self.const_provenance(&argument.expr)
                    .and_then(|provenance| match provenance {
                        BindingProvenance::StaticString(value) => {
                            Some(BoundArgument::StaticString(value))
                        }
                        _ => None,
                    })
                    .or_else(|| {
                        self.rooted_name_path(&argument.expr)
                            .map(BoundArgument::RootedExpression)
                    })
            })
            .collect();
        match module_provenance {
            Some(BindingProvenance::ModuleExport { module, export }) => {
                Some(BindingProvenance::BoundModuleCallable {
                    module,
                    export,
                    bound_arguments,
                })
            }
            Some(BindingProvenance::DefaultImport { module }) => {
                Some(BindingProvenance::BoundModuleCallable {
                    module,
                    export: "default".into(),
                    bound_arguments,
                })
            }
            _ => Some(BindingProvenance::BoundCallable {
                target: target?,
                bound_arguments,
            }),
        }
    }

    /// Track an object returned from a rooted callable for later member use.
    pub(super) fn returned_object_provenance(&mut self, expr: &Expr) -> Option<BindingProvenance> {
        // Recursive match over Expr variants with shared call/member/ident
        // resolution logic that is clearest when read as a single recursion.
        match expr {
            Expr::Call(call) => {
                let Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                if let Expr::Member(member) = &**callee
                    && member_property_name(&member.prop).as_deref() == Some("bind")
                {
                    return None;
                }
                let source = match &**callee {
                    Expr::Member(member) => self
                        .rooted_member_chain(member)
                        .and_then(|path| self.name_path(&path))?,
                    _ => self.rooted_name_path(callee)?,
                };
                (!source.is_root()).then_some(BindingProvenance::ReturnedObject { source })
            }
            Expr::OptChain(chain) => {
                let OptChainBase::Call(call) = &*chain.base else {
                    return None;
                };
                if let Expr::Member(member) = &*call.callee
                    && member_property_name(&member.prop).as_deref() == Some("bind")
                {
                    return None;
                }
                let source = match &*call.callee {
                    Expr::Member(member) => self
                        .rooted_member_chain(member)
                        .and_then(|path| self.name_path(&path))?,
                    _ => self.rooted_name_path(&call.callee)?,
                };
                (!source.is_root()).then_some(BindingProvenance::ReturnedObject { source })
            }
            Expr::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                BindingProvenance::ReturnedObject { source } => {
                    Some(BindingProvenance::ReturnedObject {
                        source: source.clone(),
                    })
                }
                _ => None,
            },
            Expr::Member(member) => {
                if let Expr::Ident(ident) = &*member.obj
                    && let Some(BindingProvenance::ReturnedObject { source }) =
                        self.visible_binding(ident.sym.as_ref())
                {
                    return Some(BindingProvenance::ReturnedObject {
                        source: source.clone(),
                    });
                }
                self.rooted_name_path(expr)
                    .map(|source| BindingProvenance::ReturnedObject { source })
            }
            Expr::Paren(paren) => self.returned_object_provenance(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.returned_object_provenance(expr)),
            _ => None,
        }
    }

    /// Build a static object-value map only when every property is rooted.
    pub(super) fn static_object_values(&mut self, expr: &Expr) -> Option<BindingProvenance> {
        let Expr::Object(object) = expr else {
            return None;
        };
        let mut values = BTreeMap::new();
        for property in &object.props {
            let swc_ecma_ast::PropOrSpread::Prop(property) = property else {
                return None;
            };
            let swc_ecma_ast::Prop::KeyValue(property) = &**property else {
                return None;
            };
            let target = self.rooted_name_path(&property.value)?;
            let key = property_name(&property.key)?;
            values.insert(self.lookup_or_intern_name(key.as_str())?, target);
        }
        Some(BindingProvenance::StaticObjectValues(values))
    }
}

impl ModuleRequestContext for ScopeCollector<'_> {
    fn is_unshadowed_require(&mut self, ident: &swc_ecma_ast::Ident) -> bool {
        ident.sym == *"require" && self.is_unbound("require")
    }

    fn is_unshadowed_wrapper(&mut self, ident: &swc_ecma_ast::Ident) -> bool {
        is_interop_wrapper(ident.sym.as_ref()) && self.is_unbound(ident.sym.as_ref())
    }

    fn static_string(&mut self, expr: &Expr) -> Option<String> {
        constant::static_string(expr, self)
    }
}
