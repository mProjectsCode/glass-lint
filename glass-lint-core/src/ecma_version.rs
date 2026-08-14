//! Minimum ECMAScript syntax-version analysis.
//!
//! This is deliberately a syntax analysis. It does not infer host APIs or
//! runtime built-ins, and SWC AST types remain private to the core crate.

use std::{collections::BTreeSet, fmt};

use swc_ecma_ast::{
    ArrowExpr, AssignExpr, BinaryOp, ClassDecl, ClassExpr, ClassProp, Function, Lit, PrivateProp,
    Program, VarDecl, VarDeclKind,
};
use swc_ecma_visit::{Visit, VisitWith};

use crate::{AnalysisLimits, ParseDiagnostic, project::SourceFile};

/// The ECMAScript edition required by a source's standard syntax.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum EcmaVersion {
    /// ECMAScript 5.
    Es5,
    /// ECMAScript 2015 (ES6).
    Es2015,
    /// ECMAScript 2016.
    Es2016,
    /// ECMAScript 2017.
    Es2017,
    /// ECMAScript 2018.
    Es2018,
    /// ECMAScript 2019.
    Es2019,
    /// ECMAScript 2020.
    Es2020,
    /// ECMAScript 2021.
    Es2021,
    /// ECMAScript 2022.
    Es2022,
    /// ECMAScript 2023.
    Es2023,
    /// ECMAScript 2024.
    Es2024,
}

impl fmt::Display for EcmaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Es5 => "ES5",
            Self::Es2015 => "ES2015",
            Self::Es2016 => "ES2016",
            Self::Es2017 => "ES2017",
            Self::Es2018 => "ES2018",
            Self::Es2019 => "ES2019",
            Self::Es2020 => "ES2020",
            Self::Es2021 => "ES2021",
            Self::Es2022 => "ES2022",
            Self::Es2023 => "ES2023",
            Self::Es2024 => "ES2024",
        };
        formatter.write_str(name)
    }
}

/// A syntax feature found in a source.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum EcmaFeature {
    /// `let` or `const` declarations.
    LetConst,
    /// Arrow functions.
    ArrowFunctions,
    /// Classes.
    Classes,
    /// Destructuring patterns.
    Destructuring,
    /// Default function parameters.
    DefaultParameters,
    /// Rest parameters or array/call spread.
    RestAndSpread,
    /// Object rest or spread.
    ObjectRestSpread,
    /// `for...of` loops.
    ForOf,
    /// Generator functions or `yield`.
    Generators,
    /// ECMAScript modules.
    Modules,
    /// Template literals.
    TemplateLiterals,
    /// Exponentiation (`**` or `**=`).
    Exponentiation,
    /// Async functions and async arrows.
    AsyncFunctions,
    /// `await` expressions.
    Await,
    /// Async generators.
    AsyncGenerators,
    /// `for await...of` loops.
    ForAwaitOf,
    /// Optional catch bindings (`catch {}`).
    OptionalCatchBinding,
    /// Optional chaining (`?.`).
    OptionalChaining,
    /// Nullish coalescing (`??`).
    NullishCoalescing,
    /// BigInt literals.
    BigInt,
    /// Logical assignment (`&&=`, `||=`, `??=`).
    LogicalAssignment,
    /// Public class fields.
    ClassFields,
    /// Private class fields.
    PrivateClassFields,
    /// Static initialization blocks.
    StaticBlocks,
    /// JSX, which is not ECMAScript syntax.
    Jsx,
    /// Decorators, which are not assigned an ECMAScript edition here.
    Decorators,
    /// Function bind syntax, which is not assigned an ECMAScript edition
    /// here.
    FunctionBind,
    /// Default export-from syntax, which is not assigned an ECMAScript
    /// edition here.
    ExportDefaultFrom,
    /// Import attributes, which are not assigned an ECMAScript edition here.
    ImportAttributes,
    /// Auto-accessors, which are not assigned an ECMAScript edition here.
    AutoAccessors,
    /// Explicit resource management syntax, which is not assigned an
    /// ECMAScript edition here.
    ExplicitResourceManagement,
}

