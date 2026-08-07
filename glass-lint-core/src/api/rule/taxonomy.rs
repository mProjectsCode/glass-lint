//! Rule confidence values.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
/// Confidence assigned to the semantic evidence.
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
mod tests {
    use super::Confidence;

    #[test]
    fn confidence_thresholds_follow_semantic_strength() {
        assert!(Confidence::High.meets(Confidence::High));
        assert!(Confidence::High.meets(Confidence::Medium));
        assert!(Confidence::Medium.meets(Confidence::Medium));
        assert!(!Confidence::Low.meets(Confidence::Medium));
    }
}
