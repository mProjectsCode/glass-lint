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

#[derive(Clone, Copy)]
struct PatternWalkContext {
    path: PathId,
    path_known: bool,
    default: Option<ValueId>,
    rest: bool,
}

impl PatternWalkContext {
    fn new(path: PathId, default: Option<ValueId>, rest: bool) -> Self {
        Self {
            path,
            path_known: true,
            default,
            rest,
        }
    }

    fn invalidate_path(&mut self) {
        self.path = PathId::EMPTY;
        self.path_known = false;
    }

    fn append_segment(&mut self, builder: &mut FactBuilder<'_, '_>, segment: PathSegmentInput<'_>) {
        if !self.path_known {
            return;
        }
        self.path = builder.append_path(self.path, segment);
        self.path_known = !self.path.is_empty();
    }
}

impl FactBuilder<'_, '_> {
    pub(in crate::analysis::facts) fn pattern_values(
        &mut self,
        pattern: &Pat,
        values: &mut Vec<ValueId>,
    ) {
        self.walk_pattern(
            pattern,
            PatternWalkContext::new(PathId::EMPTY, None, false),
            &mut |leaf| {
                if let PatternLeafKind::Binding(value) = leaf.kind {
                    values.push(value);
                }
            },
        );
    }

    pub(in crate::analysis::facts) fn pattern_write_targets(
        &mut self,
        pattern: &Pat,
        targets: &mut Vec<(ValueId, Option<ValueId>)>,
    ) {
        self.walk_pattern(
            pattern,
            PatternWalkContext::new(PathId::EMPTY, None, false),
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
        self.walk_pattern(
            pattern,
            PatternWalkContext {
                path,
                path_known: true,
                default,
                rest,
            },
            &mut |leaf| {
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
            },
        );
    }

    fn walk_pattern(
        &mut self,
        pattern: &Pat,
        context: PatternWalkContext,
        visit_leaf: &mut impl FnMut(PatternLeaf),
    ) {
        match pattern {
            Pat::Ident(ident) => visit_leaf(PatternLeaf {
                kind: PatternLeafKind::Binding(self.resolver.resolve_ident_id(&ident.id)),
                path: context.path,
                path_known: context.path_known,
                default: context.default,
                rest: context.rest,
            }),
            Pat::Assign(assign) => {
                let assigned_value = self.resolver.resolve_expr_id(&assign.right);
                let mut context = context;
                context.default = Some(assigned_value);
                self.walk_pattern(&assign.left, context, visit_leaf);
            }
            Pat::Rest(rest_pattern) => {
                let mut context = context;
                context.rest = true;
                self.walk_pattern(&rest_pattern.arg, context, visit_leaf);
            }
            Pat::Array(array) => self.walk_array_pattern(array, context, visit_leaf),
            Pat::Object(object) => self.walk_object_pattern(object, context, visit_leaf),
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
                    path: context.path,
                    path_known: context.path_known,
                    default: context.default,
                    rest: context.rest,
                });
            }
            Pat::Invalid(_) => {}
        }
    }

    fn walk_array_pattern(
        &mut self,
        array: &swc_ecma_ast::ArrayPat,
        context: PatternWalkContext,
        visit_leaf: &mut impl FnMut(PatternLeaf),
    ) {
        for (index, element) in array.elems.iter().enumerate() {
            let Some(element) = element else { continue };
            let mut child = context;
            match u32::try_from(index) {
                Ok(index) => child.append_segment(self, PathSegmentInput::Index(index)),
                Err(_) => child.invalidate_path(),
            }
            self.walk_pattern(element, child, visit_leaf);
        }
    }

    fn walk_object_pattern(
        &mut self,
        object: &swc_ecma_ast::ObjectPat,
        context: PatternWalkContext,
        visit_leaf: &mut impl FnMut(PatternLeaf),
    ) {
        for property in &object.props {
            self.walk_object_property(property, context, visit_leaf);
        }
    }

    fn walk_object_property(
        &mut self,
        property: &swc_ecma_ast::ObjectPatProp,
        context: PatternWalkContext,
        visit_leaf: &mut impl FnMut(PatternLeaf),
    ) {
        match property {
            swc_ecma_ast::ObjectPatProp::KeyValue(property) => {
                let mut child = context;
                if let Some(name) = literal_property_name(&property.key) {
                    child.append_segment(self, PathSegmentInput::Property(name.as_str()));
                } else {
                    child.invalidate_path();
                }
                self.walk_pattern(&property.value, child, visit_leaf);
            }
            swc_ecma_ast::ObjectPatProp::Assign(property) => {
                let mut child = context;
                child.append_segment(self, PathSegmentInput::Property(property.key.sym.as_ref()));
                visit_leaf(PatternLeaf {
                    kind: PatternLeafKind::Binding(
                        self.resolver.resolve_ident_id(&property.key.id),
                    ),
                    path: child.path,
                    path_known: child.path_known,
                    default: property
                        .value
                        .as_deref()
                        .map(|value| self.resolver.resolve_expr_id(value)),
                    rest: child.rest,
                });
            }
            swc_ecma_ast::ObjectPatProp::Rest(property) => {
                let mut child = context;
                child.rest = true;
                self.walk_pattern(&property.arg, child, visit_leaf);
            }
        }
    }

    pub(in crate::analysis::facts) fn is_simple_pattern(pattern: &Pat) -> bool {
        matches!(pattern, Pat::Ident(_))
    }
}
