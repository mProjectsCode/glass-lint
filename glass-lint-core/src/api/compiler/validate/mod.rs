mod error;
mod pass1_3;
mod pass4_10;

pub(crate) use error::{
    ContradictionKind, LifecycleSource, QueryCompileError, SubjectRelation, SubjectRelationError,
    classify_lifecycle_source, classify_subject_relation, is_valid_identity_event_pair,
};
#[cfg(test)]
pub(crate) use pass1_3::pass_scope_types;
pub(crate) use pass4_10::validate_query_decl;
#[cfg(test)]
pub(crate) use pass4_10::{pass_correlation_evidence, pass_structure};
