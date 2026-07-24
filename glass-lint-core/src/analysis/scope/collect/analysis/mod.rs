pub(super) mod classification;
pub(super) mod assignment;

pub(super) use assignment::{assignment_provenance, expression_is_mutable_static_object};
pub(super) use classification::{DeclarationClassification, classify_declaration};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
