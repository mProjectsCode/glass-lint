use crate::api::rule::{
    FirstError,
    query::{EventQuery, QueryBuildError, limits},
};

mod endpoint;
mod types;
#[allow(unused_imports)]
pub(crate) use endpoint::{LifecycleCallEndpoint, LifecycleCallTarget};
#[allow(unused_imports)]
pub use types::{
    IntoLifecycleCompletion, IntoLifecycleCondition, IntoLifecycleEvent, IntoLifecycleSink,
    IntoLifecycleSource, LifecycleCompletion, LifecycleCondition, LifecycleEvent,
    LifecycleEventBuilder, LifecycleSink,
};
#[allow(unused_imports)]
pub(crate) use types::{
    LifecycleCompletionKind, LifecycleConditionKind, LifecycleEventKind, LifecycleEvents,
    LifecycleSinkKind, LifecycleSinks,
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
            sources,
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

        // Validate only relationships between lifecycle stages. Collection
        // invariants are established by LifecycleEvents and LifecycleSinks.
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
        if let Err(error) = result {
            self.invalid_operation.record(error);
        }
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
    pub fn try_source<S: IntoLifecycleSource>(
        mut self,
        source: S,
    ) -> Result<Self, QueryBuildError> {
        self.state.stages.try_source(source)?;
        Ok(self)
    }

    pub fn condition(mut self, condition: LifecycleCondition) -> Self {
        let result = self.state.stages.try_condition(condition);
        self.state.record_operation(result);
        self
    }

    /// Set the lifecycle condition and return construction errors immediately.
    pub fn try_condition(
        mut self,
        condition: impl IntoLifecycleCondition,
    ) -> Result<Self, QueryBuildError> {
        self.state.stages.try_condition(condition)?;
        Ok(self)
    }

    pub fn completion(mut self, completion: LifecycleCompletion) -> Self {
        let result = self.state.stages.try_completion(completion);
        self.state.record_operation(result);
        self
    }

    /// Set lifecycle completion and return construction errors immediately.
    pub fn try_completion<C: IntoLifecycleCompletion>(
        mut self,
        completion: C,
    ) -> Result<Self, QueryBuildError> {
        self.state.stages.try_completion(completion)?;
        Ok(self)
    }

    pub fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        let LifecycleBuilderState {
            stages,
            invalid_operation,
        } = self.state;
        if let Some(error) = invalid_operation.take() {
            return Err(error);
        }
        stages.build()
    }
}

#[derive(Debug, Clone)]
pub struct CatalogLifecycleQueryBuilder {
    state: LifecycleBuilderState,
}

impl CatalogLifecycleQueryBuilder {
    fn record_error(&mut self, error: QueryBuildError) {
        self.state.invalid_operation.record(error);
    }

    fn record_operation(&mut self, result: Result<(), QueryBuildError>) {
        if let Err(error) = result {
            self.record_error(error);
        }
    }

    pub fn source<S: IntoLifecycleSource>(mut self, source: S) -> Self {
        let result = self.state.stages.try_source(source);
        self.record_operation(result);
        self
    }

    pub fn condition<C: IntoLifecycleCondition>(mut self, condition: C) -> Self {
        let result = self.state.stages.try_condition(condition);
        self.record_operation(result);
        self
    }

    pub fn completion<C: IntoLifecycleCompletion>(mut self, completion: C) -> Self {
        let result = self.state.stages.try_completion(completion);
        self.record_operation(result);
        self
    }

    pub fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        let LifecycleBuilderState {
            stages,
            invalid_operation,
        } = self.state;
        if let Some(error) = invalid_operation.take() {
            return Err(error);
        }
        stages.build()
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
            state: LifecycleBuilderState::new(symbol),
        }
    }
}

#[cfg(test)]
mod tests;
