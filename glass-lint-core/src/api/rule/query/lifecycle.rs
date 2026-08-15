use crate::api::rule::{
    FirstError,
    query::{EventQuery, QueryBuildError, limits},
    record_first_error,
};

mod endpoint;
mod types;
pub(crate) use endpoint::LifecycleCallTarget;
pub use types::{
    IntoLifecycleCompletion, IntoLifecycleCondition, IntoLifecycleEvent, IntoLifecycleQuery,
    IntoLifecycleSink, IntoLifecycleSource, LifecycleCompletion, LifecycleCondition,
    LifecycleEvent, LifecycleSink,
};
pub(crate) use types::{
    LifecycleCompletionKind, LifecycleConditionKind, LifecycleEventKind, LifecycleSinkKind,
};

// ── LifecycleQueryBuilder ─────────────────────────────────────────────
use crate::api::rule::query::LifecycleQuery;

#[derive(Debug, Clone)]
struct LifecycleStages {
    symbol: String,
    sources: Vec<EventQuery>,
    condition: Option<LifecycleCondition>,
    completion: Option<LifecycleCompletion>,
}

impl LifecycleStages {
    fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            sources: Vec::new(),
            condition: None,
            completion: None,
        }
    }

    fn source(&mut self, source: EventQuery) {
        self.sources.push(source);
    }

    fn try_source<S: IntoLifecycleSource>(&mut self, source: S) -> Result<(), QueryBuildError> {
        self.sources.push(source.into_lifecycle_source()?);
        Ok(())
    }

    fn try_condition<C: IntoLifecycleCondition>(
        &mut self,
        condition: C,
    ) -> Result<(), QueryBuildError> {
        if self.condition.is_some() {
            return Err(QueryBuildError::DuplicateLifecycleStage("condition"));
        }
        self.condition = Some(condition.into_lifecycle_condition()?);
        Ok(())
    }

    fn try_completion<C: IntoLifecycleCompletion>(
        &mut self,
        completion: C,
    ) -> Result<(), QueryBuildError> {
        if self.completion.is_some() {
            return Err(QueryBuildError::DuplicateLifecycleStage("completion"));
        }
        self.completion = Some(completion.into_lifecycle_completion()?);
        Ok(())
    }

    fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        let Self {
            symbol,
            mut sources,
            condition,
            completion,
        } = self;
        if symbol.trim().is_empty() {
            return Err(QueryBuildError::EmptyEvidenceSymbol);
        }
        if sources.is_empty() {
            return Err(QueryBuildError::MissingLifecycleSources);
        }
        if sources.len() > limits::MAX_LIFECYCLE_SOURCES {
            return Err(QueryBuildError::CollectionTooLarge(
                "lifecycle sources",
                sources.len(),
            ));
        }

        // Canonicalize source order so `LifecycleQuery` equality is
        // order-independent, matching the events and sinks collections.
        sources.sort();
        sources.dedup();

        if let Some(ref completion) = completion {
            match completion.kind() {
                LifecycleCompletionKind::AnySink(_) | LifecycleCompletionKind::AllSinks(_) => {}
                LifecycleCompletionKind::Configuration => {
                    if condition.is_none() {
                        return Err(QueryBuildError::MissingLifecycleCondition);
                    }
                }
            }
        } else {
            return Err(QueryBuildError::MissingLifecycleCompletion);
        }

        Ok(LifecycleQuery {
            symbol,
            sources,
            condition,
            completion,
        })
    }
}

#[derive(Debug, Clone)]
struct LifecycleBuilderState {
    stages: LifecycleStages,
    invalid_operation: FirstError<QueryBuildError>,
}

impl LifecycleBuilderState {
    fn new(symbol: impl Into<String>) -> Self {
        Self {
            stages: LifecycleStages::new(symbol),
            invalid_operation: FirstError::default(),
        }
    }

