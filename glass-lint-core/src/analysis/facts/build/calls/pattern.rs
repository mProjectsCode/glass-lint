use crate::analysis::{
    facts::{
        build::{Expr, FactBuilder, Pat, PathId, PathSegmentInput, ValueId},
        ParameterBinding,
    },
};

impl FactBuilder<'_, '_> {
    pub(in crate::analysis::facts::build) fn pattern_values(
        &mut self,
        pattern: &Pat,
        values: &mut Vec<ValueId>,
    ) {
        match pattern {
            Pat::Ident(ident) => values.push(self.resolver.resolve_ident_id(&ident.id)),
            Pat::Assign(assign) => self.pattern_values(&assign.left, values),
            Pat::Rest(rest) => self.pattern_values(&rest.arg, values),
            Pat::Array(array) => {
                for element in array.elems.iter().flatten() {
                    self.pattern_values(element, values);
                }
            }
            Pat::Object(object) => {
                for property in &object.props {
                    match property {
                        swc_ecma_ast::ObjectPatProp::KeyValue(property) => {
                            self.pattern_values(&property.value, values);
                        }
                        swc_ecma_ast::ObjectPatProp::Assign(property) => {
                            values.push(self.resolver.resolve_ident_id(&property.key.id));
                        }
                        swc_ecma_ast::ObjectPatProp::Rest(property) => {
                            self.pattern_values(&property.arg, values);
                        }
                    }
                }
            }
            Pat::Expr(_) | Pat::Invalid(_) => {}
        }
    }

    pub(in crate::analysis::facts::build) fn pattern_write_targets(
        &mut self,
        pattern: &Pat,
        targets: &mut Vec<(ValueId, Option<ValueId>)>,
    ) {
        match pattern {
            Pat::Ident(ident) => targets.push((self.resolver.resolve_ident_id(&ident.id), None)),
            Pat::Assign(assign) => self.pattern_write_targets(&assign.left, targets),
            Pat::Rest(rest) => self.pattern_write_targets(&rest.arg, targets),
            Pat::Array(array) => {
                for element in array.elems.iter().flatten() {
                    self.pattern_write_targets(element, targets);
                }
            }
            Pat::Object(object) => {
                for property in &object.props {
                    match property {
                        swc_ecma_ast::ObjectPatProp::KeyValue(property) => {
                            self.pattern_write_targets(&property.value, targets);
                        }
                        swc_ecma_ast::ObjectPatProp::Assign(property) => {
                            targets.push((self.resolver.resolve_ident_id(&property.key.id), None));
                        }
                        swc_ecma_ast::ObjectPatProp::Rest(property) => {
                            self.pattern_write_targets(&property.arg, targets);
                        }
                    }
                }
            }
            Pat::Expr(expr) => {
                if let Expr::Member(member) = &**expr {
                    targets.push((
                        self.resolver.resolve_member_id(member),
                        Some(self.resolver.resolve_expr_id(&member.obj)),
                    ));
                } else {
                    targets.push((self.resolver.resolve_expr_id(expr), None));
                }
            }
            Pat::Invalid(_) => {}
        }
    }

    pub(in crate::analysis::facts::build) fn parameter_bindings(
        &mut self,
        pattern: &Pat,
        parameter_index: usize,
        path: PathId,
        default: Option<ValueId>,
        rest: bool,
        output: &mut Vec<ParameterBinding>,
    ) {
        match pattern {
            Pat::Ident(ident) => output.push(ParameterBinding {
                parameter_index,
                path,
                value: self.resolver.resolve_ident_id(&ident.id),
                default,
                rest,
            }),
            Pat::Assign(assign) => {
                let assigned_value = self.resolver.resolve_expr_id(&assign.right);
                self.parameter_bindings(
                    &assign.left,
                    parameter_index,
                    path,
                    Some(assigned_value),
                    rest,
                    output,
                );
            }
            Pat::Rest(rest_pattern) => self.parameter_bindings(
                &rest_pattern.arg,
                parameter_index,
                path,
                default,
                true,
                output,
            ),
            Pat::Array(array) => {
                for (index, element) in array.elems.iter().enumerate() {
                    let Some(element) = element else { continue };
                    let Ok(index) = u32::try_from(index) else {
                        continue;
                    };
                    let path = self.append_path(path, PathSegmentInput::Index(index));
                    self.parameter_bindings(element, parameter_index, path, default, rest, output);
                }
            }
            Pat::Object(object) => {
                for property in &object.props {
                    self.record_object_pat_property(
                        property,
                        parameter_index,
                        path,
                        default,
                        rest,
                        output,
                    );
                }
            }
            Pat::Expr(_) | Pat::Invalid(_) => {}
        }
    }

    fn record_object_pat_property(
        &mut self,
        property: &swc_ecma_ast::ObjectPatProp,
        parameter_index: usize,
        path: PathId,
        default: Option<ValueId>,
        rest: bool,
        output: &mut Vec<ParameterBinding>,
    ) {
        match property {
            swc_ecma_ast::ObjectPatProp::KeyValue(property) => {
                let Some(name) = crate::analysis::syntax::property_name(&property.key) else {
                    return;
                };
                let path = self.append_path(path, PathSegmentInput::Property(name.as_str()));
                self.parameter_bindings(
                    &property.value,
                    parameter_index,
                    path,
                    default,
                    rest,
                    output,
                );
            }
            swc_ecma_ast::ObjectPatProp::Assign(property) => {
                let path =
                    self.append_path(path, PathSegmentInput::Property(property.key.sym.as_ref()));
                output.push(ParameterBinding {
                    parameter_index,
                    path,
                    value: self.resolver.resolve_ident_id(&property.key.id),
                    default: property
                        .value
                        .as_deref()
                        .map(|value| self.resolver.resolve_expr_id(value)),
                    rest,
                });
            }
            swc_ecma_ast::ObjectPatProp::Rest(property) => {
                self.parameter_bindings(
                    &property.arg,
                    parameter_index,
                    path,
                    default,
                    true,
                    output,
                );
            }
        }
    }

    pub(in crate::analysis::facts::build) fn is_simple_pattern(pattern: &Pat) -> bool {
        matches!(pattern, Pat::Ident(_))
    }
}
