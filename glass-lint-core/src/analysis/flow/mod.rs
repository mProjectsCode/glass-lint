//! Bounded semantic flow projection over the immutable fact stream.
//!
//! Local effects and indexes are built once from facts; matcher-specific
//! projection then follows only proven identities and bounded state. The
//! cross-module overlay composes summaries without re-traversing syntax.

pub(super) mod cross;
pub mod effect;
pub(super) mod index;
pub(super) mod matcher;
pub(super) mod projector;
pub(super) mod requirements;
pub(super) mod state;
pub(super) mod summary;
pub(super) mod table;
