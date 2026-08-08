//! Provenance inference for bindings and call-site expressions.
//!
//! Each helper returns `None` when syntax, lexical identity, or value flow is
//! not provable. The visitor can then record a local/unknown binding instead
//! of widening a strict match from a name-only resemblance.

use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{Callee, Expr};

use crate::analysis::{
    model::StaticProperties,
    module_request::{
        ModuleRequestContext, ModuleRequestPolicy, is_interop_wrapper, recognize_module_expression,
    },
    scope::{
        BoundArgument, ScopeExpression,
        build::{BindingProvenance, ScopeCollector},
        const_value_to_provenance, normalize_scope_expression,
        query::rooted::RootedExprContext,
    },
    syntax::{constant, literal_member_property_name, literal_property_name},
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
    pub(super) fn module_alias_provenance(&mut self, expr: &Expr) -> Option<BindingProvenance> {
        match normalize_scope_expression(expr)? {
            ScopeExpression::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                provenance @ (BindingProvenance::ModuleExport { .. }
                | BindingProvenance::DefaultImport { .. }
                | BindingProvenance::ModuleNamespace { .. }) => Some(provenance.clone()),
                _ => None,
            },
            ScopeExpression::Member {
                object,
                literal_property,
                ..
            } => {
                let property = literal_property?;
                let provenance = self.module_alias_provenance(object)?;
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
            ScopeExpression::Call {
                expression, callee, ..
            } => self
                .module_request_name(expression, ModuleRequestPolicy::alias_with_dynamic_import())
                .map(|module| BindingProvenance::ModuleNamespace { module })
                .or_else(|| {
                    let callee = callee?;
                    let ScopeExpression::Member {
                        object,
                        literal_property,
                        ..
                    } = normalize_scope_expression(callee)?
                    else {
                        return None;
                    };
                    (literal_property.as_deref() == Some("bind"))
                        .then(|| self.module_alias_provenance(object))
                        .flatten()
                }),
            ScopeExpression::Await { argument } => self.module_alias_provenance(argument),
            ScopeExpression::OptionalCall { .. } => None,
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
        let value = constant::evaluate(init, self);
        const_value_to_provenance(value, &mut |name| self.lookup_or_intern_name(name))
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
        if literal_member_property_name(&member.prop).as_deref() != Some("bind") {
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
        match crate::analysis::scope::normalize_scope_expression(expr)? {
            ScopeExpression::Call {
                callee: Some(callee),
                ..
            }
            | ScopeExpression::OptionalCall { callee } => self.returned_object_from_callee(callee),
            ScopeExpression::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                BindingProvenance::ReturnedObject { source } => {
                    Some(BindingProvenance::ReturnedObject {
                        source: source.clone(),
                    })
                }
                _ => None,
            },
            ScopeExpression::Member {
                expression, object, ..
            } => {
                if let Expr::Ident(ident) = object
                    && let Some(BindingProvenance::ReturnedObject { source }) =
                        self.visible_binding(ident.sym.as_ref())
                {
                    return Some(BindingProvenance::ReturnedObject {
                        source: source.clone(),
                    });
                }
                self.rooted_name_path(expression)
                    .map(|source| BindingProvenance::ReturnedObject { source })
            }
            ScopeExpression::Call { callee: None, .. } | ScopeExpression::Await { .. } => None,
        }
    }

    fn returned_object_from_callee(&mut self, callee: &Expr) -> Option<BindingProvenance> {
        if let Expr::Member(member) = callee
            && literal_member_property_name(&member.prop).as_deref() == Some("bind")
        {
            return None;
        }
        let source = match callee {
            Expr::Member(member) => self
                .rooted_member_chain(member)
                .and_then(|path| self.name_path(&path))?,
            _ => self.rooted_name_path(callee)?,
        };
        (!source.is_root()).then_some(BindingProvenance::ReturnedObject { source })
    }

    /// Build a static object-value map only when every property is rooted.
    pub(super) fn static_object_values(&mut self, expr: &Expr) -> Option<BindingProvenance> {
        let Expr::Object(object) = expr else {
            return None;
        };
        let mut values = StaticProperties::new();
        for property in &object.props {
            let swc_ecma_ast::PropOrSpread::Prop(property) = property else {
                return None;
            };
            let swc_ecma_ast::Prop::KeyValue(property) = &**property else {
                return None;
            };
            let target = self.rooted_name_path(&property.value)?;
            let key = literal_property_name(&property.key)?;
            let name = self.lookup_or_intern_name(key.as_str())?;
            if !values.insert(name, target) {
                return None;
            }
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
