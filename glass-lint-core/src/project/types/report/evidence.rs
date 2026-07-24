use crate::project::types::SourceLocation;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Evidence {
    message: String,
    #[cfg_attr(feature = "serde", serde(default))]
    count: u32,
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "std::ops::Not::not"))]
    truncated: bool,
    location: Option<SourceLocation>,
}

impl Evidence {
    pub fn new(
        message: String,
        count: u32,
        truncated: bool,
        location: Option<SourceLocation>,
    ) -> Self {
        Self {
            message,
            count,
            truncated,
            location,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    pub(crate) fn set_message(&mut self, message: String) {
        self.message = message;
    }
}
