use crate::project::types::SourceLocation;

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
    pub fn new(steps: Vec<EvidenceStep>) -> Self {
        debug_assert!(
            !steps.is_empty(),
            "EvidenceTrace must have at least one step"
        );
        Self { steps }
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
    pub fn new(traces: Vec<EvidenceTrace>) -> Self {
        debug_assert!(
            !traces.is_empty(),
            "EvidenceTraces must have at least one trace"
        );
        Self {
            traces,
            truncated: false,
        }
    }

    pub fn with_truncation(traces: Vec<EvidenceTrace>, truncated: bool) -> Self {
        Self { traces, truncated }
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
        Self::with_truncation(traces, self.truncated || other.truncated)
    }

    /// Create an EvidenceTraces with a single fallback Occurrence step at the
    /// given location. Used when an external path must produce a valid trace
    /// without explicit step data.
    pub fn fallback(location: SourceLocation) -> Self {
        Self::new(vec![EvidenceTrace::new(vec![EvidenceStep::new(
            EvidenceRole::Occurrence,
            "match".into(),
            location,
        )])])
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
