mod error;
mod matcher;
mod normalization;
mod taxonomy;
mod validation;

pub use error::{ApiCatalogError, ApiRuleBuildError};
pub(crate) use matcher::ApiMatcher;
pub use matcher::{
    ArgumentConstraint, ArgumentMatcher, CallMatcher, CallProvenance, ClassMatcher,
    ConstructorMatcher, FlowCompletion, FlowCondition, FlowSinkMatcher, InstanceMemberCallMatcher,
    Matcher, MemberCallMatcher, MemberCallProvenance, MemberReadMatcher, MemberReadProvenance,
    ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, ReturnedMemberCallMatcher,
    ReturnedMemberReadMatcher, ValueMatcher, canonical_rooted_chain,
};
pub(crate) use matcher::{StaticStringPredicate, ValueMatcherKind};
pub use taxonomy::{ApiCategory, ApiSeverity, Confidence};

#[derive(Debug, Clone)]
pub struct ApiRule {
    id: String,
    label: String,
    category: ApiCategory,
    severity: ApiSeverity,
    confidence: Confidence,
    matchers: Vec<Matcher>,
    compiled_matcher: crate::api::compiler::CompiledMatcherPlan,
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

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    #[must_use]
    pub fn category(&self) -> &ApiCategory {
        &self.category
    }
    #[must_use]
    pub fn severity(&self) -> ApiSeverity {
        self.severity
    }
    #[must_use]
    pub fn confidence(&self) -> Confidence {
        self.confidence
    }
    #[must_use]
    pub fn matchers(&self) -> &[Matcher] {
        &self.matchers
    }

    /// Clone the normalized matcher compiled at the rule boundary.  Catalog
    /// construction may copy this immutable plan, but must never normalize
    /// the rule again for each file.
    pub(crate) fn matcher_for_compilation(&self) -> crate::api::compiler::CompiledMatcherPlan {
        self.compiled_matcher.clone()
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
    #[must_use]
    pub fn matcher(mut self, matcher: impl Into<Matcher>) -> Self {
        self.matchers.push(matcher.into());
        self
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub fn category(mut self, category: impl Into<ApiCategory>) -> Self {
        self.category = Some(category.into());
        self
    }

    #[must_use]
    pub fn severity(mut self, severity: ApiSeverity) -> Self {
        self.severity = Some(severity);
        self
    }

    #[must_use]
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
        if !crate::RuleId::valid_name(&id) {
            return Err(ApiRuleBuildError::InvalidId(id));
        }
        if !category.is_valid() {
            return Err(ApiRuleBuildError::InvalidCategory(
                category.as_str().to_string(),
            ));
        }

        for (index, matcher) in self.matchers.iter().enumerate() {
            validation::validate_matcher_at(matcher, index)
                .map_err(ApiRuleBuildError::InvalidMatcher)?;
        }

        let candidate = matcher::ApiMatcher::from_matchers(self.matchers);
        candidate
            .validate()
            .map_err(ApiRuleBuildError::InvalidMatcher)?;
        let matcher = candidate.normalized();
        if matcher.is_empty() {
            return Err(ApiRuleBuildError::MissingMatcher);
        }
        let compiled_matcher = crate::api::compiler::CompiledMatcherPlan::compile(&matcher);
        let matchers = matcher.into_matchers();
        Ok(ApiRule {
            id,
            label,
            category,
            severity,
            confidence,
            matchers,
            compiled_matcher,
        })
    }
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

    #[test]
    fn rejects_invalid_matcher_shapes_at_the_builder_boundary() {
        let error = ApiRule::builder("network.fetch")
            .label("rule")
            .category("network")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .matcher(Matcher::rooted_member_call("client..request"))
            .build()
            .unwrap_err();
        assert!(matches!(error, ApiRuleBuildError::InvalidMatcher(_)));

        let error = ObjectFlowMatcher::builder("incomplete")
            .source(ObjectSourceMatcher::returned_by(MemberCallMatcher::rooted(
                "document.createElement",
            )))
            .build()
            .unwrap_err();
        assert!(error.contains("condition"));
    }
}
