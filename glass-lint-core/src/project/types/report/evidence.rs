use crate::project::types::SourceLocation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Error returned when an evidence collection violates its non-empty shape.
pub enum EvidenceConstructionError {
    /// An evidence trace has no steps.
    EmptyTrace,
    /// An evidence collection has no traces and is not marked truncated.
    EmptyTraces,
}

impl std::fmt::Display for EvidenceConstructionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTrace => f.write_str("evidence trace must have at least one step"),
            Self::EmptyTraces => f.write_str("evidence must have at least one trace"),
        }
    }
}

impl std::error::Error for EvidenceConstructionError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EvidenceRole {
    Source,
    Assignment,
    Requirement,
    Call,
    Return,
    Sink,
    Occurrence,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EvidenceStep {
    role: EvidenceRole,
    message: String,
    location: SourceLocation,
}

impl EvidenceStep {
    pub fn new(role: EvidenceRole, message: String, location: SourceLocation) -> Self {
        Self {
            role,
            message,
            location,
        }
    }

    pub fn role(&self) -> EvidenceRole {
        self.role
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EvidenceTrace {
    steps: Vec<EvidenceStep>,
}

impl EvidenceTrace {
    pub fn new(steps: Vec<EvidenceStep>) -> Result<Self, EvidenceConstructionError> {
        if steps.is_empty() {
            return Err(EvidenceConstructionError::EmptyTrace);
        }
        Ok(Self { steps })
    }

    pub fn steps(&self) -> &[EvidenceStep] {
        &self.steps
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EvidenceTraces {
    traces: Vec<EvidenceTrace>,
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "std::ops::Not::not"))]
    truncated: bool,
}

impl EvidenceTraces {
    pub fn new(traces: Vec<EvidenceTrace>) -> Result<Self, EvidenceConstructionError> {
        Self::with_truncation(traces, false)
    }

    pub fn with_truncation(
        traces: Vec<EvidenceTrace>,
        truncated: bool,
    ) -> Result<Self, EvidenceConstructionError> {
        if traces.is_empty() && !truncated {
            return Err(EvidenceConstructionError::EmptyTraces);
        }
        Ok(Self { traces, truncated })
    }

    pub fn traces(&self) -> &[EvidenceTrace] {
        &self.traces
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn is_empty(&self) -> bool {
        self.traces.is_empty()
    }

    pub fn len(&self) -> usize {
        self.traces.len()
    }

    /// Merge alternative traces while preserving a canonical, deterministic
    /// order. Trace step order is left untouched because it represents the
    /// order of events in the witness.
    pub(crate) fn merge(&self, other: &Self) -> Self {
        let mut traces = self
            .traces
            .iter()
            .chain(other.traces.iter())
            .cloned()
            .collect::<Vec<_>>();
        traces.sort();
        traces.dedup();
        Self {
            traces,
            truncated: self.truncated || other.truncated,
        }
    }

    /// Create an EvidenceTraces with a single fallback Occurrence step at the
    /// given location. Used when an external path must produce a valid trace
    /// without explicit step data.
    pub fn fallback(location: SourceLocation) -> Self {
        Self {
            traces: vec![EvidenceTrace {
                steps: vec![EvidenceStep::new(
                    EvidenceRole::Occurrence,
                    "evidence occurrence".into(),
                    location,
                )],
            }],
            truncated: false,
        }
    }
}

impl std::fmt::Display for EvidenceTraces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvidenceTraces")
            .field("trace_count", &self.traces.len())
            .field("truncated", &self.truncated)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{EvidenceConstructionError, EvidenceTrace, EvidenceTraces};

    #[test]
    fn rejects_empty_trace() {
        assert_eq!(
            EvidenceTrace::new(Vec::new()),
            Err(EvidenceConstructionError::EmptyTrace)
        );
    }

    #[test]
    fn rejects_empty_non_truncated_collection() {
        assert_eq!(
            EvidenceTraces::new(Vec::new()),
            Err(EvidenceConstructionError::EmptyTraces)
        );
        assert!(EvidenceTraces::with_truncation(Vec::new(), true).is_ok());
    }
}
