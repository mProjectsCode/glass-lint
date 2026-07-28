use crate::{
    RuleId, Severity,
    project::{
        EvidenceList,
        types::{Evidence, SourceLocation},
    },
};

/// Whether a finding's identity proof holds for all modeled paths reaching the
/// occurrence or only for some paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MatchCertainty {
    /// The rule proof holds on every modeled path reaching the occurrence.
    Definite,
    /// The rule proof holds on at least one, but not all, modeled paths
    /// reaching the occurrence.
    Possible,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Finding {
    rule_id: RuleId,
    message: String,
    severity: Severity,
    location: SourceLocation,
    evidence: EvidenceList,
    certainty: MatchCertainty,
}

impl Finding {
    pub fn new(
        rule_id: RuleId,
        message: String,
        severity: Severity,
        location: SourceLocation,
        evidence: EvidenceList,
        certainty: MatchCertainty,
    ) -> Self {
        Self {
            rule_id,
            message,
            severity,
            location,
            evidence,
            certainty,
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

    pub fn certainty(&self) -> MatchCertainty {
        self.certainty
    }

    pub fn set_shared_evidence(&mut self, shared: std::sync::Arc<[Evidence]>) {
        self.evidence.set_shared(shared);
    }
}
