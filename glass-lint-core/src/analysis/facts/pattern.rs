use glass_lint_datastructures::{PathId, PathSegmentInput};

use crate::analysis::{
    facts::{Expr, FactBuilder, ParameterBinding, Pat, ValueId},
    syntax::literal_property_name,
};

#[derive(Clone, Copy)]
enum PatternLeafKind {
    Binding(ValueId),
    Expression {
        value: ValueId,
        receiver: Option<ValueId>,
    },
}

#[derive(Clone, Copy)]
struct PatternLeaf {
    kind: PatternLeafKind,
    path: PathId,
    path_known: bool,
    default: Option<ValueId>,
    rest: bool,
}

impl FactBuilder<'_, '_> {
    pub(in crate::analysis::facts) fn pattern_values(
        &mut self,
        pattern: &Pat,
        values: &mut Vec<ValueId>,
    ) {
        self.walk_pattern(pattern, PathId::EMPTY, true, None, false, &mut |leaf| {
            if let PatternLeafKind::Binding(value) = leaf.kind {
                values.push(value);
            }
        });
    }

    pub(in crate::analysis::facts) fn pattern_write_targets(
        &mut self,
        pattern: &Pat,
        targets: &mut Vec<(ValueId, Option<ValueId>)>,
    ) {
        self.walk_pattern(
            pattern,
            PathId::EMPTY,
            true,
            None,
            false,
            &mut |leaf| match leaf.kind {
                PatternLeafKind::Binding(value) => targets.push((value, None)),
                PatternLeafKind::Expression { value, receiver } => {
                    targets.push((value, receiver));
                }
            },
        );
    }

    pub(in crate::analysis::facts) fn parameter_bindings(
        &mut self,
        pattern: &Pat,
        parameter_index: usize,
        path: PathId,
        default: Option<ValueId>,
        rest: bool,
        output: &mut Vec<ParameterBinding>,
    ) {
        self.walk_pattern(pattern, path, true, default, rest, &mut |leaf| {
            let PatternLeafKind::Binding(value) = leaf.kind else {
                return;
            };
            if leaf.path_known {
                output.push(ParameterBinding::new(
                    parameter_index,
                    leaf.path,
                    value,
                    leaf.default,
                    leaf.rest,
                ));
            }
        });
    }

    fn walk_pattern(
        &mut self,
        pattern: &Pat,
        path: PathId,
        path_known: bool,
        default: Option<ValueId>,
        rest: bool,
        visit_leaf: &mut impl FnMut(PatternLeaf),
    ) {
        match pattern {
            Pat::Ident(ident) => visit_leaf(PatternLeaf {
                kind: PatternLeafKind::Binding(self.resolver.resolve_ident_id(&ident.id)),
                path,
                path_known,
                default,
                rest,
            }),
            Pat::Assign(assign) => {
                let assigned_value = self.resolver.resolve_expr_id(&assign.right);
                self.walk_pattern(
                    &assign.left,
                    path,
                    path_known,
                    Some(assigned_value),
                    rest,
                    visit_leaf,
                );
            }
            Pat::Rest(rest_pattern) => self.walk_pattern(
                &rest_pattern.arg,
                path,
                path_known,
                default,
                true,
                visit_leaf,
            ),
            Pat::Array(array) => {
                self.walk_array_pattern(array, path, path_known, default, rest, visit_leaf);
            }
            Pat::Object(object) => {
                self.walk_object_pattern(object, path, path_known, default, rest, visit_leaf);
            }
            Pat::Expr(expr) => {
                let kind = if let Expr::Member(member) = &**expr {
                    PatternLeafKind::Expression {
                        value: self.resolver.resolve_member_id(member),
                        receiver: Some(self.resolver.resolve_expr_id(&member.obj)),
                    }
                } else {
                    PatternLeafKind::Expression {
                        value: self.resolver.resolve_expr_id(expr),
                        receiver: None,
                    }
                };
                visit_leaf(PatternLeaf {
                    kind,
                    path,
                    path_known,
                    default,
                    rest,
                });
            }
            Pat::Invalid(_) => {}
        }
    }

    fn walk_array_pattern(
        &mut self,
        array: &swc_ecma_ast::ArrayPat,
        path: PathId,
        path_known: bool,
        default: Option<ValueId>,
        rest: bool,
        visit_leaf: &mut impl FnMut(PatternLeaf),
    ) {
        for (index, element) in array.elems.iter().enumerate() {
            let Some(element) = element else { continue };
            let (path, path_known) = match u32::try_from(index) {
                Ok(index) if path_known => {
                    (self.append_path(path, PathSegmentInput::Index(index)), true)
                }
                Ok(_) | Err(_) => (PathId::EMPTY, false),
            };
            self.walk_pattern(element, path, path_known, default, rest, visit_leaf);
        }
    }

    fn walk_object_pattern(
        &mut self,
        object: &swc_ecma_ast::ObjectPat,
        path: PathId,
        path_known: bool,
        default: Option<ValueId>,
        rest: bool,
        visit_leaf: &mut impl FnMut(PatternLeaf),
    ) {
        for property in &object.props {
            self.walk_object_property(property, path, path_known, default, rest, visit_leaf);
        }
    }

    fn walk_object_property(
        &mut self,
        property: &swc_ecma_ast::ObjectPatProp,
        path: PathId,
        path_known: bool,
        default: Option<ValueId>,
        rest: bool,
        visit_leaf: &mut impl FnMut(PatternLeaf),
    ) {
        match property {
            swc_ecma_ast::ObjectPatProp::KeyValue(property) => {
                let (path, path_known) = match literal_property_name(&property.key) {
                    Some(name) if path_known => (
                        self.append_path(path, PathSegmentInput::Property(name.as_str())),
                        true,
                    ),
                    Some(_) | None => (PathId::EMPTY, false),
                };
                self.walk_pattern(&property.value, path, path_known, default, rest, visit_leaf);
            }
            swc_ecma_ast::ObjectPatProp::Assign(property) => {
                let path = if path_known {
                    self.append_path(path, PathSegmentInput::Property(property.key.sym.as_ref()))
                } else {
                    PathId::EMPTY
                };
                visit_leaf(PatternLeaf {
                    kind: PatternLeafKind::Binding(
                        self.resolver.resolve_ident_id(&property.key.id),
                    ),
                    path,
                    path_known,
                    default: property
                        .value
                        .as_deref()
                        .map(|value| self.resolver.resolve_expr_id(value)),
                    rest,
                });
            }
            swc_ecma_ast::ObjectPatProp::Rest(property) => {
                self.walk_pattern(&property.arg, path, path_known, default, true, visit_leaf);
            }
        }
    }

    pub(in crate::analysis::facts) fn is_simple_pattern(pattern: &Pat) -> bool {
        matches!(pattern, Pat::Ident(_))
    }
}
