use smol_str::SmolStr;

use super::endpoint::{LifecycleCallEndpoint, LifecycleCallTarget};
use crate::api::rule::query::{
    EventQuery, LifecycleQuery, MemberChain, QueryBuildError,
    canonical::CanonicalCollection,
    checked_name, limits,
    value::{ArgumentConstraints, ArgumentIndex, ArgumentMatcher, ValueMatcher},
};

macro_rules! define_lifecycle_adapter {
    ($trait_name:ident, $method:ident, $value:ty) => {
        #[doc = "Sealed fallible lifecycle input adapter."]
        pub trait $trait_name: private::Sealed {
            fn $method(self) -> Result<$value, QueryBuildError>;
        }

        impl $trait_name for $value {
            fn $method(self) -> Result<$value, QueryBuildError> {
                Ok(self)
            }
        }

        impl private::Sealed for $value {}

        impl $trait_name for Result<$value, QueryBuildError> {
            fn $method(self) -> Result<$value, QueryBuildError> {
                self
            }
        }

        impl private::Sealed for Result<$value, QueryBuildError> {}
    };
}

// ── LifecycleEvent ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LifecycleEventKind {
    PropertyWrite {
        property: SmolStr,
        value: ValueMatcher,
    },
    MemberCall {
        member: MemberChain,
        arguments: ArgumentConstraints,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LifecycleEvent {
    kind: LifecycleEventKind,
}

impl LifecycleEvent {
    pub(crate) fn kind(&self) -> &LifecycleEventKind {
        &self.kind
    }

    pub fn property_write(
        property: impl Into<SmolStr>,
        value: ValueMatcher,
    ) -> Result<Self, QueryBuildError> {
        let property = checked_name(property.into())?;
        Ok(Self {
            kind: LifecycleEventKind::PropertyWrite { property, value },
        })
    }

    pub fn member_call(
        member: impl Into<String>,
    ) -> Result<LifecycleEventBuilder, QueryBuildError> {
        let member = member.into();
        if member.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        let member = MemberChain::parse(member)?;
        Ok(LifecycleEventBuilder {
            event: LifecycleEventKind::MemberCall {
                member,
                arguments: ArgumentConstraints::new(),
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleEventBuilder {
    event: LifecycleEventKind,
}

impl LifecycleEventBuilder {
    pub fn arg(
        mut self,
        index: usize,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<Self, QueryBuildError> {
        let index = ArgumentIndex::try_from_usize(index)?;
        if let LifecycleEventKind::MemberCall { arguments, .. } = &mut self.event {
            arguments.push(index, matcher)?;
        }
        Ok(self)
    }

    pub fn build(self) -> LifecycleEvent {
        LifecycleEvent { kind: self.event }
    }
}

define_lifecycle_adapter!(IntoLifecycleEvent, into_lifecycle_event, LifecycleEvent);

impl IntoLifecycleEvent for LifecycleEventBuilder {
    fn into_lifecycle_event(self) -> Result<LifecycleEvent, QueryBuildError> {
        Ok(self.build())
    }
}

// ── LifecycleCondition ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LifecycleConditionKind {
    AnyOf(LifecycleEvents),
    AllOf(LifecycleEvents),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LifecycleCondition {
    kind: LifecycleConditionKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LifecycleEvents(CanonicalCollection<LifecycleEvent>);

impl LifecycleEvents {
    fn new<I>(events: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator,
        I::Item: IntoLifecycleEvent,
    {
        Ok(Self(CanonicalCollection::collect(
            events,
            limits::MAX_LIFECYCLE_EVENTS,
            QueryBuildError::EmptyLifecycleCondition,
            "lifecycle condition events",
            IntoLifecycleEvent::into_lifecycle_event,
        )?))
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, LifecycleEvent> {
        self.0.iter()
    }
}

impl LifecycleCondition {
    pub(crate) fn kind(&self) -> &LifecycleConditionKind {
        &self.kind
    }

    pub fn any_of<I>(events: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator,
        I::Item: IntoLifecycleEvent,
    {
        Ok(Self {
            kind: LifecycleConditionKind::AnyOf(LifecycleEvents::new(events)?),
        })
    }

    /// Require every event on the same tracked lifecycle object.
    ///
    /// This is a bounded multi-event correlation. It preserves path-local
    /// identity: an event from another object, an incompatible branch, an
    /// unknown value, or an exhausted alternative cannot complete the
    /// conjunction.
    pub fn all_of<I>(events: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator,
        I::Item: IntoLifecycleEvent,
    {
        Ok(Self {
            kind: LifecycleConditionKind::AllOf(LifecycleEvents::new(events)?),
        })
    }

    pub fn event(event: impl IntoLifecycleEvent) -> Result<Self, QueryBuildError> {
        Ok(Self {
            kind: LifecycleConditionKind::AllOf(LifecycleEvents::new([event])?),
        })
    }
}

// ── LifecycleCompletion ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LifecycleCompletionKind {
    Configuration,
    AnySink(LifecycleSinks),
    AllSinks(LifecycleSinks),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LifecycleCompletion {
    kind: LifecycleCompletionKind,
}

/// Non-empty, bounded, deterministic lifecycle sink collections.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LifecycleSinks(CanonicalCollection<LifecycleSink>);

impl LifecycleSinks {
    fn new<I>(sinks: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator,
        I::Item: IntoLifecycleSink,
    {
        Ok(Self(CanonicalCollection::collect(
            sinks,
            limits::MAX_LIFECYCLE_SINKS,
            QueryBuildError::EmptyLifecycleSinks,
            "lifecycle completion sinks",
            IntoLifecycleSink::into_lifecycle_sink,
        )?))
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, LifecycleSink> {
        self.0.iter()
    }
}

impl LifecycleCompletion {
    pub(crate) fn kind(&self) -> &LifecycleCompletionKind {
        &self.kind
    }

    pub fn configuration() -> Self {
        Self {
            kind: LifecycleCompletionKind::Configuration,
        }
    }

    pub fn any_sink<I, S>(sinks: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: IntoLifecycleSink,
    {
        Ok(Self {
            kind: LifecycleCompletionKind::AnySink(LifecycleSinks::new(sinks)?),
        })
    }

    /// Require every sink for the same tracked object, in path order.
    ///
    /// Unlike [`Self::any_sink`], one matching sink does not complete the
    /// flow. Each sink is a separate bounded correlation event; unknown,
    /// escaped, reassigned, or incompatible-path objects cannot satisfy the
    /// conjunction.
    pub fn all_sinks<I, S>(sinks: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: IntoLifecycleSink,
    {
        Ok(Self {
            kind: LifecycleCompletionKind::AllSinks(LifecycleSinks::new(sinks)?),
        })
    }
}

// Fallible completion input accepted by lifecycle query builders.
define_lifecycle_adapter!(
    IntoLifecycleCompletion,
    into_lifecycle_completion,
    LifecycleCompletion
);

// Fallible lifecycle query input accepted by `QueryDecl::lifecycle`.
define_lifecycle_adapter!(IntoLifecycleQuery, into_lifecycle_query, LifecycleQuery);

// ── LifecycleSink ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LifecycleSinkKind {
    ArgumentOf {
        endpoint: LifecycleCallEndpoint,
        index: ArgumentIndex,
    },
    AnyArgumentOf {
        endpoint: LifecycleCallEndpoint,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LifecycleSink {
    kind: LifecycleSinkKind,
}

impl LifecycleSink {
    pub(crate) fn kind(&self) -> &LifecycleSinkKind {
        &self.kind
    }

    /// Sink argument of a strict configured global call, e.g. `fetch(value)`.
    pub fn argument_of_global(
        name: impl Into<String>,
        index: usize,
    ) -> Result<Self, QueryBuildError> {
        Self::build_call_sink(
            name,
            |chain| LifecycleCallTarget::Global(chain.as_str().into()),
            Some(index),
        )
    }

    /// Sink argument of a rooted member call, e.g.
    /// `document.body.appendChild(value)`.
    pub fn argument_of_member(
        chain: impl Into<String>,
        index: usize,
    ) -> Result<Self, QueryBuildError> {
        Self::build_call_sink(
            chain,
            |chain| LifecycleCallTarget::RootedMember(chain.path().clone()),
            Some(index),
        )
    }

    fn build_call_sink(
        chain: impl Into<String>,
        target: impl FnOnce(&MemberChain) -> LifecycleCallTarget,
        index: Option<usize>,
    ) -> Result<Self, QueryBuildError> {
        let chain = chain.into();
        if chain.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        let chain = MemberChain::parse(chain)?;
        let target = target(&chain);
        let endpoint = LifecycleCallEndpoint::new(chain, target);
        let kind = match index {
            Some(index) => LifecycleSinkKind::ArgumentOf {
                endpoint,
                index: ArgumentIndex::try_from_usize(index)?,
            },
            None => LifecycleSinkKind::AnyArgumentOf { endpoint },
        };
        Ok(Self { kind })
    }

    /// Sink of any argument of a strict configured global call.
    pub fn any_argument_of_global(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        Self::build_call_sink(
            name,
            |chain| LifecycleCallTarget::Global(chain.as_str().into()),
            None,
        )
    }

    /// Sink of any argument of a rooted member call.
    pub fn any_argument_of_member(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        Self::build_call_sink(
            chain,
            |chain| LifecycleCallTarget::RootedMember(chain.path().clone()),
            None,
        )
    }
}

// Fallible sink input accepted by [`LifecycleCompletion::any_sink`].
define_lifecycle_adapter!(IntoLifecycleSink, into_lifecycle_sink, LifecycleSink);

define_lifecycle_adapter!(IntoLifecycleSource, into_lifecycle_source, EventQuery);

define_lifecycle_adapter!(
    IntoLifecycleCondition,
    into_lifecycle_condition,
    LifecycleCondition
);

mod private {
    pub trait Sealed {}

    impl Sealed for super::LifecycleEventBuilder {}
}
