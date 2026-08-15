use std::collections::BTreeSet;

use swc_ecma_ast::{
    ArrowExpr, AssignExpr, BinaryOp, ClassDecl, ClassExpr, ClassProp, Function, Lit, PrivateProp,
    VarDecl, VarDeclKind,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::{EcmaFeature, EcmaVersion, EcmaVersionReport};

#[derive(Default)]
pub(super) struct FeatureDetector {
    features: BTreeSet<EcmaFeature>,
    in_parameter_pattern: bool,
}

impl FeatureDetector {
    fn record(&mut self, feature: EcmaFeature) {
        self.features.insert(feature);
    }

    pub(super) fn finish(self) -> EcmaVersionReport {
        let (has_unversioned, minimum_version) = self.features.iter().fold(
            (false, EcmaVersion::Es5),
            |(has_unversioned, minimum_version), feature| {
                let Some(version) = feature.minimum_version() else {
                    return (true, minimum_version);
                };
                (has_unversioned, minimum_version.max(version))
            },
        );
        EcmaVersionReport {
            minimum_version: (!has_unversioned).then_some(minimum_version),
            features: self.features.into_iter().collect(),
        }
    }
}

impl Visit for FeatureDetector {
    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        self.record(EcmaFeature::ArrowFunctions);
        if arrow.is_async {
            self.record(EcmaFeature::AsyncFunctions);
        }
        arrow.type_params.visit_with(self);
        arrow.return_type.visit_with(self);
        let was_in_parameter_pattern = self.in_parameter_pattern;
        self.in_parameter_pattern = true;
        arrow.params.visit_with(self);
        self.in_parameter_pattern = false;
        arrow.body.visit_with(self);
        self.in_parameter_pattern = was_in_parameter_pattern;
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        match assignment.op {
            swc_ecma_ast::AssignOp::ExpAssign => self.record(EcmaFeature::Exponentiation),
            swc_ecma_ast::AssignOp::AndAssign
            | swc_ecma_ast::AssignOp::OrAssign
            | swc_ecma_ast::AssignOp::NullishAssign => {
                self.record(EcmaFeature::LogicalAssignment);
            }
            _ => {}
        }
        assignment.visit_children_with(self);
    }

    fn visit_bin_expr(&mut self, binary: &swc_ecma_ast::BinExpr) {
        match binary.op {
            BinaryOp::Exp => self.record(EcmaFeature::Exponentiation),
            BinaryOp::NullishCoalescing => self.record(EcmaFeature::NullishCoalescing),
            _ => {}
        }
        binary.visit_children_with(self);
    }

    fn visit_class_decl(&mut self, declaration: &ClassDecl) {
        self.record(EcmaFeature::Classes);
        declaration.visit_children_with(self);
    }

    fn visit_class_expr(&mut self, expression: &ClassExpr) {
        self.record(EcmaFeature::Classes);
        expression.visit_children_with(self);
    }

    fn visit_class_prop(&mut self, property: &ClassProp) {
        self.record(EcmaFeature::ClassFields);
        property.visit_children_with(self);
    }

    fn visit_decorator(&mut self, decorator: &swc_ecma_ast::Decorator) {
        self.record(EcmaFeature::Decorators);
        decorator.visit_children_with(self);
    }

    fn visit_function(&mut self, function: &Function) {
        if function.is_async && function.is_generator {
            self.record(EcmaFeature::AsyncGenerators);
        } else if function.is_async {
            self.record(EcmaFeature::AsyncFunctions);
        }
        if function.is_generator {
            self.record(EcmaFeature::Generators);
        }
        function.decorators.visit_with(self);
        function.type_params.visit_with(self);
        function.return_type.visit_with(self);
        let was_in_parameter_pattern = self.in_parameter_pattern;
        self.in_parameter_pattern = true;
        function.params.visit_with(self);
        self.in_parameter_pattern = false;
        function.body.visit_with(self);
        self.in_parameter_pattern = was_in_parameter_pattern;
    }

    fn visit_for_of_stmt(&mut self, statement: &swc_ecma_ast::ForOfStmt) {
        if statement.is_await {
            self.record(EcmaFeature::ForAwaitOf);
        } else {
            self.record(EcmaFeature::ForOf);
        }
        statement.visit_children_with(self);
    }

    fn visit_import_decl(&mut self, declaration: &swc_ecma_ast::ImportDecl) {
        if !declaration.type_only {
            self.record(EcmaFeature::Modules);
        }
        if declaration.with.is_some() {
            self.record(EcmaFeature::ImportAttributes);
        }
        declaration.visit_children_with(self);
    }

    fn visit_named_export(&mut self, export: &swc_ecma_ast::NamedExport) {
        if !export.type_only {
            self.record(EcmaFeature::Modules);
        }
        if export.with.is_some() {
            self.record(EcmaFeature::ImportAttributes);
        }
        export.visit_children_with(self);
    }

    fn visit_export_all(&mut self, export: &swc_ecma_ast::ExportAll) {
        if !export.type_only {
            self.record(EcmaFeature::Modules);
        }
        if export.with.is_some() {
            self.record(EcmaFeature::ImportAttributes);
        }
        export.visit_children_with(self);
    }

    fn visit_export_default_specifier(&mut self, specifier: &swc_ecma_ast::ExportDefaultSpecifier) {
        self.record(EcmaFeature::ExportDefaultFrom);
        specifier.visit_children_with(self);
    }

    fn visit_export_decl(&mut self, export: &swc_ecma_ast::ExportDecl) {
        self.record(EcmaFeature::Modules);
        export.visit_children_with(self);
    }

    fn visit_export_default_decl(&mut self, export: &swc_ecma_ast::ExportDefaultDecl) {
        self.record(EcmaFeature::Modules);
        export.visit_children_with(self);
    }

    fn visit_export_default_expr(&mut self, export: &swc_ecma_ast::ExportDefaultExpr) {
        self.record(EcmaFeature::Modules);
        export.visit_children_with(self);
    }

    fn visit_lit(&mut self, literal: &Lit) {
        if matches!(literal, Lit::BigInt(_)) {
            self.record(EcmaFeature::BigInt);
        }
        literal.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &swc_ecma_ast::CallExpr) {
        if matches!(call.callee, swc_ecma_ast::Callee::Import(_)) && call.args.len() > 1 {
            self.record(EcmaFeature::ImportAttributes);
        }
        call.visit_children_with(self);
    }

    fn visit_opt_chain_expr(&mut self, chain: &swc_ecma_ast::OptChainExpr) {
        self.record(EcmaFeature::OptionalChaining);
        chain.visit_children_with(self);
    }

    fn visit_private_prop(&mut self, property: &PrivateProp) {
        self.record(EcmaFeature::PrivateClassFields);
        property.visit_children_with(self);
    }

    fn visit_private_method(&mut self, method: &swc_ecma_ast::PrivateMethod) {
        self.record(EcmaFeature::PrivateClassFields);
        method.visit_children_with(self);
    }

    fn visit_spread_element(&mut self, spread: &swc_ecma_ast::SpreadElement) {
        self.record(EcmaFeature::RestAndSpread);
        spread.visit_children_with(self);
    }

    fn visit_static_block(&mut self, block: &swc_ecma_ast::StaticBlock) {
        self.record(EcmaFeature::StaticBlocks);
        block.visit_children_with(self);
    }

    fn visit_tpl(&mut self, template: &swc_ecma_ast::Tpl) {
        self.record(EcmaFeature::TemplateLiterals);
        template.visit_children_with(self);
    }

    fn visit_var_decl(&mut self, declaration: &VarDecl) {
        if matches!(declaration.kind, VarDeclKind::Let | VarDeclKind::Const) {
            self.record(EcmaFeature::LetConst);
        }
        declaration.visit_children_with(self);
    }

    fn visit_array_pat(&mut self, pattern: &swc_ecma_ast::ArrayPat) {
        self.record(EcmaFeature::Destructuring);
        pattern.visit_children_with(self);
    }

    fn visit_object_pat(&mut self, pattern: &swc_ecma_ast::ObjectPat) {
        self.record(EcmaFeature::Destructuring);
        pattern.visit_children_with(self);
    }

    fn visit_object_pat_prop(&mut self, property: &swc_ecma_ast::ObjectPatProp) {
        if matches!(property, swc_ecma_ast::ObjectPatProp::Rest(_)) {
            self.record(EcmaFeature::ObjectRestSpread);
        }
        property.visit_children_with(self);
    }

    fn visit_assign_pat(&mut self, pattern: &swc_ecma_ast::AssignPat) {
        if self.in_parameter_pattern {
            self.record(EcmaFeature::DefaultParameters);
        }
        pattern.visit_children_with(self);
    }

    fn visit_rest_pat(&mut self, pattern: &swc_ecma_ast::RestPat) {
        self.record(EcmaFeature::RestAndSpread);
        pattern.visit_children_with(self);
    }

    fn visit_await_expr(&mut self, expression: &swc_ecma_ast::AwaitExpr) {
        self.record(EcmaFeature::Await);
        expression.visit_children_with(self);
    }

    fn visit_yield_expr(&mut self, expression: &swc_ecma_ast::YieldExpr) {
        self.record(EcmaFeature::Generators);
        expression.visit_children_with(self);
    }

    fn visit_catch_clause(&mut self, clause: &swc_ecma_ast::CatchClause) {
        if clause.param.is_none() {
            self.record(EcmaFeature::OptionalCatchBinding);
        }
        clause.visit_children_with(self);
    }

    fn visit_jsx_element(&mut self, element: &swc_ecma_ast::JSXElement) {
        self.record(EcmaFeature::Jsx);
        element.visit_children_with(self);
    }

    fn visit_jsx_fragment(&mut self, fragment: &swc_ecma_ast::JSXFragment) {
        self.record(EcmaFeature::Jsx);
        fragment.visit_children_with(self);
    }

    fn visit_using_decl(&mut self, declaration: &swc_ecma_ast::UsingDecl) {
        self.record(EcmaFeature::ExplicitResourceManagement);
        declaration.visit_children_with(self);
    }

    fn visit_auto_accessor(&mut self, accessor: &swc_ecma_ast::AutoAccessor) {
        self.record(EcmaFeature::AutoAccessors);
        accessor.visit_children_with(self);
    }

    fn visit_prop_or_spread(&mut self, property: &swc_ecma_ast::PropOrSpread) {
        if matches!(property, swc_ecma_ast::PropOrSpread::Spread(_)) {
            self.record(EcmaFeature::ObjectRestSpread);
        }
        property.visit_children_with(self);
    }
}
