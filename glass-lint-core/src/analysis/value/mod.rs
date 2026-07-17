//! Value identities and bounded interning.
//!
//! The value layer gives semantic analysis canonical, hashable identities for
//! bindings, callables, objects, and paths. Every arena/interner is bounded;
//! exhaustion maps to an explicit unknown result rather than an invented ID.

mod arena;
mod identity;
mod path;

pub(in crate::analysis) use arena::{CallableValue, MAX_VALUES, ObjectId, Value, ValueTable};
pub(in crate::analysis) use identity::{
    BindingId, BindingKey, BindingRoot, BindingVersion, FunctionId, SymbolPath, ValueId,
};
pub(in crate::analysis) use path::{PathId, PathInterner, PathSegment};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_value_ids_fail_closed() {
        let arena = ValueTable::default();
        assert!(arena.get(ValueId(u32::MAX)).is_none());
        assert!(arena.get(ValueId::UNKNOWN).is_some());
    }

    #[test]
    fn binding_versions_are_part_of_identity() {
        let mut first = BindingKey::new(BindingRoot::Binding {
            function: FunctionId(1),
            binding: BindingId(2),
            version: BindingVersion(0),
        });
        first.append_segment("value".into());
        let mut second = BindingKey::new(BindingRoot::Binding {
            function: FunctionId(1),
            binding: BindingId(2),
            version: BindingVersion(1),
        });
        second.append_segment("value".into());
        assert_ne!(first, second);
    }

    #[test]
    fn symbol_paths_keep_segments_out_of_identity_formatting() {
        let path = SymbolPath::from_chain("client.request").append_chain(".send");
        assert_eq!(path.to_string(), "client.request.send");
        assert!(!path.is_root());
        assert!(SymbolPath::from_chain("fetch").is_root());
        assert_eq!(
            SymbolPath::from_chain("fetch.bind")
                .without_bind_suffix()
                .expect("bind suffix should be removable")
                .to_string(),
            "fetch"
        );
    }
}
