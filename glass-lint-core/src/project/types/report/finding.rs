use crate::{
    RuleId, Severity,
    project::{EvidenceList, types::{Evidence, SourceLocation}},
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Finding {
    rule_id: RuleId,
    message: String,
    severity: Severity,
    location: SourceLocation,
    evidence: EvidenceList,
}

impl Finding {
    pub fn new(
        rule_id: RuleId,
        message: String,
        severity: Severity,
        location: SourceLocation,
        evidence: EvidenceList,
    ) -> Self {
        Self {
            rule_id,
            message,
            severity,
            location,
            evidence,
        }
    }

    pub fn rule_id(&self) -> &RuleId {
        &self.rule_id
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn location(&self) -> &SourceLocation {
        &self.location
    }

    pub fn evidence(&self) -> &EvidenceList {
        &self.evidence
    }

    pub fn set_shared_evidence(&mut self, shared: std::sync::Arc<[Evidence]>) {
        self.evidence.set_shared(shared);
    }
}
