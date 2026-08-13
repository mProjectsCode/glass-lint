use crate::{
    RuleId, Severity,
    project::types::{EvidenceTraces, SourceLocation},
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

impl MatchCertainty {
    /// Combine certainty from duplicate proofs. A complete all-path proof is
    /// stronger than a proof covering only some modeled paths.
    pub(crate) const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Definite, _) | (_, Self::Definite) => Self::Definite,
            (Self::Possible, Self::Possible) => Self::Possible,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Finding {
    rule_id: RuleId,
    message: String,
    severity: Severity,
    location: SourceLocation,
    evidence: EvidenceTraces,
    certainty: MatchCertainty,
}

impl Finding {
    pub fn new(
        rule_id: RuleId,
        message: String,
        severity: Severity,
        location: SourceLocation,
        evidence: EvidenceTraces,
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

    pub fn evidence(&self) -> &EvidenceTraces {
        &self.evidence
    }

    pub fn certainty(&self) -> MatchCertainty {
        self.certainty
    }

    pub(crate) fn has_primary(&self, other: &Self) -> bool {
        self.rule_id == other.rule_id && self.location == other.location
    }

    pub(crate) fn merge_duplicate(self, other: Self) -> Self {
        debug_assert!(self.has_primary(&other));
        Self {
            rule_id: self.rule_id,
            message: self.message,
            severity: self.severity,
            location: self.location,
            evidence: self.evidence.merge(other.evidence),
            certainty: self.certainty.merge(other.certainty),
        }
    }
}
