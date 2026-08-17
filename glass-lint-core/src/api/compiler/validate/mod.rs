mod correlation_evidence;
mod error;
mod scope_types;
mod structure;

#[cfg(test)]
pub(crate) use correlation_evidence::pass_correlation_evidence;
#[cfg(test)]
pub(crate) use error::is_identity_empty;
pub(crate) use error::{
    ContradictionKind, LifecycleSource, QueryCompileError, SubjectRelation, SubjectRelationError,
    classify_lifecycle_source, classify_subject_relation, is_valid_identity_event_pair,
};
#[cfg(test)]
pub(crate) use scope_types::pass_scope_types;
#[cfg(test)]
pub(crate) use structure::pass_structure;

use crate::api::rule::query::QueryDecl;

/// Validate a single [`QueryDecl`] using the consolidated compiler passes.
pub(crate) fn validate_query_decl(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    structure::pass_structure(decl)?;
    scope_types::pass_scope_types(decl)?;
    correlation_evidence::pass_correlation_evidence(decl)
}
