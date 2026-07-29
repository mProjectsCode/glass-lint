//! Public rule declarations and builder boundary.
//!
//! Rule metadata is validated when a rule is built. Matcher declarations are
//! validated and compiled at the catalog boundary.

#![allow(clippy::redundant_pub_crate)]

mod error;
mod module;
pub mod query;
mod taxonomy;

pub use error::{CompiledCatalogError, MatcherBuildError, RuleBuildError};
pub use module::ModuleSpecifierPattern;
pub(crate) use query::value::{ArgumentMatcherKind, StaticStringPredicateKind};
pub use query::{
    AllExpr, AnyExpr, EmissionDecl, EventQuery, EventRequirement, EventSpec, IdentitySpec,
    IntoQueryDecl, LifecycleQuery, QueryBuildError, QueryDecl, QueryDiagnostic, QueryExpr, VarId,
    lifecycle::{
        LifecycleCompletion, LifecycleCondition, LifecycleEvent, LifecycleSink, LifecycleSource,
    },
    value::{ArgumentConstraint, ArgumentIndex, ArgumentMatcher, ValueMatcher, ValueMatcherKind},
};
pub use taxonomy::{Category, Confidence};

pub use crate::Severity;

#[derive(Debug, Clone)]
/// Validated provider rule with canonical query declarations.
///
/// Query declarations are compiled into physical plans at catalog construction,
/// after which the source declarations are not retained.
pub struct Rule {
    /// Provider-local stable rule name.
    id: String,
    /// Human-readable rule description.
    description: String,
    /// Provider-defined category.
    category: Category,
    /// Report severity.
    severity: Severity,
    /// Evidence confidence.
    confidence: Confidence,
    /// Query declarations retained until catalog compilation.
    queries: Vec<QueryDecl>,
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
            queries: Vec::new(),
            duplicate_field: None,
            first_query_error: None,
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
    /// Borrow all query declarations (ordinary and lifecycle).
    pub fn queries(&self) -> &[QueryDecl] {
        &self.queries
    }
}

#[derive(Debug, Clone)]
/// Fluent rule builder whose `build` method validates rule metadata.
pub struct RuleBuilder {
    id: String,
    description: Option<String>,
    category: Option<Category>,
    severity: Option<Severity>,
    confidence: Option<Confidence>,
    queries: Vec<QueryDecl>,
    duplicate_field: Option<&'static str>,
    first_query_error: Option<QueryBuildError>,
}

impl RuleBuilder {
    #[must_use]
    /// Add one query declaration (the primary authoring API).
    ///
    /// Accepts either a [`QueryDecl`] directly or a
    /// [`Result<QueryDecl, QueryBuildError>`] (from the fallible
    /// constructors), storing the first construction error for
    /// deferred reporting at `build()` time.
    pub fn query(mut self, query: impl IntoQueryDecl) -> Self {
        match query.into_query_decl() {
            Ok(decl) => self.queries.push(decl),
            Err(e) => {
                if self.first_query_error.is_none() {
                    self.first_query_error = Some(e);
                }
            }
        }
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
    pub fn category(mut self, category: Category) -> Self {
        if self.category.is_some() {
            self.duplicate_field = Some("category");
        }
        self.category = Some(category);
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

    /// Validate metadata and construct the rule.
    pub fn build(self) -> Result<Rule, RuleBuildError> {
        if let Some(field) = self.duplicate_field {
            return Err(RuleBuildError::DuplicateField(field));
        }
        if let Some(err) = self.first_query_error {
            return Err(RuleBuildError::InvalidQuery(err));
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
        Ok(Rule {
            id,
            description,
            category,
            severity,
            confidence,
            queries: self.queries,
        })
    }
}

impl Rule {
    pub(crate) fn validate_and_normalize(self) -> Result<Self, MatcherBuildError> {
        if self.queries.is_empty() {
            return Err(MatcherBuildError::MissingRequired);
        }
        Ok(self)
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
        let cat = Category::new(category)
            .map_err(|_| RuleBuildError::InvalidCategory(category.trim().to_string()))?;
        Rule::builder(id)
            .description("rule")
            .category(cat)
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .query(QueryDecl::call_global("fetch"))
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
                    .category(Category::new("one").unwrap())
                    .category(Category::new("two").unwrap()),
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
    fn rejects_empty_and_incomplete_matchers() {
        // Empty declarations list (no queries) passes build but fails
        // validate_and_normalize
        assert!(
            Rule::builder("test.test")
                .description("desc")
                .category(Category::new("cat").unwrap())
                .severity(Severity::Warning)
                .confidence(Confidence::Medium)
                .build()
                .is_ok()
        );
    }
}
