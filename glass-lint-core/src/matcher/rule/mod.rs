mod error;
mod matcher;
mod taxonomy;

pub use error::{ApiCatalogError, ApiRuleBuildError};
pub use matcher::{
    ApiMatcher, ArgStringMatcher, CallMatcher, CallProvenance, ClassMatcher, ConstructorMatcher,
    FlowMatcher, FlowRequirement, FlowSinkArgs, FlowValueMatcher, InstanceMemberCallMatcher,
    Matcher, MemberCallMatcher, MemberCallProvenance, MemberReadMatcher, MemberReadProvenance,
    ReturnedMemberCallMatcher, ReturnedMemberReadMatcher, canonical_rooted_chain,
};
pub use taxonomy::{ApiCategory, ApiSeverity, Confidence};

#[derive(Debug, Clone)]
pub struct ApiRule {
    id: String,
    label: String,
    category: ApiCategory,
    severity: ApiSeverity,
    confidence: Confidence,
    matchers: Vec<Matcher>,
}

impl ApiRule {
    /// Retain enough matcher evidence for provider rules with several
    /// configured members without dropping valid capabilities during report
    /// construction. The limit remains finite to keep reports bounded.
    pub const EVIDENCE_LIMIT: usize = 16;

    pub fn builder(id: impl Into<String>) -> ApiRuleBuilder {
        ApiRuleBuilder {
            id: id.into(),
            label: None,
            category: None,
            severity: None,
            confidence: None,
            matchers: Vec::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn category(&self) -> &ApiCategory {
        &self.category
    }
    pub fn severity(&self) -> ApiSeverity {
        self.severity
    }
    pub fn confidence(&self) -> Confidence {
        self.confidence
    }
    pub fn matchers(&self) -> &[Matcher] {
        &self.matchers
    }
}

#[derive(Debug, Clone)]
pub struct ApiRuleBuilder {
    id: String,
    label: Option<String>,
    category: Option<ApiCategory>,
    severity: Option<ApiSeverity>,
    confidence: Option<Confidence>,
    matchers: Vec<Matcher>,
}

impl ApiRuleBuilder {
    pub fn matcher(mut self, matcher: impl Into<Matcher>) -> Self {
        self.matchers.push(matcher.into());
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn category(mut self, category: impl Into<ApiCategory>) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn severity(mut self, severity: ApiSeverity) -> Self {
        self.severity = Some(severity);
        self
    }

    pub fn confidence(mut self, confidence: Confidence) -> Self {
        self.confidence = Some(confidence);
        self
    }

    pub fn build(self) -> Result<ApiRule, ApiRuleBuildError> {
        let label = required_string(self.label, ApiRuleBuildError::MissingLabel)?;
        let category = self.category.ok_or(ApiRuleBuildError::MissingCategory)?;
        let severity = self.severity.ok_or(ApiRuleBuildError::MissingSeverity)?;
        let confidence = self
            .confidence
            .ok_or(ApiRuleBuildError::MissingConfidence)?;

        let id = self.id.trim().to_string();
        if id.is_empty() {
            return Err(ApiRuleBuildError::MissingId);
        }
        if !valid_local_rule_id(&id) {
            return Err(ApiRuleBuildError::InvalidId(id));
        }
        if !category.is_valid() {
            return Err(ApiRuleBuildError::InvalidCategory(
                category.as_str().to_string(),
            ));
        }

        let matcher = ApiMatcher::from_matchers(self.matchers).normalized();
        if matcher.is_empty() {
            return Err(ApiRuleBuildError::MissingMatcher);
        }
        let matchers = matcher.into_matchers();
        Ok(ApiRule {
            id,
            label,
            category,
            severity,
            confidence,
            matchers,
        })
    }
}

fn valid_local_rule_id(value: &str) -> bool {
    !value.is_empty()
        && value.chars().enumerate().all(|(index, character)| {
            (index == 0 && character.is_ascii_lowercase())
                || (index > 0
                    && (character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '-'
                        || character == '_'
                        || character == '.'))
        })
        && !value.starts_with(['-', '_', '.'])
        && !value.ends_with(['-', '_', '.'])
        && !value.contains("..")
}

fn required_string(
    value: Option<String>,
    missing_error: ApiRuleBuildError,
) -> Result<String, ApiRuleBuildError> {
    let value = value.ok_or_else(|| missing_error.clone())?;
    if value.trim().is_empty() {
        return Err(missing_error);
    }

    Ok(value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(id: &str, category: &str) -> Result<ApiRule, ApiRuleBuildError> {
        ApiRule::builder(id)
            .label("rule")
            .category(category)
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
    }

    #[test]
    fn rejects_noncanonical_rule_ids_and_categories() {
        for id in [
            "Network.fetch",
            ".network",
            "network.",
            "network..fetch",
            "network:fetch",
        ] {
            assert!(matches!(
                build(id, "network"),
                Err(ApiRuleBuildError::InvalidId(_))
            ));
        }
        assert!(matches!(
            build("network.fetch", "  "),
            Err(ApiRuleBuildError::InvalidCategory(_))
        ));
    }

    #[test]
    fn accepts_provider_category_paths_and_displayable_errors() {
        assert!(build("network.fetch", "browser/network").is_ok());
        let error = build("UPPER", "network").unwrap_err();
        assert!(error.to_string().contains("invalid rule ID"));
    }
}
