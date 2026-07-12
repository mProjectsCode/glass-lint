use std::collections::{BTreeMap, BTreeSet};

use swc_common::{SourceMap, SourceMapper, Span, sync::Lrc};

use crate::matcher::{
    ApiRule, ApiSeverity, CompiledCatalog, classify_compiled_api_usage, validate_catalog,
};
use crate::{
    Evidence, Finding, LintConfigError, LintReport, Position, RegistryError, RuleId, RuleMetadata,
    Severity, SourceRange,
};

#[derive(Clone, Debug)]
pub struct RuleCatalog {
    rules: Vec<ApiRule>,
    namespaced: BTreeMap<String, RuleId>,
}

impl RuleCatalog {
    pub fn new(provider: impl Into<String>, rules: Vec<ApiRule>) -> Result<Self, RegistryError> {
        let provider = provider.into();
        RuleId::parse(format!("{provider}:placeholder"))?;
        validate_catalog(&rules).map_err(|error| match error {
            crate::matcher::ApiCatalogError::DuplicateRule(id) => {
                RegistryError::InvalidRule(format!("{provider}:{id}"), "duplicate rule".into())
            }
        })?;
        let mut namespaced = BTreeMap::new();
        for rule in &rules {
            let id = RuleId::parse(format!("{provider}:{}", rule.id()))?;
            namespaced.insert(rule.id().to_string(), id);
        }
        Ok(Self { rules, namespaced })
    }

    pub fn metadata(&self) -> Vec<RuleMetadata> {
        self.rules
            .iter()
            .filter_map(|rule| {
                self.namespaced_id(rule.id()).map(|id| RuleMetadata {
                    id: id.clone(),
                    description: rule.label().to_string(),
                    default_severity: severity(rule.severity()),
                    messages: BTreeMap::from([(
                        "detected".into(),
                        "Detected matching capability".into(),
                    )]),
                })
            })
            .collect()
    }

    pub fn rule_ids(&self) -> Vec<RuleId> {
        self.rules
            .iter()
            .filter_map(|rule| self.namespaced_id(rule.id()).cloned())
            .collect()
    }

    fn namespaced_id(&self, id: &str) -> Option<&RuleId> {
        self.namespaced.get(id)
    }
}

pub struct Linter {
    catalog: RuleCatalog,
    enabled: BTreeSet<RuleId>,
    compiled: CompiledCatalog,
}

impl Linter {
    pub fn new(catalog: RuleCatalog) -> Self {
        let enabled = catalog.rule_ids().into_iter().collect();
        let compiled = CompiledCatalog::from_rules(&catalog.rules);
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
        let compiled = CompiledCatalog::from_rules(&catalog.rules);
        Ok(Self {
            catalog,
            enabled,
            compiled,
        })
    }

    pub fn catalog(&self) -> &RuleCatalog {
        &self.catalog
    }

    /// Lints one JavaScript/JSX source file.
    ///
    /// Parsing stops after the first parser diagnostic.  Findings contain
    /// source ranges in one-based Unicode display columns, while evidence is
    /// bounded and carries the first matching source snippet for each group.
    pub fn lint(&self, source: &str, filename: &str) -> LintReport {
        let parsed = match crate::parse(source, filename) {
            Ok(parsed) => parsed,
            Err(error) => {
                return LintReport {
                    schema_version: crate::REPORT_VERSION,
                    tool_version: env!("CARGO_PKG_VERSION").into(),
                    findings: Vec::new(),
                    parse_diagnostics: vec![error],
                };
            }
        };
        let selected: Vec<_> = self
            .catalog
            .rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| {
                self.catalog
                    .namespaced_id(rule.id())
                    .is_some_and(|id| self.enabled.contains(id))
            })
            .map(|(index, _)| index)
            .collect();
        let classification = classify_compiled_api_usage(
            &parsed.program,
            &self.compiled,
            &self.catalog.rules,
            &selected,
        );
        let mut findings = Vec::new();
        for capability in classification.capabilities() {
            let Some(rule_id) = self.catalog.namespaced_id(capability.id()).cloned() else {
                continue;
            };
            let mut ranges: Vec<_> = capability
                .evidence()
                .iter()
                .flat_map(|evidence| evidence_ranges(&parsed.source_map, &evidence.spans))
                .collect();
            remove_contained_ranges(&mut ranges);
            if ranges.is_empty() {
                ranges.push(source_range(source, 0, 0));
            }
            for range in ranges {
                findings.push(Finding {
                    rule_id: rule_id.clone(),
                    message_id: "detected".into(),
                    message: capability.label().into(),
                    severity: severity(capability.severity()),
                    range,
                    evidence: capability
                        .evidence()
                        .iter()
                        .map(|evidence| {
                            let span = evidence.spans.iter().find(|span| !span.is_dummy()).copied();
                            Evidence {
                                message: format!(
                                    "{}: {} ({} occurrence{})",
                                    evidence.kind().as_str(),
                                    evidence.symbol(),
                                    evidence.count(),
                                    if evidence.count() == 1 { "" } else { "s" }
                                ),
                                range: span
                                    .map(|span| source_range_from_span(&parsed.source_map, span)),
                                source: span
                                    .and_then(|span| parsed.source_map.span_to_snippet(span).ok()),
                            }
                        })
                        .collect(),
                });
            }
        }
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
        LintReport {
            schema_version: crate::REPORT_VERSION,
            tool_version: env!("CARGO_PKG_VERSION").into(),
            findings,
            parse_diagnostics: Vec::new(),
        }
    }
}

