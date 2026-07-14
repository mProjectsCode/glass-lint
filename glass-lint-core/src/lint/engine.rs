use std::collections::BTreeSet;

use super::catalog::RuleCatalog;
use super::ranges::{remove_contained_ranges, source_range, source_range_from_span};
use crate::api::rule::ApiSeverity;
use crate::api::{
    classification::{ApiCapability, ApiClassificationResult},
    classifier::classify_compiled_api_usage,
    compiler::CompiledCatalog,
};
use crate::diagnostic::{Evidence, Finding, LintReport, SourceRange};
use crate::{REPORT_VERSION, RuleId};
use swc_common::SourceMapper;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LintConfigError {
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

pub struct Linter {
    catalog: RuleCatalog,
    enabled: BTreeSet<RuleId>,
    compiled: CompiledCatalog,
}

impl Linter {
    /// Apply provider-neutral engine configuration to this linter.
    pub fn configured(self, config: &crate::CoreConfig) -> Result<Self, LintConfigError> {
        match &config.rules {
            Some(rules) => Self::with_rules(self.catalog, rules.clone()),
            None => Ok(self),
        }
    }
    #[must_use]
    pub fn new(catalog: RuleCatalog) -> Self {
        let enabled = catalog.rule_ids().into_iter().collect();
        let compiled = catalog.compiled();
        Self {
            catalog,
            enabled,
            compiled,
        }
    }

    pub fn with_rules(
        catalog: RuleCatalog,
        enabled: impl IntoIterator<Item = RuleId>,
    ) -> Result<Self, LintConfigError> {
        let known: BTreeSet<_> = catalog.rule_ids().into_iter().collect();
        let enabled: BTreeSet<_> = enabled.into_iter().collect();
        if let Some(id) = enabled.iter().find(|id| !known.contains(*id)) {
            return Err(LintConfigError::UnknownRule(id.clone()));
        }
        let compiled = catalog.compiled();
        Ok(Self {
            catalog,
            enabled,
            compiled,
        })
    }

    /// Combine provider linters into one analysis pass under a shared host
    /// environment while preserving each linter's enabled rule selection.
    pub fn combine_with_environment(
        linters: impl IntoIterator<Item = Self>,
        environment: crate::Environment,
    ) -> Result<Self, crate::RuleCatalogError> {
        let mut catalogs = Vec::new();
        let mut enabled = BTreeSet::new();
        for linter in linters {
            catalogs.push(linter.catalog);
            enabled.extend(linter.enabled);
        }
        let catalog = RuleCatalog::combine_with_environment(catalogs, environment)?;
        Ok(Self::with_rules(catalog, enabled)
            .expect("combined catalog retains every selected rule"))
    }

    #[must_use]
    pub fn catalog(&self) -> &RuleCatalog {
        &self.catalog
    }

    /// Lints one JavaScript/JSX source file.
    ///
    /// Parsing stops after the first parser diagnostic.  Findings contain
    /// source ranges in one-based Unicode display columns. Evidence is bounded
    /// and each finding carries only the located occurrences enclosed by its
    /// primary range.
    #[must_use]
    pub fn lint(&self, source: &str, filename: &str) -> LintReport {
        let _span = tracing::info_span!(target: "glass_lint::lint", "lint", filename, source_bytes = source.len(), selected_rules = self.enabled.len()).entered();
        tracing::debug!(target: "glass_lint::parse", "parsing source");
        let parsed = match crate::parse::parse(source, filename) {
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

        let selected = self.selected_rule_indices();

        let classification = {
            let _span = tracing::debug_span!(target: "glass_lint::semantic", "semantic").entered();
            classify_compiled_api_usage(
                &parsed.program,
                &self.compiled,
                &self.catalog.rules,
                &selected,
                self.catalog.environment(),
            )
        };

        let mut findings = {
            let _span = tracing::debug_span!(target: "glass_lint::matching", "matching").entered();
            self.findings_for(&classification, &parsed, source)
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

    fn selected_rule_indices(&self) -> BTreeSet<usize> {
        self.catalog
            .rules
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                self.catalog
                    .rule_id(*index)
                    .is_some_and(|id| self.enabled.contains(id))
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// Turn classifier capabilities into report findings. Classification is
    /// kept separate from report assembly so source-range policy remains in
    /// this layer and semantic analysis stays provider-neutral.
    fn findings_for(
        &self,
        classification: &ApiClassificationResult,
        parsed: &crate::parse::ParsedSource,
        source: &str,
    ) -> Vec<Finding> {
        classification
            .capabilities()
            .iter()
            .flat_map(|capability| self.findings_for_capability(capability, parsed, source))
            .collect()
    }

    fn findings_for_capability(
        &self,
        capability: &ApiCapability,
        parsed: &crate::parse::ParsedSource,
        source: &str,
    ) -> Vec<Finding> {
        let Some(rule_id) = self.catalog.rule_id(capability.rule_index).cloned() else {
            return Vec::new();
        };
        let evidence: Vec<_> = capability
            .evidence()
            .iter()
            .flat_map(|evidence| {
                evidence
                    .spans
                    .iter()
                    .copied()
                    .filter(|span| !span.is_dummy())
                    .map(|span| Self::report_evidence(evidence, span, parsed))
            })
            .collect();
        let mut ranges: Vec<_> = evidence
            .iter()
            .filter_map(|evidence| evidence.range.clone())
            .collect();
        remove_contained_ranges(&mut ranges);
        if ranges.is_empty() {
            ranges.push(source_range(source, 0, 0));
        }

        ranges
            .into_iter()
            .map(|range| {
                let local_evidence = evidence
                    .iter()
                    .filter(|evidence| {
                        evidence
                            .range
                            .as_ref()
                            .is_some_and(|evidence_range| contains_range(&range, evidence_range))
                    })
                    .cloned()
                    .collect();
                Finding {
                    rule_id: rule_id.clone(),
                    message_id: "detected".into(),
                    message: capability.label().into(),
                    severity: match capability.severity() {
                        ApiSeverity::Info => crate::Severity::Info,
                        ApiSeverity::Warning => crate::Severity::Warning,
                        ApiSeverity::Error => crate::Severity::Error,
                    },
                    range,
                    evidence: local_evidence,
                }
            })
            .collect()
    }

    fn report_evidence(
        evidence: &crate::api::classification::ApiEvidence,
        span: swc_common::Span,
        parsed: &crate::parse::ParsedSource,
    ) -> Evidence {
        Evidence {
            message: format!("{} of \"{}\"", evidence.kind().as_str(), evidence.symbol()),
            range: Some(source_range_from_span(&parsed.source_map, span)),
            source: parsed.source_map.span_to_snippet(span).ok(),
        }
    }
}

fn contains_range(outer: &SourceRange, inner: &SourceRange) -> bool {
    let outer_start = (outer.start.line, outer.start.column);
    let outer_end = (outer.end.line, outer.end.column);
    let inner_start = (inner.start.line, inner.start.column);
    let inner_end = (inner.end.line, inner.end.column);
    outer_start <= inner_start && inner_end <= outer_end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::rule::{ApiRule, Confidence, Matcher};
    use crate::{Position, SourceRange};
    fn catalog() -> RuleCatalog {
        let rule = ApiRule::builder("network.fetch")
            .label("Uses fetch")
            .category("network")
            .severity(ApiSeverity::Warning)
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
        let rule = ApiRule::builder("vault.write")
            .label("Writes vault files")
            .category("vault")
            .severity(ApiSeverity::Info)
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
        let rule = ApiRule::builder("metadata.read")
            .label("Reads metadata")
            .category("metadata")
            .severity(ApiSeverity::Warning)
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
        assert_eq!(report.parse_diagnostics[0].code, "source_too_large");
        assert_eq!(report.parse_diagnostics[0].filename, "large.js");
        assert!(report.parse_diagnostics[0].range.is_none());
    }

    #[test]
    fn parse_diagnostics_carry_stable_location_context() {
        let report = Linter::new(catalog()).lint("fetch(", "broken.js");
        assert!(report.findings.is_empty());
        let diagnostic = &report.parse_diagnostics[0];
        assert_eq!(diagnostic.code, "syntax_error");
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
        let rule_a = ApiRule::builder("alpha.first")
            .label("First")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let rule_b = ApiRule::builder("beta.second")
            .label("Second")
            .category("network")
            .severity(ApiSeverity::Warning)
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
            catalog.clone(),
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
        let rule_a = ApiRule::builder("alpha.first")
            .label("First")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let rule_b = ApiRule::builder("beta.second")
            .label("Second")
            .category("network")
            .severity(ApiSeverity::Warning)
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
        let first = ApiRule::builder("network.request")
            .label("First provider request")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let second = ApiRule::builder("network.request")
            .label("Second provider request")
            .category("network")
            .severity(ApiSeverity::Warning)
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
        let enabled_rule = ApiRule::builder("enabled")
            .label("Enabled")
            .category("test")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let disabled_rule = ApiRule::builder("disabled")
            .label("Disabled")
            .category("test")
            .severity(ApiSeverity::Warning)
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