impl EcmaFeature {
    /// Return the earliest ECMAScript edition containing this feature.
    /// `None` means that the feature is not standard ECMAScript syntax.
    #[must_use]
    pub const fn minimum_version(self) -> Option<EcmaVersion> {
        use EcmaVersion as Version;

        Some(match self {
            Self::LetConst
            | Self::ArrowFunctions
            | Self::Classes
            | Self::Destructuring
            | Self::DefaultParameters
            | Self::RestAndSpread
            | Self::ForOf
            | Self::Generators
            | Self::Modules
            | Self::TemplateLiterals => Version::Es2015,
            Self::Exponentiation => Version::Es2016,
            Self::AsyncFunctions | Self::Await => Version::Es2017,
            Self::ObjectRestSpread | Self::AsyncGenerators | Self::ForAwaitOf => Version::Es2018,
            Self::OptionalCatchBinding => Version::Es2019,
            Self::OptionalChaining | Self::NullishCoalescing | Self::BigInt => Version::Es2020,
            Self::LogicalAssignment => Version::Es2021,
            Self::ClassFields | Self::PrivateClassFields | Self::StaticBlocks => Version::Es2022,
            Self::Jsx
            | Self::Decorators
            | Self::FunctionBind
            | Self::ExportDefaultFrom
            | Self::ImportAttributes
            | Self::AutoAccessors
            | Self::ExplicitResourceManagement => {
                return None;
            }
        })
    }
}

/// Result of analyzing one source's ECMAScript syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EcmaVersionReport {
    minimum_version: Option<EcmaVersion>,
    features: Vec<EcmaFeature>,
}

impl EcmaVersionReport {
    /// Return the oldest standard ECMAScript edition compatible with the
    /// detected syntax. This is `None` when a detected feature is not
    /// standard ECMAScript, such as JSX or decorators.
    #[must_use]
    pub fn minimum_version(&self) -> Option<EcmaVersion> {
        self.minimum_version
    }

    /// Return all detected features in deterministic order.
    #[must_use]
    pub fn features(&self) -> &[EcmaFeature] {
        &self.features
    }

    pub(crate) fn from_program(program: &Program) -> Self {
        let mut detector = FeatureDetector::default();
        program.visit_with(&mut detector);
        detector.finish()
    }
}

/// Analyze the syntax of one source without requiring a rule catalog or
/// host environment.
pub fn analyze_ecma_version(source: &SourceFile) -> Result<EcmaVersionReport, ParseDiagnostic> {
    analyze_ecma_version_with_limits(source, &AnalysisLimits::default())
}

/// Analyze one source with an explicit syntax-depth analysis limit.
pub fn analyze_ecma_version_with_limits(
    source: &SourceFile,
    limits: &AnalysisLimits,
) -> Result<EcmaVersionReport, ParseDiagnostic> {
    let program = crate::parse::SourceParser::with_syntax_depth(source, limits.syntax_depth())?
        .parse_program_only()?;
    Ok(EcmaVersionReport::from_program(&program))
}

#[derive(Default)]
struct FeatureDetector {
    features: BTreeSet<EcmaFeature>,
    in_parameter_pattern: bool,
}

impl FeatureDetector {
    fn record(&mut self, feature: EcmaFeature) {
        self.features.insert(feature);
    }

