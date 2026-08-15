use super::*;
#[test]
fn diagnostic_kind_table_contains_only_canonical_codes() {
    for kind in ALL {
        let owned: DiagnosticCode = (*kind).into();
        assert_eq!(DiagnosticCode::try_from(kind.as_str()), Ok(owned));
    }
}
