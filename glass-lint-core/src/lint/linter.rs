use std::collections::{BTreeMap, BTreeSet};

use super::catalog::RuleCatalog;
use crate::{
    CoreConfig, Environment, ProjectInput, ProjectInputError, ProjectReport, ProjectSession,
    REPORT_VERSION, RuleCatalogError, RuleId, SourceLanguage,
    analysis::{LocalModuleModel, ProjectSemanticModel},
    api::classification::ApiClassificationResult,
    diagnostic::LintReport,
    project::ModuleId,
};

type AnalyzedModules = BTreeMap<
    String,
    (
        swc_common::sync::Lrc<swc_common::SourceMap>,
        LocalModuleModel,
    ),
>;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Configuration failure when selecting rules for a linter.
pub enum LintConfigError {
    /// A requested fully-qualified rule ID is absent from the catalog.
    UnknownRule(RuleId),
}

impl std::fmt::Display for LintConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRule(id) => write!(f, "unknown rule `{id}`"),
        }
    }
}

impl std::error::Error for LintConfigError {}

/// Immutable catalog plus sorted enabled-rule indexes for lint execution.
pub struct Linter {
    /// Validated rule catalog and compiled matcher plans.
    catalog: RuleCatalog,
    /// Enabled rule indexes in deterministic order.
    enabled: Vec<usize>,
}

