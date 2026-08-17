//! Rule confidence values.

/// Confidence assigned to the semantic evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Confidence {
    /// Strongly proven identity/flow.
    High,
    /// Partially constrained but supported identity/flow.
    Medium,
    /// Lower-confidence supported heuristic.
    Low,
}

impl Confidence {
    #[must_use]
    /// Return the stable serialized spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }

    #[must_use]
    /// Whether this confidence satisfies a minimum-confidence threshold.
    pub fn meets(self, minimum: Self) -> bool {
        self.rank() <= minimum.rank()
    }

    fn rank(self) -> u8 {
        match self {
            Self::High => 0,
            Self::Medium => 1,
            Self::Low => 2,
        }
    }
}

#[cfg(test)]
mod tests;
