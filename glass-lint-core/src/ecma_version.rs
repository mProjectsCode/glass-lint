//! Minimum ECMAScript syntax-version analysis.
//!
//! This is deliberately a syntax analysis. It does not infer host APIs or
//! runtime built-ins, and SWC AST types remain private to the core crate.

use std::fmt;

use swc_ecma_ast::Program;
use swc_ecma_visit::VisitWith;

mod detector;
use detector::FeatureDetector;

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

#[cfg(test)]
mod tests;
