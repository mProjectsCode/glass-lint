use crate::api::rule::Confidence;

#[test]
fn confidence_thresholds_follow_semantic_strength() {
    assert!(Confidence::High.meets(Confidence::High));
    assert!(Confidence::High.meets(Confidence::Medium));
    assert!(Confidence::Medium.meets(Confidence::Medium));
    assert!(!Confidence::Low.meets(Confidence::Medium));
}
