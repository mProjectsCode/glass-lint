//! Rule-independent semantic fact identities and payloads.
//!
//! Types have been moved to [`crate::analysis::model::fact`].

pub use crate::analysis::model::fact::{
    ArgumentView, CallArgInfo, CallUnwrap, ClassFactRole, ControlKind, ControlRegionId, FactId,
    FactKind, FactPayload, FunctionBoundary, MAX_FACTS, ParameterBinding, SemanticFact,
};
