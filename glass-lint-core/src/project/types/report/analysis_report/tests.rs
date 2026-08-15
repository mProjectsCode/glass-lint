use super::ReportCompletion;

#[test]
fn joining_completion_states_is_monotone() {
    use ReportCompletion::{Complete, Partial};

    assert_eq!(Complete.join(Complete), Complete);
    assert_eq!(Complete.join(Partial), Partial);
    assert_eq!(Partial.join(Complete), Partial);
    assert_eq!(Partial.join(Partial), Partial);
    assert!(!Complete.is_partial());
    assert!(Partial.is_partial());
}
