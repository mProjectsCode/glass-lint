//! Validated rule taxonomy and report severity types.

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
/// Provider-defined hierarchical category name.
pub struct Category(String);

impl Category {
    /// Trim, validate, and store a candidate category name.
    pub fn new(value: impl Into<String>) -> Result<Self, crate::api::rule::RuleBuildError> {
        let trimmed = value.into().trim().to_string();
        let category = Self(trimmed);
        if category.is_valid() {
            Ok(category)
        } else {
            Err(crate::api::rule::RuleBuildError::InvalidCategory(
                category.0,
            ))
        }
    }

    #[must_use]
    /// Borrow the canonical category spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the category obeys the lowercase path grammar.
    pub fn is_valid(&self) -> bool {
        !self.0.is_empty()
            && self.0.chars().enumerate().all(|(index, character)| {
                (index == 0 && character.is_ascii_lowercase())
                    || (index > 0
                        && (character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '-'
                            || character == '_'
                            || character == '.'
                            || character == '/'))
            })
            && !self.0.ends_with(['-', '_', '.', '/'])
            && !self.0.contains("..")
            && !self.0.contains("//")
    }
}



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
}
