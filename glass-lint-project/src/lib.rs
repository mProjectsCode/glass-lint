//! Bounded filesystem construction for Glass Lint projects.
//!
//! The crate is split by the phases of project construction:
//! configuration and errors live in [`options`], source membership in
//! [`discovery`], module resolution in [`resolver`], and the public loading
//! API in [`loader`]. Core receives only owned sources and typed resolution
//! results; no resolver or filesystem type crosses that boundary.
//!
//! Discovery and resolution are bounded before the project session is linked,
//! keeping filesystem policy out of the provider-neutral core engine.

mod corpus;
mod discovery;
mod error;
mod loader;
mod options;
mod resolver;

pub use corpus::{CorpusFile, SourceCorpus};
pub use error::ProjectLoadError;
pub use loader::{ProjectLoadMetrics, ProjectLoader};
pub use options::{ProjectLoadOptions, ProjectSelection};

#[cfg(test)]
mod tests;
