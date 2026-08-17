use glass_lint_datastructures::NameTable;

use crate::analysis::{
    facts::{FactStream, stream::FrozenStorage},
    model::{
        fact::Building,
        value::{Value, ValueId, ValueTable},
    },
};

#[test]
fn failed_path_interning_is_recorded_as_incomplete() {
    let mut stream = FactStream::<Building>::new();
    stream.mark_path_exhausted();
    assert!(stream.path_exhausted());
    assert!(!stream.is_valid());
}

#[test]
fn name_exhaustion_is_recorded_and_invalidates_stream() {
    let mut stream = FactStream::<Building>::new();
    assert!(stream.is_valid());
    stream.mark_name_exhausted();
    assert!(!stream.is_valid());
}

#[test]
fn freeze_transitions_to_frozen_phase_with_both_tables() {
    let mut values = ValueTable::default();
    let string = values.intern(Value::StaticString("from-arena".into()));
    let storage = FrozenStorage::from_tables(NameTable::default(), values);
    let stream = FactStream::<Building>::new().freeze(storage);

    assert!(stream.is_valid());
    assert_eq!(stream.values().static_string(string), Some("from-arena"));
    assert!(stream.values().get(ValueId::from_test(u32::MAX)).is_none());
}

#[test]
fn frozen_values_are_borrowed_by_artifact_local_id() {
    let mut values = ValueTable::default();
    let string = values.intern(Value::StaticString("from-arena".into()));
    let storage = FrozenStorage::from_tables(NameTable::default(), values);
    let stream = FactStream::<Building>::new().freeze(storage);

    assert_eq!(stream.values().static_string(string), Some("from-arena"));
    assert!(stream.values().get(ValueId::from_test(u32::MAX)).is_none());
}
