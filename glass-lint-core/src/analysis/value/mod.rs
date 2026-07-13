//! Value identities and bounded interning.

mod arena;
mod identity;

pub(in crate::analysis) use arena::{CallableValue, ObjectId, Value, ValueArena};
pub(in crate::analysis) use identity::{
    BindingId, BindingKey, BindingRoot, BindingVersion, FunctionId, SymbolPath, ValueId,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_value_ids_fail_closed() {
        let arena = ValueArena::default();
        assert!(arena.get(ValueId(u32::MAX)).is_none());
        assert!(arena.get(ValueId::UNKNOWN).is_some());
    }

    #[test]
    fn binding_versions_are_part_of_identity() {
        let first = BindingKey {
            root: BindingRoot::Binding {
                function: FunctionId(1),
                binding: BindingId(2),
                version: BindingVersion(0),
            },
            path: vec!["value".into()],
        };
        let second = BindingKey {
            root: BindingRoot::Binding {
                function: FunctionId(1),
                binding: BindingId(2),
                version: BindingVersion(1),
            },
            path: vec!["value".into()],
        };
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
