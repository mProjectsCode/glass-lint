//! Private parser-to-artifact lowering boundary.
//!
//! Parser and AST details stop here. Downstream project analysis receives an
//! immutable local artifact and its source map, never a parsed program.

use std::{collections::BTreeMap, sync::Arc};

use swc_common::Spanned;
use swc_ecma_ast::Program;
use swc_ecma_visit::VisitWith;

use crate::{
    ParseDiagnostic, SourceFile,
    analysis::{
        LocatedSourceContext, SemanticArtifact,
        facts::{self, SemanticFacts},
        flow::effect::FunctionEffects,
        module,
        name::NameTable,
        resolution,
        scope::ScopeGraph,
        status::{AnalysisComponent, AnalysisStatus, IncompleteReason, StatusScope},
    },
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
/// Converts SWC `BytePos` spans to zero-based `ByteRange` values relative to
/// the authored source text. Validation ensures the result is within bounds
/// and on UTF-8 character boundaries.
pub(in crate::analysis) struct SpanNormalizer {
    /// SWC `BytePos` value assigned to authored byte offset zero.
    start: u32,
    /// Length of the authored source in bytes.
    len: u32,
    /// Retained source text for boundary validation, when available.
    text: Option<crate::SourceText>,
    /// True when the source text is entirely ASCII; boundary checks are then
    /// redundant since every byte position is a valid UTF-8 boundary.
    is_ascii: bool,
}

impl SpanNormalizer {
    pub(in crate::analysis) fn new(
        source_start: swc_common::BytePos,
        source: impl Into<crate::SourceText>,
    ) -> Self {
        let source = source.into();
        let is_ascii = source.is_ascii();
        Self {
            start: source_start.0,
            len: u32::try_from(source.len()).unwrap_or(u32::MAX),
            text: Some(source),
            is_ascii,
        }
    }

    #[cfg(test)]
    pub(in crate::analysis) fn for_program(program: &Program) -> Self {
        let span = program.span();
        Self {
            start: span.lo.0,
            len: span.hi.0.saturating_sub(span.lo.0),
            text: None,
            is_ascii: false,
        }
    }

    pub(in crate::analysis) fn normalize(
        &self,
        span: swc_common::Span,
    ) -> Result<crate::ByteRange, InvalidParserSpan> {
        let offset = span.lo.0.checked_sub(self.start).ok_or(InvalidParserSpan)?;
        let end = span.hi.0.checked_sub(self.start).ok_or(InvalidParserSpan)?;
        if end > self.len {
            return Err(InvalidParserSpan);
        }
        if !self.is_ascii
            && self.text.as_ref().is_some_and(|source| {
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
    pub(crate) source: LocatedSourceContext,
    pub(crate) semantic: Arc<SemanticArtifact>,
}

/// Lower one source file into an immutable semantic artifact. Parsing,
/// scope collection, fact construction, and effect extraction all happen in
/// one pass. The result is ready for project linking and matcher projection.
pub fn lower_source(
    linter: &crate::Linter,
    source: &SourceFile,
) -> Result<LoweredSource, ParseDiagnostic> {
    let parsed = crate::parse::parse_with_language_and_depth(
        &source.source,
        &source.path,
        source.language,
        linter.analysis_limits().syntax_depth(),
    )?;
    let coordinates = SpanNormalizer::new(parsed.source_start, source.source.clone());
    let semantic = lower_program(
        &parsed.program,
        linter.analysis_environment(),
        linter.analysis_limits(),
        &coordinates,
    );
    Ok(LoweredSource {
        source: LocatedSourceContext::new(source),
        semantic: Arc::new(semantic),
    })
}

/// Lower an already-parsed SWC program into a `SemanticArtifact`. Used
/// by both the main lowering path and by tests that need fine-grained
/// control over limits or name budgets.
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

fn check_facts_budget(
    stream: &facts::FactStream,
    resolver: &resolution::Resolver,
    limits: &crate::AnalysisLimits,
) -> Option<IncompleteReason> {
    let name_exhausted = resolver.name_table_exhausted();
    if stream.budget_exhausted()
        || stream.path_exhausted()
        || (resolver.value_arena_exhausted() && !name_exhausted)
        || (!stream.is_structurally_valid() && !stream.name_exhausted())
    {
        Some(IncompleteReason::BudgetExhausted {
            component: AnalysisComponent::Facts,
            limit: limits.semantic_operations(),
            observed: Some(stream.facts().len()),
        })
    } else {
        None
    }
}

fn check_invalid_parser_span(stream: &facts::FactStream) -> Option<IncompleteReason> {
    stream
        .invalid_parser_span()
        .then_some(IncompleteReason::InvalidParserSpan)
}

fn check_name_exhaustion(resolver: &resolution::Resolver) -> Option<IncompleteReason> {
    resolver
        .name_exhaustion()
        .map(|exhaustion| IncompleteReason::NameExhausted {
            limit: exhaustion.limit,
            attempted: exhaustion.attempted,
        })
}

fn check_effects_budget(
    effects: &FunctionEffects,
    limits: &crate::AnalysisLimits,
) -> Option<IncompleteReason> {
    effects
        .budget_exhausted()
        .then_some(IncompleteReason::BudgetExhausted {
            component: AnalysisComponent::Effects,
            limit: limits.effect_operations(),
            observed: Some(effects.operation_count()),
        })
}

fn lower_program_with_name_limit(
    program: &Program,
    environment: &crate::Environment,
    limits: &crate::AnalysisLimits,
    coordinates: &SpanNormalizer,
    name_limit: usize,
) -> SemanticArtifact {
    LocalLowering {
        environment,
        limits,
        coordinates,
        name_limit,
    }
    .run(program)
}

/// Consuming coordinator for the private local-analysis phases.  Keeping the
/// transition in one owner makes it difficult for callers to observe or reuse
/// an intermediate scope, resolution, or fact state.
struct LocalLowering<'a> {
    environment: &'a crate::Environment,
    limits: &'a crate::AnalysisLimits,
    coordinates: &'a SpanNormalizer,
    name_limit: usize,
}

impl LocalLowering<'_> {
    fn run(self, program: &Program) -> SemanticArtifact {
        let Self {
            environment,
            limits,
            coordinates,
            name_limit,
        } = self;
        let names = NameTable::with_max_entries(name_limit);
        let scoped_program = ScopeGraph::collect_scoped_program(program, environment, names);
        let (scope_graph, issues) = scoped_program.into_parts();
        let mut resolver = resolution::Resolver::new(scope_graph, coordinates.clone());

        let mut builder =
            facts::build::FactBuilder::with_limit(&mut resolver, limits.semantic_operations());
        VisitWith::visit_with(program, &mut builder);

        let built = builder.into_built_facts();
        let mut stream = built.stream;
        let interface = built.interface;
        let mut status = AnalysisStatus::default();

        if !issues.is_empty() {
            status.record(
                StatusScope::Project,
                IncompleteReason::ScopeShapeMismatch {
                    detail: format!("{} scope collection issue(s)", issues.len()),
                },
            );
        }

        if let Some(reason) = check_facts_budget(&stream, &resolver, limits) {
            status.record(StatusScope::Project, reason);
        }
        if let Some(reason) = check_invalid_parser_span(&stream) {
            status.record(StatusScope::Project, reason);
        }
        if let Some(reason) = check_name_exhaustion(&resolver) {
            status.record(StatusScope::Project, reason);
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

        let (names, values) = resolver.into_parts();
        stream
            .freeze_names(names)
            .expect("Stream already owns a NameTable");

        let facts = SemanticFacts::from_lowering(stream, interface, environment, values);
        let effects = FunctionEffects::collect(facts.stream(), limits.effect_operations());
        if let Some(reason) = check_effects_budget(&effects, limits) {
            status.record(StatusScope::Project, reason);
        }

        SemanticArtifact::from_lowering(facts, export_origins, effects, status)
    }
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
        assert!(artifact.facts().matcher_index().is_empty());
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
