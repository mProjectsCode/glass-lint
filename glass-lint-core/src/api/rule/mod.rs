//! Public rule declarations and builder boundary.
//!
//! Rule metadata is validated when a rule is built. Matcher declarations are
//! validated and compiled at the catalog boundary.

#![allow(clippy::redundant_pub_crate)]

mod error;
mod module;
pub mod query;
mod taxonomy;

#[derive(Debug, Clone)]
struct FirstError<E> {
    value: Option<E>,
}

impl<E> Default for FirstError<E> {
    fn default() -> Self {
        Self { value: None }
    }
}

impl<E> FirstError<E> {
    fn record(&mut self, error: E) {
        if self.value.is_none() {
            self.value = Some(error);
        }
    }

    fn take(self) -> Option<E> {
        self.value
    }
}

fn record_first_error<T, E>(slot: &mut FirstError<E>, result: Result<T, E>) {
    if let Err(error) = result {
        slot.record(error);
    }
}

pub use error::{
    CompiledCatalogError, CompilerInvariantDiagnostic, MatcherBuildError, PhysicalPlanDiagnostic,
    RuleBuildError,
};
pub use module::ModuleSpecifierPattern;
pub(crate) use query::value::{ArgumentMatcherKind, StaticStringPredicateKind, ValueMatcherKind};
pub use query::{
    EventQuery, EventRequirement, IntoQueryDecl, LifecycleQuery, QueryBuildError, QueryDecl,
    QueryDiagnostic,
    lifecycle::{
        IntoLifecycleCompletion, IntoLifecycleCondition, IntoLifecycleEvent, IntoLifecycleSink,
        IntoLifecycleSource, LifecycleCompletion, LifecycleCondition, LifecycleEvent,
        LifecycleSink,
    },
    value::{ArgumentConstraint, ArgumentIndex, ArgumentMatcher, ValueMatcher},
};
pub use taxonomy::Confidence;

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
            severity: None,
            confidence: None,
            queries: Vec::new(),
            duplicate_field: FirstError::default(),
        }
    }

    /// Start the deferred-error builder used by declarative rule catalogs.
    pub fn catalog_builder(id: impl Into<String>) -> CatalogRuleBuilder {
        CatalogRuleBuilder {
            inner: Self::builder(id),
            first_query_error: FirstError::default(),
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
    severity: Option<Severity>,
    confidence: Option<Confidence>,
    queries: Vec<QueryDecl>,
    duplicate_field: FirstError<&'static str>,
}

impl RuleBuilder {
    #[must_use]
    /// Add one already-validated query declaration.
    pub fn query(mut self, query: QueryDecl) -> Self {
        self.queries.push(query);
        self
    }

    /// Add one query declaration and return construction errors immediately.
    ///
    /// This is the preferred API for code that can propagate a
    /// [`QueryBuildError`]. [`Self::query`] accepts only a finished
    /// [`QueryDecl`]; declarative catalogs that need to defer fallible inputs
    /// should use [`CatalogRuleBuilder::query`] instead.
    pub fn try_query(self, query: impl IntoQueryDecl) -> Result<Self, QueryBuildError> {
        let mut builder = self;
        builder.try_add_query(query)?;
        Ok(builder)
    }

    fn try_add_query(&mut self, query: impl IntoQueryDecl) -> Result<(), QueryBuildError> {
        self.queries.push(query.into_query_decl()?);
        Ok(())
    }

    #[must_use]
    /// Add a deterministic sequence of query declarations.
    pub fn queries<I>(mut self, queries: I) -> Self
    where
        I: IntoIterator<Item = QueryDecl>,
    {
        for query in queries {
            self = self.query(query);
        }
        self
    }

    /// Add a sequence of query declarations, failing at the first invalid
    /// declaration.
    pub fn try_queries<I, Q>(mut self, queries: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = Q>,
        Q: IntoQueryDecl,
    {
        for query in queries {
            self = self.try_query(query)?;
        }
        Ok(self)
    }

    #[must_use]
    /// Set the human-readable description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        if self.description.is_some() {
            self.record_duplicate("description");
        }
        self.description = Some(description.into());
        self
    }

    #[must_use]
    /// Set report severity.
    pub fn severity(mut self, severity: Severity) -> Self {
        if self.severity.is_some() {
            self.record_duplicate("severity");
        }
        self.severity = Some(severity);
        self
    }

    #[must_use]
    /// Set evidence confidence.
    pub fn confidence(mut self, confidence: Confidence) -> Self {
        if self.confidence.is_some() {
            self.record_duplicate("confidence");
        }
        self.confidence = Some(confidence);
        self
    }

    fn record_duplicate(&mut self, field: &'static str) {
        self.duplicate_field.record(field);
    }

    /// Validate metadata and construct the rule.
    pub fn build(self) -> Result<Rule, RuleBuildError> {
        if let Some(field) = self.duplicate_field.take() {
            return Err(RuleBuildError::DuplicateField(field));
        }
        if self.queries.is_empty() {
            return Err(RuleBuildError::MissingQuery);
        }
        if self.queries.len() > query::limits::MAX_QUERY_ROOTS_PER_RULE {
            return Err(RuleBuildError::TooManyQueries(self.queries.len()));
        }
        let description = required_string(self.description, RuleBuildError::MissingDescription)?;
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
            severity,
            confidence,
            queries: self.queries,
        })
    }
}

#[derive(Debug, Clone)]
/// Deferred-error builder reserved for declarative provider catalogs.
pub struct CatalogRuleBuilder {
    inner: RuleBuilder,
    first_query_error: FirstError<QueryBuildError>,
}

impl CatalogRuleBuilder {
    #[must_use]
    pub fn query(mut self, query: impl IntoQueryDecl) -> Self {
        record_first_error(&mut self.first_query_error, self.inner.try_add_query(query));
        self
    }

    #[must_use]
    pub fn queries<I, Q>(mut self, queries: I) -> Self
    where
        I: IntoIterator<Item = Q>,
        Q: IntoQueryDecl,
    {
        for query in queries {
            self = self.query(query);
        }
        self
    }

    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.inner = self.inner.description(description);
        self
    }

    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.inner = self.inner.severity(severity);
        self
    }

    #[must_use]
    pub fn confidence(mut self, confidence: Confidence) -> Self {
        self.inner = self.inner.confidence(confidence);
        self
    }

    pub fn build(self) -> Result<Rule, RuleBuildError> {
        if let Some(error) = self.first_query_error.take() {
            return Err(RuleBuildError::InvalidQuery(error));
        }
        self.inner.build()
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
mod tests;