    fn record_operation(&mut self, result: Result<(), QueryBuildError>) {
        record_first_error(&mut self.invalid_operation, result);
    }

    fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        let Self {
            stages,
            invalid_operation,
        } = self;
        if let Some(error) = invalid_operation.take() {
            return Err(error);
        }
        stages.build()
    }
}

#[derive(Debug, Clone)]
pub struct LifecycleQueryBuilder {
    state: LifecycleBuilderState,
}

impl LifecycleQueryBuilder {
    pub fn source(mut self, source: EventQuery) -> Self {
        self.state.stages.source(source);
        self
    }

    /// Add a lifecycle source and return construction errors immediately.
    pub fn try_source<S: IntoLifecycleSource>(self, source: S) -> Result<Self, QueryBuildError> {
        let mut builder = self;
        builder.try_add_source(source)?;
        Ok(builder)
    }

    fn try_add_source<S: IntoLifecycleSource>(&mut self, source: S) -> Result<(), QueryBuildError> {
        self.state.stages.try_source(source)
    }

    pub fn condition(mut self, condition: LifecycleCondition) -> Self {
        let result = self.state.stages.try_condition(condition);
        self.state.record_operation(result);
        self
    }

    /// Set the lifecycle condition and return construction errors immediately.
    pub fn try_condition<C: IntoLifecycleCondition>(
        self,
        condition: C,
    ) -> Result<Self, QueryBuildError> {
        let mut builder = self;
        builder.try_add_condition(condition)?;
        Ok(builder)
    }

    fn try_add_condition<C: IntoLifecycleCondition>(
        &mut self,
        condition: C,
    ) -> Result<(), QueryBuildError> {
        self.state.stages.try_condition(condition)
    }

    pub fn completion(mut self, completion: LifecycleCompletion) -> Self {
        let result = self.state.stages.try_completion(completion);
        self.state.record_operation(result);
        self
    }

    /// Set lifecycle completion and return construction errors immediately.
    pub fn try_completion<C: IntoLifecycleCompletion>(
        self,
        completion: C,
    ) -> Result<Self, QueryBuildError> {
        let mut builder = self;
        builder.try_add_completion(completion)?;
        Ok(builder)
    }

    fn try_add_completion<C: IntoLifecycleCompletion>(
        &mut self,
        completion: C,
    ) -> Result<(), QueryBuildError> {
        self.state.stages.try_completion(completion)
    }

    pub fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        self.state.build()
    }
}

#[derive(Debug, Clone)]
pub struct CatalogLifecycleQueryBuilder {
    inner: LifecycleQueryBuilder,
    first_error: FirstError<QueryBuildError>,
}

impl CatalogLifecycleQueryBuilder {
    pub fn source<S: IntoLifecycleSource>(mut self, source: S) -> Self {
        record_first_error(&mut self.first_error, self.inner.try_add_source(source));
        self
    }

    pub fn condition<C: IntoLifecycleCondition>(mut self, condition: C) -> Self {
        record_first_error(
            &mut self.first_error,
            self.inner.try_add_condition(condition),
        );
        self
    }

    pub fn completion<C: IntoLifecycleCompletion>(mut self, completion: C) -> Self {
        record_first_error(
            &mut self.first_error,
            self.inner.try_add_completion(completion),
        );
        self
    }

    pub fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        if let Some(error) = self.first_error.take() {
            return Err(error);
        }
        self.inner.build()
    }
}

impl LifecycleQuery {
    pub fn builder(symbol: impl Into<String>) -> LifecycleQueryBuilder {
        LifecycleQueryBuilder {
            state: LifecycleBuilderState::new(symbol),
        }
    }

    pub fn catalog_builder(symbol: impl Into<String>) -> CatalogLifecycleQueryBuilder {
        CatalogLifecycleQueryBuilder {
            inner: Self::builder(symbol),
            first_error: FirstError::default(),
        }
    }
}

#[cfg(test)]
mod tests;
