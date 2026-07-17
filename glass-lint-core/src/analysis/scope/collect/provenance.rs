//! Provenance inference for bindings and call-site expressions.
//!
//! Each helper returns `None` when syntax, lexical identity, or value flow is
//! not provable. The visitor can then record a local/unknown binding instead
//! of widening a strict match from a name-only resemblance.

use std::collections::BTreeMap;

use swc_ecma_ast::{CallExpr, Callee, Expr, Lit};

use super::{
    super::super::syntax::{
        constant::{self, ConstValue},
        member_property_name, property_name,
    },
    BindingProvenance, BoundArgument, LexicalScopeCollector,
};

impl LexicalScopeCollector {
    /// Resolve a module export, namespace member, dynamic import, or require
    /// expression while preserving lexical shadowing checks.
    pub(super) fn module_alias_provenance(&self, expr: &Expr) -> Option<BindingProvenance> {
        match expr {
            Expr::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                provenance @ (BindingProvenance::ModuleExport { .. }
                | BindingProvenance::ModuleNamespace { .. }) => Some(provenance.clone()),
                _ => None,
            },
            Expr::Member(member) => match self.module_alias_provenance(&member.obj)? {
                BindingProvenance::ModuleNamespace { module } => {
                    Some(BindingProvenance::ModuleExport {
                        module,
                        export: member_property_name(&member.prop)?,
                    })
                }
                provenance @ BindingProvenance::ModuleExport { .. }
                    if member_property_name(&member.prop).as_deref() == Some("bind") =>
                {
                    Some(provenance)
                }
                _ => None,
            },
            Expr::Call(call) => self
                .require_module_name(call)
                .map(|module| BindingProvenance::ModuleNamespace { module })
                .or_else(|| {
                    if matches!(call.callee, Callee::Import(_))
                        && let Some(Expr::Lit(Lit::Str(specifier))) =
                            call.args.first().map(|argument| &*argument.expr)
                    {
                        return Some(BindingProvenance::ModuleNamespace {
                            module: specifier.value.to_string_lossy().to_string(),
                        });
                    }
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
    fn require_module_name(&self, call: &CallExpr) -> Option<String> {
        self.direct_require_module_name(call).or_else(|| {
            let Callee::Expr(callee) = &call.callee else {
                return None;
            };
            let Expr::Ident(wrapper) = &**callee else {
                return None;
            };
            (Self::is_module_interop_wrapper(wrapper.sym.as_ref())
                && self.is_unbound(wrapper.sym.as_ref()))
            .then(|| call.args.first())
            .flatten()
            .and_then(|arg| self.require_module_expr_name(&arg.expr))
        })
    }

    /// Find a literal module name through supported wrapper expression shapes.
    pub(super) fn require_module_expr_name(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call(call) => self.require_module_name(call),
            Expr::Member(member) => self.require_module_expr_name(&member.obj),
            Expr::Paren(paren) => self.require_module_expr_name(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.require_module_expr_name(expr)),
            _ => None,
        }
    }

    /// Recognize an unshadowed direct `require("literal")` call.
    fn direct_require_module_name(&self, call: &CallExpr) -> Option<String> {
        let Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let Expr::Ident(ident) = &**callee else {
            return None;
        };
        if ident.sym != *"require" || !self.is_unbound("require") {
            return None;
        }
        call.args.first().and_then(|arg| match &*arg.expr {
            Expr::Lit(Lit::Str(value)) => Some(value.value.to_string_lossy().to_string()),
            _ => None,
        })
    }

    /// Convert a bounded constant result into collector provenance.
    pub(super) fn const_provenance(&self, init: &Expr) -> Option<BindingProvenance> {
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
                values.keys().cloned().collect(),
            )),
            ConstValue::Unknown => None,
        }
    }

    /// Resolve the strict provenance forms accepted for a call argument.
    pub(super) fn argument_provenance(&self, expr: &Expr) -> Option<BindingProvenance> {
        self.module_alias_provenance(expr)
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
                self.rooted_expr_name(expr)
                    .map(|target| BindingProvenance::ValueAlias {
                        target: target.into(),
                    })
            })
    }

    /// Preserve a callable identity and supported static `.bind` arguments.
    pub(super) fn bound_callable_provenance(&self, expr: &Expr) -> Option<BindingProvenance> {
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
        let target = self.rooted_expr_name(&member.obj)?;
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
                        self.rooted_expr_name(&argument.expr)
                            .map(|value| BoundArgument::RootedExpression(value.into()))
                    })
            })
            .collect();
        match self.module_alias_provenance(&member.obj) {
            Some(BindingProvenance::ModuleExport { module, export }) => {
                Some(BindingProvenance::BoundModuleCallable {
                    module,
                    export,
                    bound_arguments,
                })
            }
            _ => Some(BindingProvenance::BoundCallable {
                target: target.into(),
                bound_arguments,
            }),
        }
    }

    /// Track an object returned from a rooted callable for later member use.
    pub(super) fn returned_object_provenance(&self, expr: &Expr) -> Option<BindingProvenance> {
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
                let source = self.rooted_expr_name(callee)?;
                source
                    .contains('.')
                    .then_some(BindingProvenance::ReturnedObject {
                        source: source.into(),
                    })
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
                self.rooted_expr_name(expr)
                    .map(|source| BindingProvenance::ReturnedObject {
                        source: source.into(),
                    })
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
    pub(super) fn static_object_values(&self, expr: &Expr) -> Option<BindingProvenance> {
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
            let target = self.rooted_expr_name(&property.value)?;
            values.insert(property_name(&property.key)?, target.into());
        }
        Some(BindingProvenance::StaticObjectValues(values))
    }
}
