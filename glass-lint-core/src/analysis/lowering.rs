//! Private parser-to-artifact lowering boundary.
//!
//! Parser and AST details stop here. Downstream project analysis receives an
//! immutable local artifact and its source map, never a parsed program.

use std::{collections::BTreeMap, sync::Arc};

use swc_common::Spanned;
use swc_ecma_ast::Program;
use swc_ecma_visit::VisitWith;

use super::{
    SemanticArtifact, facts, module, resolution,
    status::{AnalysisComponent, AnalysisStatus, IncompleteReason},
};
use crate::{
    ParseDiagnostic, SourceFile,
    analysis::{facts::SemanticFacts, flow::effect::FunctionEffects, status::StatusScope},
};

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
    start: u32,
    len: u32,
    text: Option<Arc<str>>,
}

impl SpanNormalizer {
    pub(in crate::analysis) fn new(source_start: swc_common::BytePos, source: &str) -> Self {
        Self {
            start: source_start.0,
            len: u32::try_from(source.len()).unwrap_or(u32::MAX),
            text: Some(Arc::from(source)),
        }
    }

    #[cfg(test)]
    pub(in crate::analysis) fn for_program(program: &Program) -> Self {
        let span = program.span();
        Self {
            start: span.lo.0,
            len: span.hi.0.saturating_sub(span.lo.0),
            text: None,
        }
    }

    pub(in crate::analysis) fn normalize(
        &self,
        span: swc_common::Span,
    ) -> Result<crate::ByteRange, InvalidParserSpan> {
        let offset = span
            .lo
            .0
            .checked_sub(self.start)
            .ok_or(InvalidParserSpan)?;
        let end = span
            .hi
            .0
            .checked_sub(self.start)
            .ok_or(InvalidParserSpan)?;
        if end > self.len
            || self.text.as_ref().is_some_and(|source| {
                let offset = offset as usize;
                let end = end as usize;
                offset > source.len()
                    || end > source.len()
                    || !source.is_char_boundary(offset)
                    || !source.is_char_boundary(end)
            })
        {
            return Err(InvalidParserSpan);
        }
        crate::ByteRange::new(offset, end).map_err(|_| InvalidParserSpan)
    }
}

pub struct LoweredSource {
    pub(crate) source: super::local::LocatedSourceContext,
    pub(crate) semantic: Arc<SemanticArtifact>,
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
        source: super::local::LocatedSourceContext::new(source),
        semantic: Arc::new(semantic),
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
    lower_program_with_name_limit(
        program,
        environment,
        limits,
        coordinates,
        crate::analysis::name::MAX_NAMES,
    )
}

fn lower_program_with_name_limit(
    program: &Program,
    environment: &crate::Environment,
    limits: &crate::AnalysisLimits,
    coordinates: &SpanNormalizer,
    name_limit: usize,
) -> SemanticArtifact {
    let resolver = resolution::Resolver::collect_with_name_limit(
        program,
        environment,
        coordinates.clone(),
        name_limit,
    );
    let mut builder = facts::build::FactBuilder::with_limit(&resolver, limits.semantic_operations);
    VisitWith::visit_with(program, &mut builder);

    let (mut stream, interface) = builder.into_parts();
    let mut status = AnalysisStatus::default();
    let name_exhausted = resolver.name_table_exhausted();
    // Scope collection, resolution, value interning, and path projection are
    // all local semantic work. Their bounded failures are intentionally
    // aggregated under the Facts component so retained-prefix policy has one
    // typed status boundary rather than several sentinel booleans.
    if stream.budget_exhausted()
        || stream.path_exhausted()
        || (resolver.value_arena_exhausted() && !name_exhausted)
        || (!stream.is_structurally_valid() && !stream.name_exhausted())
    {
        status.record(
            StatusScope::Project,
            IncompleteReason::BudgetExhausted {
                component: AnalysisComponent::Facts,
                limit: limits.semantic_operations,
                observed: Some(stream.facts().len()),
            },
        );
    }
    if stream.invalid_parser_span() {
        status.record(StatusScope::Project, IncompleteReason::InvalidParserSpan);
    }
    if let Some(exhaustion) = resolver.name_exhaustion() {
        status.record(
            StatusScope::Project,
            IncompleteReason::NameExhausted {
                limit: exhaustion.limit,
                attempted: exhaustion.attempted,
            },
        );
    }
    let export_origins = interface
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

    if resolver.name_table_exhausted() {
        stream.mark_name_exhausted();
    }

    // TODO: if we design with the borrow checker in mind, is there not a simpler
    // way to do this?
    let names = resolver
        .into_name_table()
        .expect("Failed to aquire exclusive NameTable");
    stream
        .freeze_names(Arc::new(names))
        .expect("Stream already owns a NameTable");

    let facts = SemanticFacts::from_lowering(stream, interface, environment);
    let effects = FunctionEffects::collect(facts.stream(), limits.effect_operations);
    if effects.budget_exhausted() {
        status.record(
            StatusScope::Project,
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
    fn name_exhaustion_invalidates_indexes_and_effects_with_an_accurate_status() {
        let source = "function helper(options) { return options.send; } helper({ send: 1 });";
        let parsed = crate::parse(source, "name-exhaustion.js").expect("source should parse");
        let coordinates = SpanNormalizer::new(parsed.source_start, source);
        let artifact = lower_program_with_name_limit(
            &parsed.program,
            &crate::Environment::default(),
            &crate::AnalysisLimits::default(),
            &coordinates,
            2,
        );

        assert!(!artifact.facts().stream().is_valid());
        assert!(artifact.facts().shared_matcher_index().is_empty());
        assert!(artifact.effects().iter_effects().next().is_none());
        let (_, project_diagnostics) = artifact.status().diagnostics();
        assert_eq!(project_diagnostics.len(), 1);
        assert_eq!(
            project_diagnostics[0].code.as_str(),
            "semantic_name_budget_exhausted"
        );
        assert!(project_diagnostics[0].message.contains("limit=2"));
        assert!(project_diagnostics[0].message.contains("attempted=3"));

        let repeated = lower_program_with_name_limit(
            &parsed.program,
            &crate::Environment::default(),
            &crate::AnalysisLimits::default(),
            &coordinates,
            2,
        );
        assert_eq!(
            format!("{:?}", artifact.facts().stream().facts()),
            format!("{:?}", repeated.facts().stream().facts())
        );
        assert_eq!(artifact.status(), repeated.status());
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
