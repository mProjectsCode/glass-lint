//! Public rule declarations and builder boundary.
//!
//! A [`Rule`] is fully validated and normalized before it is exposed to the
//! compiler. This keeps malformed IDs, taxonomy, matcher shapes, and
//! unbounded declarations out of analysis and report construction.

#![allow(clippy::redundant_pub_crate)]

mod error;
pub mod matcher;
mod module;
mod normalization;
mod taxonomy;
pub mod validation;

pub use error::{CompiledCatalogError, MatcherBuildError, RuleBuildError};
pub(crate) use matcher::MatcherFamily;
pub use matcher::{
    ArgumentConstraint, ArgumentMatcher, CallMatcher, ClassMatcher, ConstructorMatcher,
    FlowCompletion, FlowCondition, FlowSinkMatcher, InstanceMemberCallMatcher, Matcher, MatcherSet,
    MemberCallMatcher, MemberCallProvenance, MemberReadMatcher, ObjectEventMatcher,
    ObjectFlowMatcher, ObjectSourceMatcher, ReturnedMemberCallMatcher, ReturnedMemberReadMatcher,
    StaticStringPredicate, SymbolProvenance, ValueMatcher, ValueMatcherKind,
};
pub use module::ModuleSpecifierPattern;
pub use taxonomy::{Category, Confidence};

pub use crate::Severity;

#[derive(Debug, Clone)]
/// Validated provider rule with canonical matcher declarations.
pub struct Rule {
    /// Provider-local stable rule name. [`RuleCatalog`] adds the provider
    /// namespace when constructing the public rule ID.
    id: String,
    /// Human-readable rule description.
    description: String,
    /// Provider-defined category.
    category: Category,
    /// Report severity.
    severity: Severity,
    /// Evidence confidence.
    confidence: Confidence,
    /// Validated, normalized matcher declarations.
    matchers: Vec<Matcher>,
}

impl Rule {
    /// Retain enough matcher evidence for provider rules with several
    /// configured members without dropping valid capabilities during report
    /// construction. The limit remains finite to keep reports bounded.
    pub const EVIDENCE_LIMIT: usize = 16;

    /// Start a builder for one provider-local stable rule name.
    pub fn builder(id: impl Into<String>) -> RuleBuilder {
        RuleBuilder {
            id: id.into(),
            description: None,
            category: None,
            severity: None,
            confidence: None,
            matchers: Vec::new(),
            duplicate_field: None,
        }
    }

    #[must_use]
    /// Borrow the provider-local stable rule name.
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    /// Borrow the human-readable description.
    pub fn description(&self) -> &str {
        &self.description
    }

    #[must_use]
    /// Borrow the provider category.
    pub fn category(&self) -> &Category {
        &self.category
    }

    #[must_use]
    /// Return report severity.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    #[must_use]
    /// Return evidence confidence.
    pub fn confidence(&self) -> Confidence {
        self.confidence
    }

    #[must_use]
    /// Borrow normalized matcher declarations.
    pub fn matchers(&self) -> &[Matcher] {
        &self.matchers
    }
}

#[derive(Debug, Clone)]
/// Fluent rule builder whose `build` method validates all invariants.
pub struct RuleBuilder {
    id: String,
    description: Option<String>,
    category: Option<Category>,
    severity: Option<Severity>,
    confidence: Option<Confidence>,
    matchers: Vec<Matcher>,
    duplicate_field: Option<&'static str>,
}

impl RuleBuilder {
    #[must_use]
    /// Add one matcher declaration.
    pub fn matcher(mut self, matcher: impl Into<Matcher>) -> Self {
        self.matchers.push(matcher.into());
        self
    }

