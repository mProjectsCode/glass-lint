mod error;
mod matcher;
mod taxonomy;

pub use error::{ApiCatalogError, ApiRuleBuildError};
pub use matcher::{
    ApiMatcher, ArgStringMatcher, CallMatcher, CallProvenance, ClassMatcher, ConstructorMatcher,
    FlowMatcher, FlowRequirement, FlowSinkArgs, FlowValueMatcher, Matcher, MemberCallMatcher,
    MemberCallProvenance, MemberReadMatcher, MemberReadProvenance, canonical_rooted_chain,
};
pub use taxonomy::{ApiCategory, ApiSeverity, Confidence};

#[derive(Debug, Clone)]
pub struct ApiRule {
    pub id: String,
    pub label: String,
    pub category: ApiCategory,
    pub severity: ApiSeverity,
    pub confidence: Confidence,
    pub matchers: Vec<Matcher>,
    pub implies: Vec<String>,
}

impl ApiRule {
    pub const EVIDENCE_LIMIT: usize = 5;

    pub fn builder(id: impl Into<String>) -> ApiRuleBuilder {
        ApiRuleBuilder {
            id: id.into(),
            label: None,
            category: None,
            severity: None,
            confidence: None,
            matchers: Vec::new(),
            implies: Vec::new(),
        }
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
    implies: Vec<String>,
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

    pub fn implies<I, S>(mut self, implies: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.implies.extend(implies.into_iter().map(Into::into));
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

        let matcher = ApiMatcher::from_matchers(self.matchers).normalized();
        let implies = normalized_strings(self.implies);
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
            implies,
        })
    }
}

fn required_string(
    value: Option<String>,
    missing_error: ApiRuleBuildError,
) -> Result<String, ApiRuleBuildError> {
    let value = value.ok_or(missing_error)?;
    if value.trim().is_empty() {
        return Err(missing_error);
    }

    Ok(value.trim().to_string())
}

fn normalized_strings(values: Vec<String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}
