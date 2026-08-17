use super::*;

#[cfg(test)]
mod test_support {
    use super::*;

    /// Build a reference fact with the stable defaults used by model tests.
    pub(super) fn reference(id: FactId, span: ByteRange, owner: FunctionId) -> SemanticFact {
        SemanticFact::new(
            FactStreamToken::for_test(),
            id,
            span,
            owner,
            FactPayload::Reference {
                value: ValueId::UNKNOWN,
                provenance: SymbolCallProvenance::Local,
                static_string_origin: None,
            },
        )
    }

    /// Build a call fact with an empty argument list and unknown optional
    /// identities; individual tests only provide the fields under test.
    pub(super) fn call(id: FactId, span: ByteRange, owner: FunctionId) -> SemanticFact {
        SemanticFact::new(
            FactStreamToken::for_test(),
            id,
            span,
            owner,
            FactPayload::Call(CallEvent::unknown(
                ValueId::UNKNOWN,
                span,
                SymbolCallProvenance::Local,
                Vec::new(),
            )),
        )
    }
}

#[cfg(test)]
mod control_region_tests {
    use super::*;

    #[test]
    fn control_regions_are_typed_and_orderable() {
        assert!(ControlRegionId::from_test(1) < ControlRegionId::from_test(2));
        assert_eq!(ControlRegionId::default(), ControlRegionId::from_test(0));
    }
}

#[cfg(test)]
mod fact_id_tests {
    use super::*;

    #[test]
    fn fact_id_from_index_rejects_overflow() {
        assert!(FactId::from_index(MAX_FACTS).is_none());
        assert!(FactId::from_index(MAX_FACTS - 1).is_some());
    }

    #[test]
    fn fact_id_index_rejects_overflow() {
        assert!(FactId::from_test(u32::MAX).index().is_none());
        assert!(FactId::from_test(0).index().is_some());
    }
}

#[cfg(test)]
mod call_arg_info_tests {
    use super::*;

    #[test]
    fn call_arg_info_unknown_creates_default() {
        let info = CallArgInfo::unknown();
        assert_eq!(info.value, ValueId::UNKNOWN);
        assert_eq!(info.base_value, ValueId::UNKNOWN);
        assert_eq!(info.base_path, PathId::EMPTY);
        assert!(!info.spread);
    }
}

#[cfg(test)]
mod parameter_binding_tests {
    use super::*;

    #[test]
    fn parameter_binding_constructs_with_all_fields() {
        let binding = ParameterBinding {
            parameter_index: 2,
            path: PathId::EMPTY,
            value: ValueId::UNKNOWN,
            default: Some(ValueId::UNKNOWN),
            rest: true,
        };
        assert_eq!(binding.parameter_index, 2);
        assert!(binding.rest);
        assert!(binding.default.is_some());
    }

    #[test]
    fn parameter_binding_without_default() {
        let binding = ParameterBinding {
            parameter_index: 0,
            path: PathId::EMPTY,
            value: ValueId::UNKNOWN,
            default: None,
            rest: false,
        };
        assert_eq!(binding.parameter_index, 0);
        assert!(binding.default.is_none());
        assert!(!binding.rest);
    }
}

#[cfg(test)]
mod semantic_fact_tests {
    use super::*;

    #[test]
    fn semantic_fact_new_creates_fact_with_all_fields() {
        let fact = super::test_support::reference(
            FactId::from_test(1),
            ByteRange::new(0, 5).unwrap(),
            FunctionId::from_test(0),
        );
        assert_eq!(fact.id(), FactId::from_test(1));
        assert!(matches!(fact.payload(), FactPayload::Reference { .. }));
    }

    #[test]
    fn semantic_fact_round_trips_span() {
        let range = ByteRange::new(10, 20).unwrap();
        let fact = super::test_support::call(FactId::from_test(2), range, FunctionId::from_test(1));
        assert_eq!(fact.id(), FactId::from_test(2));
    }
}

#[cfg(test)]
mod fact_payload_tests {
    use super::*;

    #[test]
    fn fact_payload_import_holds_module_string() {
        let payload = FactPayload::Import {
            module: "fs".into(),
        };
        let FactPayload::Import { module } = &payload else {
            panic!("expected Import");
        };
        assert_eq!(module, "fs");
    }

    #[test]
    fn fact_payload_class_declaration_holds_name_and_role() {
        let payload = FactPayload::Class {
            name: Some(SmolStr::new("MyClass")),
            role: ClassFactRole::Declaration,
            provenance: None,
        };
        let FactPayload::Class { name, role, .. } = &payload else {
            panic!("expected Class");
        };
        assert_eq!(name.as_ref().map(SmolStr::as_str), Some("MyClass"));
        assert_eq!(*role, ClassFactRole::Declaration);
    }

    #[test]
    fn fact_payload_class_instanceof_holds_role() {
        let payload = FactPayload::Class {
            name: None,
            role: ClassFactRole::InstanceofOperand,
            provenance: Some(ClassIdentity::new("React", "Component")),
        };
        let FactPayload::Class {
            role, provenance, ..
        } = &payload
        else {
            panic!("expected Class");
        };
        assert_eq!(*role, ClassFactRole::InstanceofOperand);
        assert_eq!(
            provenance.as_ref().map(ClassIdentity::module),
            Some(&SmolStr::new("React"))
        );
        assert_eq!(
            provenance.as_ref().map(ClassIdentity::export),
            Some(&SmolStr::new("Component"))
        );
    }
}
