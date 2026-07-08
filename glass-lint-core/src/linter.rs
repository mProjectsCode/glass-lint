use std::collections::{BTreeMap, BTreeSet};

use swc_common::{SourceMap, Span, sync::Lrc};

use crate::matcher::{ApiRule, ApiSeverity, classify_api_usage, validate_catalog};
use crate::{
    Evidence, Finding, LintConfigError, LintReport, Position, RegistryError, RuleId, RuleMetadata,
    Severity, SourceRange,
};

#[derive(Clone, Debug)]
pub struct RuleCatalog {
    provider: String,
    rules: Vec<ApiRule>,
}

impl RuleCatalog {
    pub fn new(provider: impl Into<String>, rules: Vec<ApiRule>) -> Result<Self, RegistryError> {
        let provider = provider.into();
        RuleId::parse(format!("{provider}:placeholder"))?;
        validate_catalog(&rules).map_err(|error| match error {
            crate::matcher::ApiCatalogError::DuplicateRule(id) => RegistryError::InvalidRule(
                RuleId::parse(format!("{provider}:{id}"))
                    .expect("validated provider and catalog rule ID"),
                "duplicate rule".into(),
            ),
        })?;
        for rule in &rules {
            RuleId::parse(format!("{provider}:{}", rule.id))?;
        }
        Ok(Self { provider, rules })
    }

    pub fn metadata(&self) -> Vec<RuleMetadata> {
        self.rules
            .iter()
            .map(|rule| RuleMetadata {
                id: self.namespaced_id(&rule.id),
                description: rule.label.clone(),
                default_severity: severity(rule.severity),
                messages: BTreeMap::from([(
                    "detected".into(),
                    "Detected matching capability".into(),
                )]),
            })
            .collect()
    }

    pub fn rule_ids(&self) -> Vec<RuleId> {
        self.rules
            .iter()
            .map(|rule| self.namespaced_id(&rule.id))
            .collect()
    }

    fn namespaced_id(&self, id: &str) -> RuleId {
        RuleId::parse(format!("{}:{id}", self.provider)).expect("catalog IDs were validated")
    }
}

pub struct Linter {
    catalog: RuleCatalog,
    enabled: BTreeSet<RuleId>,
}

impl Linter {
    pub fn new(catalog: RuleCatalog) -> Self {
        let enabled = catalog.rule_ids().into_iter().collect();
        Self { catalog, enabled }
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
        Ok(Self { catalog, enabled })
    }

    pub fn catalog(&self) -> &RuleCatalog {
        &self.catalog
    }

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
            .filter(|rule| self.enabled.contains(&self.catalog.namespaced_id(&rule.id)))
            .cloned()
            .collect();
        let classification = classify_api_usage(Some(&parsed.program), &selected);
        let mut findings = Vec::new();
        for capability in classification.capabilities() {
            let mut ranges: Vec<_> = capability
                .evidence()
                .iter()
                .flat_map(|evidence| evidence_ranges(&parsed.source_map, &evidence.spans))
                .collect();
            ranges.sort_by_key(|range| (range.start.line, range.start.column));
            ranges.dedup();
            if ranges.is_empty() {
                ranges.push(source_range(source, 0, 0));
            }
            for range in ranges {
                findings.push(Finding {
                    rule_id: self.catalog.namespaced_id(capability.id()),
                    message_id: "detected".into(),
                    message: capability.label().into(),
                    severity: severity(capability.severity()),
                    range,
                    evidence: capability
                        .evidence()
                        .iter()
                        .map(|evidence| Evidence {
                            message: format!(
                                "{}: {} ({} occurrence{})",
                                evidence.kind().as_str(),
                                evidence.symbol(),
                                evidence.count(),
                                if evidence.count() == 1 { "" } else { "s" }
                            ),
                            range: None,
                            source: None,
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
    }
}

fn evidence_ranges(source_map: &Lrc<SourceMap>, spans: &[Span]) -> Vec<SourceRange> {
    spans
        .iter()
        .filter(|span| !span.is_dummy())
        .map(|span| source_range_from_span(source_map, *span))
        .collect()
}

fn source_range_from_span(source_map: &Lrc<SourceMap>, span: Span) -> SourceRange {
    let start = source_map.lookup_char_pos(span.lo());
    let end = source_map.lookup_char_pos(span.hi());
    SourceRange {
        start: Position {
            line: start.line as u32,
            column: start.col_display as u32 + 1,
        },
        end: Position {
            line: end.line as u32,
            column: end.col_display as u32 + 1,
        },
    }
}

fn position(source: &str, offset: usize) -> Position {
    let prefix = &source[..offset.min(source.len())];
    Position {
        line: prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1,
        column: prefix
            .rsplit_once('\n')
            .map_or(prefix.len(), |(_, tail)| tail.len()) as u32
            + 1,
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
    use crate::matcher::{ApiRule, Confidence};

    fn catalog() -> RuleCatalog {
        let rule = ApiRule::builder("network.fetch")
            .label("Uses fetch")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .global_calls(["fetch"])
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
    fn validates_custom_rule_selection() {
        let unknown = RuleId::parse("test:missing").unwrap();
        assert!(matches!(
            Linter::with_rules(catalog(), [unknown]),
            Err(LintConfigError::UnknownRule(_))
        ));
    }
}
