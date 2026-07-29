mod error;
mod pass1_3;
mod pass4_10;

// Re-exports are used by tests and sibling modules.
#[allow(unused_imports)]
pub(crate) use error::{ContradictionKind, QueryCompileError};
#[allow(unused_imports)]
pub(crate) use pass1_3::{pass_type_checking, pass_variable_collection, pass_well_formedness};
#[allow(unused_imports)]
pub(crate) use pass4_10::{
    pass_boundedness, pass_correlation_scope, pass_evidence_projection, pass_lifecycle_validation,
    pass_relation_availability, validate_query_decl,
};