impl Linter {
    /// Starts a deterministic project collection session.
    pub fn begin_project(
        &self,
        root: impl Into<std::path::PathBuf>,
    ) -> Result<ProjectSession<'_>, ProjectInputError> {
        ProjectSession::new(self, root)
    }

    /// Apply provider-neutral engine configuration to this linter.
    pub fn configured(self, config: &CoreConfig) -> Result<Self, LintConfigError> {
        match &config.rules {
            Some(rules) => Self::with_rules(self.catalog, rules.clone()),
            None => Ok(self),
        }
    }

    #[must_use]
    /// Construct a linter with every catalog rule enabled.
    pub fn new(catalog: RuleCatalog) -> Self {
        let enabled = (0..catalog.rules.len()).collect();
        Self { catalog, enabled }
    }

    /// Select all rules at or above the requested confidence level.
    /// Construct a linter with rules at or above a confidence threshold.
    pub fn with_confidence(catalog: RuleCatalog, confidence: crate::api::rule::Confidence) -> Self {
        let enabled = catalog
            .rules
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| {
                (rule.confidence() as u8 <= confidence as u8).then_some(index)
            })
            .collect();
        Self { catalog, enabled }
    }

    /// Construct a linter with a validated explicit rule selection.
    pub fn with_rules(
        catalog: RuleCatalog,
        enabled: impl IntoIterator<Item = RuleId>,
    ) -> Result<Self, LintConfigError> {
        let mut indices = enabled
            .into_iter()
            .map(|id| {
                catalog
                    .rule_index(&id)
                    .ok_or(LintConfigError::UnknownRule(id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        indices.sort_unstable();
        indices.dedup();
        Ok(Self {
            catalog,
            enabled: indices,
        })
    }

    /// Combine provider linters into one analysis pass under a shared host
    /// environment while preserving each linter's enabled rule selection.
    pub fn combine_with_environment(
        linters: impl IntoIterator<Item = Self>,
        environment: Environment,
    ) -> Result<Self, RuleCatalogError> {
        let mut catalogs = Vec::new();
        let mut enabled = BTreeSet::new();
        for linter in linters {
            enabled.extend(linter.enabled_rule_ids());
            catalogs.push(linter.catalog);
        }
        let catalog = RuleCatalog::combine_with_environment(catalogs, environment)?;
        Ok(Self::with_rules(catalog, enabled)
            .expect("combined catalog retains every selected rule"))
    }

    #[must_use]
    /// Borrow the validated catalog.
    pub fn catalog(&self) -> &RuleCatalog {
        &self.catalog
    }

    /// Returns the enabled rule IDs in deterministic catalog order.
    #[must_use]
    pub fn enabled_rule_ids(&self) -> Vec<RuleId> {
        self.enabled
            .iter()
            .filter_map(|&index| self.catalog.rule_id(index).cloned())
            .collect()
    }

    /// Borrow the environment used by semantic analysis.
    pub fn analysis_environment(&self) -> &Environment {
        self.catalog.environment()
    }

    /// Lints one JavaScript/JSX or TypeScript source file.
    ///
    /// Parsing stops after the first parser diagnostic.  Findings contain
    /// source ranges in one-based Unicode display columns. Evidence is bounded
    /// and each finding carries only the located occurrences enclosed by its
    /// primary range.
    #[must_use]
    pub fn lint(&self, source: &str, filename: &str) -> LintReport {
        let _span = tracing::info_span!(target: "glass_lint::lint", "lint", filename, source_bytes = source.len(), selected_rules = self.enabled.len()).entered();
        tracing::debug!(target: "glass_lint::parse", "parsing source");

        let language = SourceLanguage::from_filename(filename);
        let parsed = match crate::parse::parse_with_language(source, filename, language) {
            Ok(parsed) => parsed,
            Err(error) => {
                tracing::debug!(target: "glass_lint::parse", diagnostics = 1, "parse failed");

                return LintReport {
                    schema_version: REPORT_VERSION,
                    tool_version: env!("CARGO_PKG_VERSION").into(),
                    findings: Vec::new(),
                    parse_diagnostics: vec![error],
                };
            }
        };

        let classifications = {
            let _span = tracing::debug_span!(target: "glass_lint::semantic", "semantic").entered();

            let local = LocalModuleModel::analyze(&parsed.program, self.catalog.environment());

            let project = ProjectSemanticModel::single(filename, parsed.source_map.clone(), local);

            project.classify(self.catalog.compiled(), &self.catalog.rules, &self.enabled)
        };

        let mut findings = {
            let _span = tracing::debug_span!(target: "glass_lint::matching", "matching").entered();
            classifications
                .get(&ModuleId(0))
                .map_or_else(Vec::new, |classification| {
                    self.findings_for(classification, &parsed.source_map, source)
                })
        };

        findings.sort_by(|left, right| {
            (
                &left.range.start.line,
                &left.range.start.column,
                &left.rule_id,
            )
                .cmp(&(
                    &right.range.start.line,
                    &right.range.start.column,
                    &right.rule_id,
                ))
        });

        tracing::debug!(target: "glass_lint::lint", findings = findings.len(), "report assembled");
        tracing::info!(target: "glass_lint::lint", findings = findings.len(), parse_diagnostics = 0, "lint complete");

        LintReport {
            schema_version: REPORT_VERSION,
            tool_version: env!("CARGO_PKG_VERSION").into(),
            findings,
            parse_diagnostics: Vec::new(),
        }
    }

    /// Lints an in-memory project using explicit, already-classified
    /// resolution results.  Filesystem loading belongs to the project crate.
    pub fn lint_project(&self, input: ProjectInput) -> Result<ProjectReport, ProjectInputError> {
        let input = input.validate()?;
        let mut session = self.begin_project(input.root)?;
        for source in input.sources {
            session.add_source(source)?;
        }
        for (key, result) in input.resolutions {
            session.record_resolution(key, result)?;
        }
        session.finish()
    }

    /// Link an already analyzed project and return phase timings.
    pub(crate) fn lint_analyzed_project_timed(
        &self,
        input: ProjectInput,
        analyzed: AnalyzedModules,
        parse_diagnostics: BTreeMap<String, crate::ParseDiagnostic>,
    ) -> Result<(ProjectReport, std::time::Duration, std::time::Duration), ProjectInputError> {
        let input = input.validate()?;
        let mut authored = std::collections::BTreeSet::new();
        for (path, (source_map, local)) in &analyzed {
            authored.extend(
                local
                    .interface()
                    .authored_requests(path, source_map)
                    .into_iter()
                    .map(|request| request.key),
            );
        }
        for (key, _) in &input.resolutions {
            if !authored.contains(key) {
                return Err(ProjectInputError::UnknownRequest(key.clone()));
            }
        }
        let sources = input
            .sources
            .iter()
            .map(|source| (source.path.to_string(), source.source.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut files = parse_diagnostics
            .into_iter()
            .map(|(path, diagnostic)| {
                (
                    path.clone(),
                    crate::ProjectFileReport {
                        path: path.into(),
                        findings: Vec::new(),
                        parse_diagnostics: vec![diagnostic],
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();

        let linking_start = std::time::Instant::now();
        let project = ProjectSemanticModel::link(input, analyzed)?;
        let linking_elapsed = linking_start.elapsed();
        let matching_start = std::time::Instant::now();
        let classifications =
            project.classify(self.catalog.compiled(), &self.catalog.rules, &self.enabled);
        let matching_elapsed = matching_start.elapsed();
        self.populate_project_files(&project, &classifications, &sources, &mut files);

        let mut diagnostics = project.diagnostics().to_vec();
        if project.flow_budget_exhausted() {
            diagnostics.push(crate::ProjectDiagnostic {
                code: "flow_link_budget_exhausted".into(),
                message: "qualified function-effect projection exceeded its bounded budget".into(),
                location: None,
            });
            diagnostics.sort_by(|left, right| left.code.cmp(&right.code));
        }

        let evidence = files
            .values()
            .map(|file| {
                file.findings
                    .iter()
                    .map(|finding| finding.evidence.len())
                    .sum::<usize>()
            })
            .sum();

        let report = ProjectReport {
            schema_version: REPORT_VERSION,
            tool_version: env!("CARGO_PKG_VERSION").into(),
            files: files.into_values().collect(),
            diagnostics,
            operations: project.operation_counts(evidence),
        };

        Ok((report, linking_elapsed, matching_elapsed))
    }

    fn populate_project_files(
        &self,
        project: &ProjectSemanticModel,
        classifications: &BTreeMap<ModuleId, ApiClassificationResult>,
        sources: &BTreeMap<String, String>,
        files: &mut BTreeMap<String, crate::ProjectFileReport>,
    ) {
        for module in project.modules() {
            let Some(classification) = classifications.get(&module.id()) else {
                continue;
            };
            let Some(source) = sources.get(module.path()) else {
                continue;
            };
            let mut findings =
                self.project_findings_for_module(project, module, classification, source);
            findings.sort_by_key(|finding| {
                (
                    finding.location.range.start.line,
                    finding.location.range.start.column,
                    finding.rule_id.clone(),
                )
            });
            findings.dedup();
            files.insert(
                module.path().to_owned(),
                crate::ProjectFileReport {
                    path: module.path().to_owned().into(),
                    findings,
                    parse_diagnostics: Vec::new(),
                },
            );
        }
    }

    fn project_findings_for_module(
        &self,
        project: &ProjectSemanticModel,
        module: &crate::analysis::ProjectModule,
        classification: &ApiClassificationResult,
        source: &str,
    ) -> Vec<crate::ProjectFinding> {
        self.findings_for(classification, module.source_map(), source)
            .into_iter()
            .map(|finding| {
                let mut project_finding =
                    crate::ProjectFinding::from_finding(finding, module.path());
                let finding_rule_id = project_finding.rule_id.clone();
                let related = classification
                    .capabilities()
                    .iter()
                    .filter(|capability| {
                        self.catalog
                            .rule_id(capability.rule_index)
                            .is_some_and(|id| id == &finding_rule_id)
                    })
                    .flat_map(crate::api::classification::ApiCapability::evidence)
                    .flat_map(|evidence| &evidence.related)
                    .filter_map(|related| {
                        project
                            .fact_location(ModuleId(related.module), related.event)
                            .map(|mut location| {
                                location.message.clone_from(&related.symbol);
                                location
                            })
                    });
                project_finding.append_related(related);
                project_finding
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Position, SourceRange,
        api::rule::{Confidence, Matcher, Rule, Severity},
        lint::{findings::contains_range, ranges::remove_contained_ranges},
    };
    fn catalog() -> RuleCatalog {
        let rule = Rule::builder("network.fetch")
            .label("Uses fetch")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_global("fetch").unwrap();
        RuleCatalog::with_environment("test", vec![rule], environment).unwrap()
    }

    #[test]
    fn emits_one_located_finding_per_match() {
        let report = Linter::new(catalog()).lint("fetch('/a');\nfetch('/b');", "input.js");
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.findings[0].range.start.line, 1);
        assert_eq!(report.findings[1].range.start.line, 2);
        assert_eq!(report.findings[0].evidence.len(), 1);
        assert_eq!(report.findings[1].evidence.len(), 1);
        assert_eq!(report.findings[0].evidence[0].message, "call of \"fetch\"");
        assert_eq!(
            report.findings[0].evidence[0].range.as_ref(),
            Some(&report.findings[0].range)
        );
        assert_eq!(
            report.findings[1].evidence[0].range.as_ref(),
            Some(&report.findings[1].range)
        );
    }

    #[test]
    fn findings_only_carry_evidence_for_their_own_location() {
        let rule = Rule::builder("vault.write")
            .label("Writes vault files")
            .category("vault")
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .matcher(Matcher::rooted_member_call("app.vault.create"))
            .matcher(Matcher::rooted_member_call("app.vault.createFolder"))
            .build()
            .unwrap();
        let report = Linter::new(RuleCatalog::new("test", vec![rule]).unwrap()).lint(
            "this.app.vault.create('a');\nthis.app.vault.createFolder('b');",
            "input.js",
        );

        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.findings[0].evidence.len(), 1);
        assert_eq!(
            report.findings[0].evidence[0].message,
            "member_call of \"app.vault.create\""
        );
        assert_eq!(report.findings[1].evidence.len(), 1);
        assert_eq!(
            report.findings[1].evidence[0].message,
            "member_call of \"app.vault.createFolder\""
        );
    }

    #[test]
    fn rejects_shadowed_global_lookalikes() {
        let report = Linter::new(catalog()).lint(
            "function demo(fetch) { fetch('/local'); } fetch('/global');",
            "input.js",
        );
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn collapses_contained_ranges_for_same_rule() {
        let rule = Rule::builder("metadata.read")
            .label("Reads metadata")
            .category("metadata")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::rooted_member_read("app.metadataCache"))
            .matcher(Matcher::rooted_member_call(
                "app.metadataCache.getFileCache",
            ))
            .build()
            .unwrap();
        let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
        let report =
            Linter::new(catalog).lint("this.app.metadataCache.getFileCache(file);", "input.js");

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].range.start.column, 1);
        assert_eq!(report.findings[0].range.end.column, 36);
        assert_eq!(report.findings[0].evidence.len(), 2);
        assert!(report.findings[0].evidence.iter().all(|evidence| {
            evidence
                .range
                .as_ref()
                .is_some_and(|range| contains_range(&report.findings[0].range, range))
        }));
    }

    #[test]
    fn range_sweep_removes_large_nested_and_duplicate_sets() {
        let mut ranges = (0..5_000)
            .map(|column| SourceRange {
                start: Position { line: 1, column },
                end: Position {
                    line: 2,
                    column: 5_000 - column,
                },
            })
            .collect::<Vec<_>>();
        ranges.push(ranges[0].clone());

        remove_contained_ranges(&mut ranges);

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start.column, 0);
    }

    #[test]
    fn validates_custom_rule_selection() {
        let unknown = RuleId::parse("test:missing").unwrap();
        assert!(matches!(
            Linter::with_rules(catalog(), [unknown]),
            Err(LintConfigError::UnknownRule(_))
        ));
    }

    #[test]
    fn reports_structured_diagnostic_for_oversized_source() {
        let report =
            Linter::new(catalog()).lint(&"x".repeat(crate::MAX_SOURCE_BYTES + 1), "large.js");
        assert!(report.findings.is_empty());
        assert_eq!(report.parse_diagnostics.len(), 1);
        assert_eq!(report.parse_diagnostics[0].code, "source_too_large".into());
        assert_eq!(report.parse_diagnostics[0].filename, "large.js");
        assert!(report.parse_diagnostics[0].range.is_none());
    }

    #[test]
    fn parse_diagnostics_carry_stable_location_context() {
        let report = Linter::new(catalog()).lint("fetch(", "broken.js");
        assert!(report.findings.is_empty());
        let diagnostic = &report.parse_diagnostics[0];
        assert_eq!(diagnostic.code, "syntax_error".into());
        assert_eq!(diagnostic.filename, "broken.js");
        assert!(diagnostic.message.starts_with("JavaScript parse error:"));
        assert!(diagnostic.range.is_some());
    }

    #[test]
    fn source_locations_handle_crlf_and_eof_without_byte_columns() {
        let report = Linter::new(catalog()).lint("fetch('/a');\r\nfetch('/é');", "crlf.js");
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.findings[0].range.start.line, 1);
        assert_eq!(report.findings[1].range.start.line, 2);
        assert!(report.findings[1].range.end.column > report.findings[1].range.start.column);

        let empty = Linter::new(catalog()).lint("", "empty.js");
        assert!(empty.findings.is_empty());
        assert!(empty.parse_diagnostics.is_empty());
    }

    #[test]
    fn evidence_ranges_and_snippets_are_populated_for_unicode_source() {
        let report = Linter::new(catalog()).lint("// é\nfetch('/x');", "unicode.js");
        let evidence = &report.findings[0].evidence[0];
        assert_eq!(
            evidence.range.as_ref().map(|range| range.start.line),
            Some(2)
        );
        assert_eq!(evidence.source.as_deref(), Some("fetch"));
    }

    #[test]
    fn evidence_limit_is_source_ordered_and_applied_once() {
        let source = (0..20).map(|_| "fetch();\n").collect::<String>();
        let report = Linter::new(catalog()).lint(&source, "many.js");
        assert_eq!(report.findings.len(), 16);
        assert_eq!(report.findings.first().unwrap().range.start.line, 1);
        assert_eq!(report.findings.last().unwrap().range.start.line, 16);
    }

    #[test]
    fn enabled_rule_order_does_not_affect_findings() {
        let rule_a = Rule::builder("alpha.first")
            .label("First")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let rule_b = Rule::builder("beta.second")
            .label("Second")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("XMLHttpRequest"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment
            .add_globals(["fetch", "XMLHttpRequest"])
            .unwrap();
        let catalog =
            RuleCatalog::with_environment("test", vec![rule_a, rule_b], environment).unwrap();

        let source = "fetch('/a'); new XMLHttpRequest();";
        let report_asc = Linter::with_rules(
            catalog.clone(),
            [
                RuleId::parse("test:alpha.first").unwrap(),
                RuleId::parse("test:beta.second").unwrap(),
            ],
        )
        .unwrap()
        .lint(source, "order.js");
        let report_desc = Linter::with_rules(
            catalog,
            [
                RuleId::parse("test:beta.second").unwrap(),
                RuleId::parse("test:alpha.first").unwrap(),
            ],
        )
        .unwrap()
        .lint(source, "order.js");

        // Both runs produce identical findings regardless of internal order.
        assert_eq!(report_asc.findings.len(), report_desc.findings.len());
        for (a, b) in report_asc.findings.iter().zip(report_desc.findings.iter()) {
            assert_eq!(a.rule_id, b.rule_id);
            assert_eq!(a.range, b.range);
            assert_eq!(a.message, b.message);
        }
    }

    #[test]
    fn disabled_catalog_rules_do_not_produce_findings() {
        let rule_a = Rule::builder("alpha.first")
            .label("First")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let rule_b = Rule::builder("beta.second")
            .label("Second")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("XMLHttpRequest"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment
            .add_globals(["fetch", "XMLHttpRequest"])
            .unwrap();
        let catalog =
            RuleCatalog::with_environment("test", vec![rule_a, rule_b], environment).unwrap();
        let report = Linter::with_rules(catalog, [RuleId::parse("test:beta.second").unwrap()])
            .unwrap()
            .lint("fetch(); XMLHttpRequest();", "subset.js");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule_id.as_str(), "test:beta.second");
    }

    #[test]
    fn combines_provider_rules_with_overlapping_local_ids() {
        let first = Rule::builder("network.request")
            .label("First provider request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let second = Rule::builder("network.request")
            .label("Second provider request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("requestUrl"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_globals(["fetch", "requestUrl"]).unwrap();
        let linter = Linter::combine_with_environment(
            [
                Linter::new(RuleCatalog::new("first", vec![first]).unwrap()),
                Linter::new(RuleCatalog::new("second", vec![second]).unwrap()),
            ],
            environment,
        )
        .unwrap();

        let report = linter.lint("fetch('/a'); requestUrl('/b');", "combined.js");
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.findings[0].rule_id.as_str(), "first:network.request");
        assert_eq!(
            report.findings[1].rule_id.as_str(),
            "second:network.request"
        );
    }

    #[test]
    fn combined_linter_preserves_each_input_rule_selection() {
        let enabled_rule = Rule::builder("enabled")
            .label("Enabled")
            .category("test")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let disabled_rule = Rule::builder("disabled")
            .label("Disabled")
            .category("test")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("requestUrl"))
            .build()
            .unwrap();
        let enabled = Linter::new(RuleCatalog::new("first", vec![enabled_rule]).unwrap());
        let disabled =
            Linter::with_rules(RuleCatalog::new("second", vec![disabled_rule]).unwrap(), [])
                .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_globals(["fetch", "requestUrl"]).unwrap();

        let report = Linter::combine_with_environment([enabled, disabled], environment)
            .unwrap()
            .lint("fetch(); requestUrl();", "selection.js");

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule_id.as_str(), "first:enabled");
    }
}
