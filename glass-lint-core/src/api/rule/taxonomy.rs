#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct ApiCategory(String);

impl ApiCategory {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().trim().to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn is_valid(&self) -> bool {
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

impl From<&str> for ApiCategory {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ApiCategory {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiSeverity {
    Info,
    Warning,
    Error,
}

impl ApiSeverity {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    pub(crate) fn as_diagnostic_severity(self) -> Severity {
        match self {
            Self::Info => Severity::Info,
            Self::Warning => Severity::Warning,
            Self::Error => Severity::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}
use crate::Severity;
