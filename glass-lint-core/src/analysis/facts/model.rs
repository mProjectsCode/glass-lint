//! Rule-independent semantic fact identities and payloads.
//!
//! Types have been moved to [`crate::analysis::model::fact`].

pub(in crate::analysis) use crate::analysis::model::fact::{
    ArgumentView, Building, CallArgInfo, CallUnwrap, ClassFactRole, ControlKind, ControlRegionId,
    FactId, FactPayload, Frozen, FunctionBoundary, MAX_FACTS, ParameterBinding, SemanticFact,
};
