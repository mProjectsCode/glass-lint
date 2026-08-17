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

#[derive(Debug, Clone)]
struct DeferredBuilder<T, E> {
    inner: T,
    first_error: FirstError<E>,
}

impl<T, E> DeferredBuilder<T, E> {
    fn new(inner: T) -> Self {
        Self {
            inner,
            first_error: FirstError::default(),
        }
    }

    fn record_with(&mut self, operation: impl FnOnce(&mut T) -> Result<(), E>) {
        if let Err(error) = operation(&mut self.inner) {
            self.first_error.record(error);
        }
    }

    fn into_parts(self) -> (T, Option<E>) {
        (self.inner, self.first_error.take())
    }

    fn map_inner(self, operation: impl FnOnce(T) -> T) -> Self {
        Self {
            inner: operation(self.inner),
            first_error: self.first_error,
        }
    }
}

pub use error::{
    CompiledCatalogError, CompilerInvariantDiagnostic, MatcherBuildError, PhysicalPlanDiagnostic,
    RuleBuildError,
};
pub use module::ModuleSpecifierPattern;
pub(crate) use query::value::{ArgumentMatcherKind, StaticStringPredicateKind, ValueMatcherKind};
pub use query::{
    EventQuery, EventRequirement, IntoQueryDecl, LifecycleQuery, MatchKind, QueryBuildError,
    QueryDecl, QueryDiagnostic,
    lifecycle::{
        IntoLifecycleCompletion, IntoLifecycleCondition, IntoLifecycleEvent, IntoLifecycleQuery,
        IntoLifecycleSink, IntoLifecycleSource, LifecycleCompletion, LifecycleCondition,
        LifecycleEvent, LifecycleSink,
    },
    value::{ArgumentConstraint, ArgumentIndex, ArgumentMatcher, ValueMatcher},
};
pub use taxonomy::Confidence;

pub use crate::Severity;
use crate::rule_id::RuleName;

/// Validated provider rule with canonical query declarations.
///
/// Query declarations are compiled into physical plans at catalog construction,
/// after which the source declarations are not retained.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Provider-local stable rule name.
    id: RuleName,
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
            inner: DeferredBuilder::new(Self::builder(id)),
        }
    }

    #[must_use]
    /// Borrow the provider-local stable rule name.
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    pub(crate) fn id_name(&self) -> &RuleName {
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

/// Fluent rule builder whose `build` method validates rule metadata.
#[derive(Debug, Clone)]
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
        let id = RuleName::new(id).map_err(RuleBuildError::InvalidId)?;
        Ok(Rule {
            id,
            description,
            severity,
            confidence,
            queries: self.queries,
        })
    }
}

/// Deferred-error builder reserved for declarative provider catalogs.
#[derive(Debug, Clone)]
pub struct CatalogRuleBuilder {
    inner: DeferredBuilder<RuleBuilder, QueryBuildError>,
}

impl CatalogRuleBuilder {
    #[must_use]
    pub fn query(mut self, query: impl IntoQueryDecl) -> Self {
        self.inner.record_with(|inner| inner.try_add_query(query));
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
        self.inner = self.inner.map_inner(|inner| inner.description(description));
        self
    }

    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.inner = self.inner.map_inner(|inner| inner.severity(severity));
        self
    }

    #[must_use]
    pub fn confidence(mut self, confidence: Confidence) -> Self {
        self.inner = self.inner.map_inner(|inner| inner.confidence(confidence));
        self
    }

    pub fn build(self) -> Result<Rule, RuleBuildError> {
        let (inner, first_error) = self.inner.into_parts();
        if let Some(error) = first_error {
            return Err(RuleBuildError::InvalidQuery(error));
        }
        inner.build()
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
