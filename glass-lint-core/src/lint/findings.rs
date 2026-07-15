use super::Linter;
use super::ranges::{remove_contained_ranges, source_range, source_range_from_span};
use crate::api::classification::{ApiCapability, ApiClassificationResult};
use crate::api::rule::ApiSeverity;
use crate::diagnostic::{Evidence, Finding, SourceRange};
use swc_common::SourceMapper;

impl Linter {
    /// Turn classifier capabilities into report findings. Classification is
    /// kept separate from report assembly so source-range policy remains in
    /// this layer and semantic analysis stays provider-neutral.
    pub(super) fn findings_for(
        &self,
        classification: &ApiClassificationResult,
        source_map: &swc_common::sync::Lrc<swc_common::SourceMap>,
        source: &str,
    ) -> Vec<Finding> {
        classification
            .capabilities()
            .iter()
            .flat_map(|capability| self.findings_for_capability(capability, source_map, source))
            .collect()
    }

    fn findings_for_capability(
        &self,
        capability: &ApiCapability,
        source_map: &swc_common::sync::Lrc<swc_common::SourceMap>,
        source: &str,
    ) -> Vec<Finding> {
        let Some(rule_id) = self.catalog().rule_id(capability.rule_index).cloned() else {
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
                    .map(|span| Self::report_evidence(evidence, span, source_map))
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
        source_map: &swc_common::sync::Lrc<swc_common::SourceMap>,
    ) -> Evidence {
        Evidence {
            message: format!("{} of \"{}\"", evidence.kind().as_str(), evidence.symbol()),
            range: Some(source_range_from_span(source_map, span)),
            source: source_map.span_to_snippet(span).ok(),
        }
    }
}

pub(crate) fn contains_range(outer: &SourceRange, inner: &SourceRange) -> bool {
    let outer_start = (outer.start.line, outer.start.column);
    let outer_end = (outer.end.line, outer.end.column);
    let inner_start = (inner.start.line, inner.start.column);
    let inner_end = (inner.end.line, inner.end.column);
    outer_start <= inner_start && inner_end <= outer_end
}
