//! Public rule, matcher, classification, and compilation API.
//!
//! Declarations in this layer are validated/compiled before analysis. The
//! runtime semantic pass consumes immutable plans and generic evidence types;
//! provider policy remains in the provider crates.

pub mod classification;
pub mod compiler;
pub mod rule;