    fn finish(self) -> EcmaVersionReport {
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

    fn visit_object_lit(&mut self, object: &swc_ecma_ast::ObjectLit) {
        object.visit_children_with(self);
    }

    fn visit_prop_or_spread(&mut self, property: &swc_ecma_ast::PropOrSpread) {
        if matches!(property, swc_ecma_ast::PropOrSpread::Spread(_)) {
            self.record(EcmaFeature::ObjectRestSpread);
        }
        property.visit_children_with(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(source: &str) -> EcmaVersionReport {
        let source = SourceFile::new("test.js", source).unwrap();
        analyze_ecma_version(&source).unwrap()
    }

    #[test]
    fn empty_source_is_es5() {
        let report = analyze("");
        assert_eq!(report.minimum_version(), Some(EcmaVersion::Es5));
        assert!(report.features().is_empty());
    }

    #[test]
    fn arrows_detect_nested_default_parameters() {
        let report = analyze(
            "const object = ({ value = 1 }) => value; \
             const array = ([value = 1]) => value; \
             const rest = ({ value = 1, ...other }) => value + other.value;",
        );
        assert!(report.features().contains(&EcmaFeature::DefaultParameters));
    }

    #[test]
    fn pattern_and_object_features_are_recorded_without_confusing_defaults() {
        let report = analyze(
            "const { value = 1, ...rest } = source; \
             const values = [...items]; \
             const copy = { ...source }; \
             const outer = () => { const factory = () => { const { nested = 1 } = value; }; return factory; };",
        );
        assert!(!report.features().contains(&EcmaFeature::DefaultParameters));
        assert!(report.features().contains(&EcmaFeature::Destructuring));
        assert!(report.features().contains(&EcmaFeature::RestAndSpread));
        assert!(report.features().contains(&EcmaFeature::ObjectRestSpread));
    }

    #[test]
    fn reports_the_highest_required_standard_version() {
        let report = analyze("const run = async () => await task();");
        assert_eq!(report.minimum_version(), Some(EcmaVersion::Es2017));
        assert_eq!(
            report.features(),
            &[
                EcmaFeature::LetConst,
                EcmaFeature::ArrowFunctions,
                EcmaFeature::AsyncFunctions,
                EcmaFeature::Await,
            ]
        );
    }

    #[test]
    fn reports_non_ecmascript_syntax_without_claiming_compatibility() {
        let report = analyze("const view = <Panel />;");
        assert_eq!(report.minimum_version(), None);
        assert_eq!(
            report.features(),
            &[EcmaFeature::LetConst, EcmaFeature::Jsx]
        );
    }

    #[test]
    fn reports_modules_and_newer_expression_features() {
        let report = analyze("export const value = source?.value ?? 1n;");
        assert_eq!(report.minimum_version(), Some(EcmaVersion::Es2020));
        assert!(report.features().contains(&EcmaFeature::Modules));
        assert!(report.features().contains(&EcmaFeature::OptionalChaining));
        assert!(report.features().contains(&EcmaFeature::NullishCoalescing));
        assert!(report.features().contains(&EcmaFeature::BigInt));
    }

    #[test]
    fn reports_import_attributes_on_static_and_dynamic_imports() {
        let report = analyze(
            "import value from 'mod' with { type: 'json' }; \
             export { value } from 'mod' with { type: 'json' }; \
             export * from 'mod' with { type: 'json' }; \
             import('mod', { with: { type: 'json' } });",
        );
        assert_eq!(report.minimum_version(), None);
        assert!(report.features().contains(&EcmaFeature::ImportAttributes));
    }

    #[test]
    fn reports_default_export_from_syntax() {
        let report = analyze("export value from 'mod';");
        assert_eq!(report.minimum_version(), None);
        assert!(report.features().contains(&EcmaFeature::ExportDefaultFrom));
    }

    #[test]
    fn reports_auto_accessors() {
        let report = analyze("class Example { accessor value; }");
        assert_eq!(report.minimum_version(), None);
        assert!(report.features().contains(&EcmaFeature::AutoAccessors));
    }

    #[test]
    fn explicit_limits_bound_standalone_analysis() {
        let source = SourceFile::new("deep.js", "(((value)))").unwrap();
        let limits = AnalysisLimits::default().with_syntax_depth(1).unwrap();
        let error = analyze_ecma_version_with_limits(&source, &limits).unwrap_err();
        assert_eq!(error.code().as_str(), "syntax_depth_exceeded");
    }
}