fn severity(value: ApiSeverity) -> Severity {
    match value {
        ApiSeverity::Info => Severity::Info,
        ApiSeverity::Warning => Severity::Warning,
        ApiSeverity::Error => Severity::Error,
    }
}

fn evidence_ranges(source_map: &Lrc<SourceMap>, spans: &[Span]) -> Vec<SourceRange> {
    spans
        .iter()
        .filter(|span| !span.is_dummy())
        .map(|span| source_range_from_span(source_map, *span))
        .collect()
}

fn remove_contained_ranges(ranges: &mut Vec<SourceRange>) {
    // Sorting longer same-start intervals first means a single running end is
    // enough to recognize every contained interval. This preserves the prior
    // outermost-range behavior without comparing every pair of occurrences.
    ranges.sort_by(|left, right| {
        (left.start.line, left.start.column)
            .cmp(&(right.start.line, right.start.column))
            .then_with(|| (right.end.line, right.end.column).cmp(&(left.end.line, left.end.column)))
    });
    let mut enclosing_end = None;
    ranges.retain(|range| {
        let end = (range.end.line, range.end.column);
        if enclosing_end.is_some_and(|outer_end| end <= outer_end) {
            return false;
        }
        enclosing_end = Some(end);
        true
    });
}

fn source_range_from_span(source_map: &Lrc<SourceMap>, span: Span) -> SourceRange {
    let start = source_map.lookup_char_pos(span.lo());
    let end = source_map.lookup_char_pos(span.hi());
    SourceRange {
        start: Position {
            line: u32::try_from(start.line).unwrap_or(u32::MAX),
            column: u32::try_from(start.col_display)
                .unwrap_or(u32::MAX)
                .saturating_add(1),
        },
        end: Position {
            line: u32::try_from(end.line).unwrap_or(u32::MAX),
            column: u32::try_from(end.col_display)
                .unwrap_or(u32::MAX)
                .saturating_add(1),
        },
    }
}

fn position(source: &str, offset: usize) -> Position {
    let mut end = offset.min(source.len());
    while end > 0 && !source.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = &source[..end];
    Position {
        line: u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count())
            .unwrap_or(u32::MAX)
            .saturating_add(1),
        column: prefix
            .rsplit_once('\n')
            .map_or(prefix.chars().count(), |(_, tail)| tail.chars().count())
            .try_into()
            .unwrap_or(u32::MAX)
            .saturating_add(1),
    }
}

fn source_range(source: &str, start: usize, length: usize) -> SourceRange {
    SourceRange {
        start: position(source, start),
        end: position(source, start.saturating_add(length)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::{ApiRule, Confidence, Matcher};

    fn catalog() -> RuleCatalog {
        let rule = ApiRule::builder("network.fetch")
            .label("Uses fetch")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        RuleCatalog::new("test", vec![rule]).unwrap()
    }

    #[test]
    fn emits_one_located_finding_per_match() {
        let report = Linter::new(catalog()).lint("fetch('/a');\nfetch('/b');", "input.js");
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.findings[0].range.start.line, 1);
        assert_eq!(report.findings[1].range.start.line, 2);
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
        let catalog = RuleCatalog::new("test", vec![rule_a, rule_b]).unwrap();

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
        let catalog = RuleCatalog::new("test", vec![rule_a, rule_b]).unwrap();
        let report = Linter::with_rules(catalog, [RuleId::parse("test:beta.second").unwrap()])
            .unwrap()
            .lint("fetch(); XMLHttpRequest();", "subset.js");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule_id.as_str(), "test:beta.second");
    }
}
