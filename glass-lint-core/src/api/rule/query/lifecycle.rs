use crate::api::rule::{
    DeferredBuilder,
    query::{EventQuery, QueryBuildError, limits},
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

        let completion = completion.ok_or(QueryBuildError::MissingLifecycleCompletion)?;
        match completion.kind() {
            LifecycleCompletionKind::AnySink(_) | LifecycleCompletionKind::AllSinks(_) => {}
            LifecycleCompletionKind::Configuration => {
                if condition.is_none() {
                    return Err(QueryBuildError::MissingLifecycleCondition);
                }
            }
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
pub struct CatalogLifecycleQueryBuilder {
    inner: DeferredBuilder<LifecycleStages, QueryBuildError>,
}

impl CatalogLifecycleQueryBuilder {
    pub fn source<S: IntoLifecycleSource>(mut self, source: S) -> Self {
        self.inner.record_with(|inner| inner.try_source(source));
        self
    }

    pub fn condition<C: IntoLifecycleCondition>(mut self, condition: C) -> Self {
        self.inner
            .record_with(|inner| inner.try_condition(condition));
        self
    }

    pub fn completion<C: IntoLifecycleCompletion>(mut self, completion: C) -> Self {
        self.inner
            .record_with(|inner| inner.try_completion(completion));
        self
    }

    pub fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        let (inner, first_error) = self.inner.into_parts();
        if let Some(error) = first_error {
            return Err(error);
        }
        inner.build()
    }
}

impl LifecycleQuery {
    pub fn catalog_builder(symbol: impl Into<String>) -> CatalogLifecycleQueryBuilder {
        CatalogLifecycleQueryBuilder {
            inner: DeferredBuilder::new(LifecycleStages::new(symbol)),
        }
    }
}

#[cfg(test)]
mod tests;
