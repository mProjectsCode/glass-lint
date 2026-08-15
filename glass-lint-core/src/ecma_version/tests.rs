use super::*;

fn analyze(source: &str) -> EcmaVersionReport {
    let source = SourceFile::new("test.js", source).unwrap();
    analyze_ecma_version(&source).unwrap()
}

#[test]
fn empty_source_is_es5() {
    let report = analyze("");
    assert_eq!(report.minimum_version(), Some(EcmaVersion::Es5));
    assert_eq!(report.features().len(), 0);
}

#[test]
fn arrows_detect_nested_default_parameters() {
    let report = analyze(
        "const object = ({ value = 1 }) => value; \
             const array = ([value = 1]) => value; \
             const rest = ({ value = 1, ...other }) => value + other.value;",
    );
    assert!(report.features().contains(&EcmaFeature::DefaultParameters));
}

#[test]
fn pattern_and_object_features_are_recorded_without_confusing_defaults() {
    let report = analyze(
        "const { value = 1, ...rest } = source; \
             const values = [...items]; \
             const copy = { ...source }; \
             const outer = () => { const factory = () => { const { nested = 1 } = value; }; return factory; };",
    );
    assert!(!report.features().contains(&EcmaFeature::DefaultParameters));
    assert!(report.features().contains(&EcmaFeature::Destructuring));
    assert!(report.features().contains(&EcmaFeature::RestAndSpread));
    assert!(report.features().contains(&EcmaFeature::ObjectRestSpread));
}

#[test]
fn reports_the_highest_required_standard_version() {
    let report = analyze("const run = async () => await task();");
    assert_eq!(report.minimum_version(), Some(EcmaVersion::Es2017));
    assert_eq!(
        report.features(),
        &[
            EcmaFeature::LetConst,
            EcmaFeature::ArrowFunctions,
            EcmaFeature::AsyncFunctions,
            EcmaFeature::Await,
        ]
    );
}

#[test]
fn reports_non_ecmascript_syntax_without_claiming_compatibility() {
    let report = analyze("const view = <Panel />;");
    assert_eq!(report.minimum_version(), None);
    assert_eq!(
        report.features(),
        &[EcmaFeature::LetConst, EcmaFeature::Jsx]
    );
}

#[test]
fn reports_modules_and_newer_expression_features() {
    let report = analyze("export const value = source?.value ?? 1n;");
    assert_eq!(report.minimum_version(), Some(EcmaVersion::Es2020));
    assert!(report.features().contains(&EcmaFeature::Modules));
    assert!(report.features().contains(&EcmaFeature::OptionalChaining));
    assert!(report.features().contains(&EcmaFeature::NullishCoalescing));
    assert!(report.features().contains(&EcmaFeature::BigInt));
}

#[test]
fn reports_import_attributes_on_static_and_dynamic_imports() {
    let report = analyze(
        "import value from 'mod' with { type: 'json' }; \
             export { value } from 'mod' with { type: 'json' }; \
             export * from 'mod' with { type: 'json' }; \
             import('mod', { with: { type: 'json' } });",
    );
    assert_eq!(report.minimum_version(), None);
    assert!(report.features().contains(&EcmaFeature::ImportAttributes));
}

#[test]
fn reports_default_export_from_syntax() {
    let report = analyze("export value from 'mod';");
    assert_eq!(report.minimum_version(), None);
    assert!(report.features().contains(&EcmaFeature::ExportDefaultFrom));
}

#[test]
fn reports_auto_accessors() {
    let report = analyze("class Example { accessor value; }");
    assert_eq!(report.minimum_version(), None);
    assert!(report.features().contains(&EcmaFeature::AutoAccessors));
}

#[test]
fn explicit_limits_bound_standalone_analysis() {
    let source = SourceFile::new("deep.js", "(((value)))").unwrap();
    let limits = AnalysisLimits::default().with_syntax_depth(1).unwrap();
    let error = analyze_ecma_version_with_limits(&source, &limits).unwrap_err();
    assert_eq!(error.code().as_str(), "syntax_depth_exceeded");
}