    #[must_use]
    /// Set the human-readable description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        if self.description.is_some() {
            self.duplicate_field = Some("description");
        }
        self.description = Some(description.into());
        self
    }

    #[must_use]
    /// Set the provider category.
    pub fn category(mut self, category: impl Into<Category>) -> Self {
        if self.category.is_some() {
            self.duplicate_field = Some("category");
        }
        self.category = Some(category.into());
        self
    }

    #[must_use]
    /// Set report severity.
    pub fn severity(mut self, severity: Severity) -> Self {
        if self.severity.is_some() {
            self.duplicate_field = Some("severity");
        }
        self.severity = Some(severity);
        self
    }

    #[must_use]
    /// Set evidence confidence.
    pub fn confidence(mut self, confidence: Confidence) -> Self {
        if self.confidence.is_some() {
            self.duplicate_field = Some("confidence");
        }
        self.confidence = Some(confidence);
        self
    }

    /// Validate metadata/matchers, normalize them, and construct the rule.
    pub fn build(self) -> Result<Rule, RuleBuildError> {
        if let Some(field) = self.duplicate_field {
            return Err(RuleBuildError::DuplicateField(field));
        }
        let description = required_string(self.description, RuleBuildError::MissingDescription)?;
        let category = self.category.ok_or(RuleBuildError::MissingCategory)?;
        let severity = self.severity.ok_or(RuleBuildError::MissingSeverity)?;
        let confidence = self.confidence.ok_or(RuleBuildError::MissingConfidence)?;

        let id = self.id.trim().to_string();
        if id.is_empty() {
            return Err(RuleBuildError::MissingId);
        }
        if !crate::RuleId::valid_name(&id) {
            return Err(RuleBuildError::InvalidId(id));
        }
        if !category.is_valid() {
            return Err(RuleBuildError::InvalidCategory(
                category.as_str().to_string(),
            ));
        }

        let candidate = matcher::MatcherSet::from_matchers(self.matchers);
        candidate
            .validate()
            .map_err(RuleBuildError::InvalidMatcher)?;
        let matcher = candidate.normalized();
        if matcher.is_empty() {
            return Err(RuleBuildError::MissingMatcher);
        }
        let matchers = matcher.into_matchers();

        Ok(Rule {
            id,
            description,
            category,
            severity,
            confidence,
            matchers,
        })
    }
}

fn required_string(
    value: Option<String>,
    missing_error: RuleBuildError,
) -> Result<String, RuleBuildError> {
    let value = value.ok_or_else(|| missing_error.clone())?;
    if value.trim().is_empty() {
        return Err(missing_error);
    }

    Ok(value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(id: &str, category: &str) -> Result<Rule, RuleBuildError> {
        Rule::builder(id)
            .description("rule")
            .category(category)
            .severity(Severity::Info)
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
                Err(RuleBuildError::InvalidId(_))
            ));
        }
        assert!(matches!(
            build("network.fetch", "  "),
            Err(RuleBuildError::InvalidCategory(_))
        ));
    }

    #[test]
    fn accepts_provider_category_paths_and_displayable_errors() {
        assert!(build("network.fetch", "browser/network").is_ok());
        let error = build("UPPER", "network").unwrap_err();
        assert!(error.to_string().contains("invalid rule ID"));
    }

    #[test]
    fn rejects_duplicate_required_metadata() {
        let cases = [
            (
                "description",
                Rule::builder("network.fetch")
                    .description("one")
                    .description("two"),
            ),
            (
                "category",
                Rule::builder("network.fetch")
                    .category("one")
                    .category("two"),
            ),
            (
                "severity",
                Rule::builder("network.fetch")
                    .severity(Severity::Info)
                    .severity(Severity::Warning),
            ),
            (
                "confidence",
                Rule::builder("network.fetch")
                    .confidence(Confidence::High)
                    .confidence(Confidence::Medium),
            ),
        ];
        for (field, builder) in cases {
            assert!(matches!(
                builder.build(),
                Err(RuleBuildError::DuplicateField(actual)) if actual == field
            ));
        }
    }

    #[test]
    fn rejects_invalid_matcher_shapes_at_the_builder_boundary() {
        let error = Rule::builder("network.fetch")
            .description("rule")
            .category("network")
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .matcher(Matcher::rooted_member_call("client..request"))
            .build()
            .unwrap_err();
        assert!(matches!(error, RuleBuildError::InvalidMatcher(_)));

        let error = ObjectFlowMatcher::builder("incomplete")
            .source(ObjectSourceMatcher::returned_by(MemberCallMatcher::rooted(
                "document.createElement",
            )))
            .build()
            .unwrap_err();
        assert!(matches!(error, MatcherBuildError::MissingRequired));

        let error = Rule::builder("class.invalid-global")
            .description("rule")
            .category("classes")
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .matcher(Matcher::heuristic_class(""))
            .build()
            .unwrap_err();
        assert!(matches!(error, RuleBuildError::InvalidMatcher(_)));
    }
}
