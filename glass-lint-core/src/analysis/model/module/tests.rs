use super::*;

#[test]
fn unknown_exports_reject_function_and_static_metadata() {
    let mut interface = ModuleInterface::default();
    interface.add_function_export("function", FunctionId::from_test(1));
    interface.add_static_string("text", "before");

    interface.mark_unknown_exports();
    interface.add_function_export("late-function", FunctionId::from_test(2));
    interface.add_static_string("late-text", "after");

    assert!(interface.is_unknown());
    assert_eq!(interface.exports().count(), 0);
    assert_eq!(interface.function_export("function"), None);
    assert_eq!(interface.static_string("text"), None);
    assert_eq!(interface.function_export("late-function"), None);
    assert_eq!(interface.static_string("late-text"), None);
}

#[test]
fn compatible_export_observations_merge_independently_of_order() {
    let function = FunctionId::from_test(1);
    let resolution = ModuleExport::Local {
        name: "value".into(),
    };

    let mut first = ModuleInterface::default();
    first.add_function_export("value", function);
    first.add_static_string("value", "text");
    first.add_export("value", resolution.clone());

    let mut second = ModuleInterface::default();
    second.add_export("value", resolution);
    second.add_static_string("value", "text");
    second.add_function_export("value", function);

    assert_eq!(first, second);
    assert_eq!(first.function_export("value"), Some(function));
    assert_eq!(first.static_string("value"), Some("text"));
}

#[test]
fn conflicting_export_observations_clear_all_metadata() {
    let mut interface = ModuleInterface::default();
    interface.add_export("value", ModuleExport::Value);
    interface.add_function_export("value", FunctionId::from_test(1));
    interface.add_static_string("value", "text");
    interface.add_function_export("value", FunctionId::from_test(2));

    let Some((name, export)) = interface.exports().next() else {
        panic!("conflict should retain an unknown export entry");
    };
    assert_eq!(name.as_str(), "value");
    assert_eq!(export, &ModuleExport::Unknown);
    assert_eq!(interface.function_export("value"), None);
    assert_eq!(interface.static_string("value"), None);
}

#[test]
fn conflicting_static_strings_clear_the_export_entry() {
    let mut interface = ModuleInterface::default();
    interface.add_static_string("value", "first");
    interface.add_static_string("value", "second");

    assert_eq!(interface.static_string("value"), None);
    let Some((name, export)) = interface.exports().next() else {
        panic!("conflict should retain an unknown export entry");
    };
    assert_eq!(name.as_str(), "value");
    assert_eq!(export, &ModuleExport::Unknown);
}

#[test]
fn request_constructors_retain_their_valid_kind_and_role_pair() {
    let span = ByteRange::new(0, 1).unwrap();
    let mut interface = ModuleInterface::default();
    interface.add_import_request(span, "imported", vec![ImportedBinding::named("default")]);
    interface.add_reexport_request(span, "reexported");
    interface.add_star_export_request(span, "starred");
    interface.add_dynamic_import_request(span, "dynamic");
    interface.add_require_request(span, "required");

    let requests = interface.requests().collect::<Vec<_>>();
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[0].kind(), ResolutionRequestKind::StaticImport);
    assert!(matches!(
        requests[0].role(),
        ModuleRequestRole::Import { .. }
    ));
    assert_eq!(requests[1].kind(), ResolutionRequestKind::StaticImport);
    assert_eq!(requests[1].role(), &ModuleRequestRole::ReExport);
    assert_eq!(requests[2].kind(), ResolutionRequestKind::StaticImport);
    assert_eq!(requests[2].role(), &ModuleRequestRole::StarExport);
    assert_eq!(requests[3].kind(), ResolutionRequestKind::DynamicImport);
    assert_eq!(requests[3].role(), &ModuleRequestRole::DynamicImport);
    assert_eq!(requests[4].kind(), ResolutionRequestKind::Require);
    assert_eq!(requests[4].role(), &ModuleRequestRole::Require);
}
