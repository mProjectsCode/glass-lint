mod error;
mod pass1_3;
mod pass4_10;

// Re-exports used by sibling modules and tests.
pub(crate) use error::{ContradictionKind, QueryCompileError};
// Individual pass functions exist only in test builds for targeted unit tests.
// Production validation uses the consolidated passes in `validate_query_decl`.
#[cfg(test)]
pub(crate) use pass1_3::{pass_type_checking, pass_variable_collection, pass_well_formedness};
pub(crate) use pass4_10::validate_query_decl;
#[cfg(test)]
pub(crate) use pass4_10::{
    pass_boundedness, pass_correlation_scope, pass_evidence_projection, pass_lifecycle_validation,
    pass_relation_availability,
};
