use super::requirements::PlanRequirements;
use crate::api::{
    classification::MatchKind,
    rule::{
        ArgumentConstraint,
        query::{
            EventSpec, IdentitySpec,
            lifecycle::{LifecycleCompletion, LifecycleCondition},
        },
    },
};

// ── Normalized IR ──────────────────────────────────────────────────────────

/// A canonical normalized logical query with no `All` variant.
///
/// Normalization merges same-event conjunctions into one event node,
/// detects contradictions, and assigns dense deterministic variable slots.
/// Fields are private to `api/compiler`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedQuery {
    pub(crate) root: NormalizedRoot,
    pub(crate) emission: NormalizedEmission,
    pub(crate) requirements: PlanRequirements,
}

impl NormalizedQuery {
    pub(crate) fn root(&self) -> &NormalizedRoot {
        &self.root
    }

    pub(crate) fn emission(&self) -> &NormalizedEmission {
        &self.emission
    }

    pub(crate) fn requirements(&self) -> &PlanRequirements {
        &self.requirements
    }
}

/// Evidence emission for a normalized query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedEmission {
    pub(crate) primary_slot: u32,
    pub(crate) kind: MatchKind,
    pub(crate) symbol: String,
}

impl NormalizedEmission {
    pub(crate) fn kind(&self) -> MatchKind {
        self.kind
    }

    pub(crate) fn symbol(&self) -> &str {
        &self.symbol
    }
}

/// Normalized root expression — no `All` variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NormalizedRoot {
    Event(NormalizedEvent),
    Any(Box<[Self]>),
    Lifecycle(NormalizedLifecycle),
}

/// A single normalized event node with merged subject and arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedEvent {
    pub(crate) slot: u32,
    pub(crate) event: EventSpec,
    pub(crate) identity: Option<IdentitySpec>,
    pub(crate) subject: NormalizedSubject,
    pub(crate) arguments: Box<[ArgumentConstraint]>,
}

impl NormalizedEvent {
    pub(crate) fn slot(&self) -> u32 {
        self.slot
    }

    pub(crate) fn event(&self) -> &EventSpec {
        &self.event
    }

    pub(crate) fn identity(&self) -> Option<&IdentitySpec> {
        self.identity.as_ref()
    }

    pub(crate) fn subject(&self) -> &NormalizedSubject {
        &self.subject
    }

    pub(crate) fn arguments(&self) -> &[ArgumentConstraint] {
        &self.arguments
    }
}

/// Subject relationship in a normalized event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NormalizedSubject {
    Direct,
    Returned {
        producer: IdentitySpec,
        object_slot: u32,
    },
    Instance {
        constructor: IdentitySpec,
        object_slot: u32,
    },
}

/// Normalized lifecycle — preserves sources, condition, and completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedLifecycle {
    pub(crate) sources: Vec<NormalizedEvent>,
    pub(crate) condition: Option<LifecycleCondition>,
    pub(crate) completion: Option<LifecycleCompletion>,
}

impl NormalizedLifecycle {
    pub(crate) fn sources(&self) -> &[NormalizedEvent] {
        &self.sources
    }

    pub(crate) fn condition(&self) -> Option<&LifecycleCondition> {
        self.condition.as_ref()
    }

    pub(crate) fn completion(&self) -> Option<&LifecycleCompletion> {
        self.completion.as_ref()
    }
}
