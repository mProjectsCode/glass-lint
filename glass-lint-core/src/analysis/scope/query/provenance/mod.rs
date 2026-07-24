//! Provenance queries over the lexical scope graph.
//!
//! These methods deliberately keep identity, shadowing, and mutation checks
//! together. A rooted spelling is useful only when every relevant binding and
//! property write remains proven at the use position.

#![allow(clippy::match_same_arms)]

mod callable;
mod chain;
mod object;
