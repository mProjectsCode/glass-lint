//! Interned abstract values and the bounded per-file arena.
//!
//! Types have been moved to [`crate::analysis::model::value`].

pub use crate::analysis::model::value::{
    CallableValue, MAX_VALUES, ObjectId, StaticObject, Value, ValueTable,
};
