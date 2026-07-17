//! Private parser-to-artifact lowering boundary.
//!
//! Parser and AST details stop here. Downstream project analysis receives an
//! immutable local artifact and its source map, never a parsed program.

use std::collections::BTreeMap;

use swc_common::Spanned;
use swc_ecma_ast::Program;

use super::{
    SemanticArtifact, facts, flow, module, resolution,
    status::{AnalysisComponent, AnalysisStatus, IncompleteReason},
};
use crate::{ParseDiagnostic, SourceFile};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis) struct InvalidParserSpan;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) struct ParserSpanKey {
    lo: u32,
    hi: u32,
}

impl From<swc_common::Span> for ParserSpanKey {
    fn from(span: swc_common::Span) -> Self {
        Self {
            lo: span.lo.0,
            hi: span.hi.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct SpanNormalizer {
    source_start: u32,
    source_len: u32,
    boundaries: Option<Vec<bool>>,
}

impl SpanNormalizer {
    pub(in crate::analysis) fn new(source_start: swc_common::BytePos, source: &str) -> Self {
        Self {
            source_start: source_start.0,
            source_len: u32::try_from(source.len()).unwrap_or(u32::MAX),
            boundaries: Some(
                (0..=source.len())
                    .map(|offset| source.is_char_boundary(offset))
                    .collect(),
            ),
        }
    }

    #[cfg(test)]
    pub(in crate::analysis) fn for_program(program: &Program) -> Self {
        let span = program.span();
        Self {
            source_start: span.lo.0,
            source_len: span.hi.0.saturating_sub(span.lo.0),
            boundaries: None,
        }
    }

    pub(in crate::analysis) fn normalize(
        &self,
        span: swc_common::Span,
    ) -> Result<crate::ByteRange, InvalidParserSpan> {
        let start = span
            .lo
            .0
            .checked_sub(self.source_start)
            .ok_or(InvalidParserSpan)?;
        let end = span
            .hi
            .0
            .checked_sub(self.source_start)
            .ok_or(InvalidParserSpan)?;
        if end > self.source_len
            || self.boundaries.as_ref().is_some_and(|boundaries| {
                !boundaries.get(start as usize).copied().unwrap_or(false)
                    || !boundaries.get(end as usize).copied().unwrap_or(false)
            })
        {
            return Err(InvalidParserSpan);
        }
        crate::ByteRange::new(start, end).map_err(|_| InvalidParserSpan)
    }
}

pub struct LoweredSource {
    pub(crate) source: super::local::SourceContext,
    pub(crate) semantic: SemanticArtifact,
}

pub fn lower_source(
    linter: &crate::Linter,
    source: &SourceFile,
) -> Result<LoweredSource, ParseDiagnostic> {
    let parsed = crate::parse::parse_with_language_and_depth(
        &source.source,
        &source.path,
        source.language,
        linter.analysis_limits().syntax_depth,
    )?;
    let coordinates = SpanNormalizer::new(parsed.source_start, &source.source);
    let semantic = lower_program(
        &parsed.program,
        linter.analysis_environment(),
        linter.analysis_limits(),
        &coordinates,
    );
    Ok(LoweredSource {
        source: super::local::SourceContext::new(source),
        semantic,
    })
}

pub fn lower_artifact(
    linter: &crate::Linter,
    source: &SourceFile,
) -> Result<SemanticArtifact, ParseDiagnostic> {
    let parsed = crate::parse::parse_with_language_and_depth(
        &source.source,
        &source.path,
        source.language,
        linter.analysis_limits().syntax_depth,
    )?;
    let coordinates = SpanNormalizer::new(parsed.source_start, &source.source);
    Ok(lower_program(
        &parsed.program,
        linter.analysis_environment(),
        linter.analysis_limits(),
        &coordinates,
    ))
}

pub fn lower_program(
    program: &Program,
    environment: &crate::Environment,
    limits: &crate::AnalysisLimits,
    coordinates: &SpanNormalizer,
) -> SemanticArtifact {
    let resolver =
        resolution::Resolver::collect_with_environment(program, environment, coordinates.clone());
    let mut builder = facts::build::FactBuilder::with_limit(&resolver, limits.semantic_operations);
    swc_ecma_visit::VisitWith::visit_with(program, &mut builder);
    let (stream, interface) = builder.into_parts();
    let facts = facts::SemanticFacts::from_lowering(stream, interface);
    let mut status = AnalysisStatus::default();
    // Scope collection, resolution, value interning, and path projection are
    // all local semantic work. Their bounded failures are intentionally
    // aggregated under the Facts component so retained-prefix policy has one
    // typed status boundary rather than several sentinel booleans.
    if facts.stream().budget_exhausted()
        || facts.stream().path_exhausted()
        || resolver.value_arena_exhausted()
        || !facts.is_valid()
    {
        status.record(
            crate::analysis::status::StatusScope::Project,
            IncompleteReason::BudgetExhausted {
                component: AnalysisComponent::Facts,
                limit: limits.semantic_operations,
                observed: Some(facts.stream().facts().len()),
            },
        );
    }
    if facts.stream().invalid_parser_span() {
        status.record(
            crate::analysis::status::StatusScope::Project,
            IncompleteReason::InvalidParserSpan,
        );
    }
    let export_origins = facts
        .interface()
        .exports()
        .filter_map(|(_, export)| match export {
            module::ModuleExport::Local { name } => Some((
                name.clone(),
                resolver.exported_provenance(name, program.span()),
            )),
            module::ModuleExport::Value
            | module::ModuleExport::ReExport { .. }
            | module::ModuleExport::Namespace { .. }
            | module::ModuleExport::Unknown => None,
        })
        .collect::<BTreeMap<_, _>>();
    let effects = flow::effect::FunctionEffects::collect(facts.stream(), limits.effect_operations);
    if effects.budget_exhausted() {
        status.record(
            crate::analysis::status::StatusScope::Project,
            IncompleteReason::BudgetExhausted {
                component: AnalysisComponent::Effects,
                limit: limits.effect_operations,
                observed: Some(effects.operation_count()),
            },
        );
    }
    SemanticArtifact::from_lowering(facts, export_origins, effects, status)
}

#[cfg(test)]
mod tests {
    use swc_common::{BytePos, Span};

    use super::*;

    #[test]
    fn swc_span_is_normalized_to_zero_based_byte_range_once() {
        let normalizer = SpanNormalizer::new(BytePos(40), "aé\r\n");
        assert_eq!(
            normalizer.normalize(Span::new(BytePos(40), BytePos(43))),
            Ok(crate::ByteRange::new(0, 3).unwrap())
        );
        assert!(
            normalizer
                .normalize(Span::new(BytePos(42), BytePos(43)))
                .is_err()
        );
        assert!(
            normalizer
                .normalize(Span::new(BytePos(40), BytePos(46)))
                .is_err()
        );
    }

    #[test]
    fn invalid_parser_span_records_incomplete_without_fake_location() {
        let source = "fetch('/remote');";
        let parsed = crate::parse::parse(source, "main.js").unwrap();
        let invalid = SpanNormalizer::new(BytePos(parsed.source_start.0 + 100), source);
        let artifact = lower_program(
            &parsed.program,
            &crate::Environment::default(),
            &crate::AnalysisLimits::default(),
            &invalid,
        );
        assert!(!artifact.status().is_complete());
        assert!(artifact.facts().stream().facts().is_empty());
        let (files, project) = artifact.status().diagnostics();
        assert!(files.is_empty());
        assert_eq!(project.len(), 1);
        assert_eq!(project[0].code.as_str(), "invalid_parser_span");
        assert!(project[0].location.is_none());
    }
}
