use crate::analysis::facts::{FactPayload, FactStream, Frozen, build_test_facts};

fn count_instance_calls(stream: &FactStream<Frozen>) -> usize {
    stream
        .facts()
        .iter()
        .filter(|fact| {
            matches!(
                &fact.payload,
                FactPayload::Call(call) if call.instance_class().is_some()
            )
        })
        .count()
}

fn count_method_instance_calls(stream: &FactStream<Frozen>) -> usize {
    stream
        .facts()
        .iter()
        .filter(|fact| {
            matches!(
                &fact.payload,
                FactPayload::Call(call)
                    if call.instance_class().is_some()
                        && call
                            .callee_name()
                            .is_some_and(|name| stream.names().resolve(name) == Some("method"))
            )
        })
        .count()
}

#[test]
fn ternary_instance_origins_do_not_cross_incompatible_arms() {
    for source in [
        "import { Foo } from 'a'; import { Bar } from 'b'; let value; flag ? value = new Foo() : value = new Bar(); value.method();",
        "import { Foo } from 'a'; import { Bar } from 'b'; let value; flag ? value = new Bar() : value = new Foo(); value.method();",
    ] {
        let stream = build_test_facts(source, "ternary-instance.js");
        assert_eq!(
            count_method_instance_calls(&stream),
            0,
            "incompatible ternary arms must not share an instance origin"
        );
    }
}

#[test]
fn ternary_class_origins_do_not_cross_incompatible_arms() {
    for source in [
        "import { Foo } from 'a'; import { Bar } from 'b'; let ctor; flag ? ctor = Foo : ctor = Bar; const value = new ctor(); value.method();",
        "import { Foo } from 'a'; import { Bar } from 'b'; let ctor; flag ? ctor = Bar : ctor = Foo; const value = new ctor(); value.method();",
    ] {
        let stream = build_test_facts(source, "ternary-class.js");
        assert_eq!(
            count_method_instance_calls(&stream),
            0,
            "incompatible ternary arms must not share a class origin"
        );
    }
}

#[test]
fn redeclaration_replaces_prior_instance_origin() {
    let source = r"
        import { Foo } from 'lib';
        var value = new Foo();
        var value;
        value.method();
    ";
    let stream = build_test_facts(source, "redeclaration.js");
    assert_eq!(
        count_instance_calls(&stream),
        0,
        "a later hoisted declaration must invalidate the earlier origin"
    );
}

#[test]
fn destructuring_write_replaces_prior_instance_origin() {
    let source = r"
        import { Foo } from 'lib';
        var value = new Foo();
        ({ value } = source);
        value.method();
    ";
    let stream = build_test_facts(source, "destructuring-write.js");
    assert_eq!(
        count_instance_calls(&stream),
        0,
        "a destructuring write must invalidate the earlier origin"
    );
}

/// Construction in try is visible to a call inside try.
#[test]
fn construction_inside_try_is_visible_there() {
    let source = r"
        import { Foo } from 'lib';
        function test() {
            try {
                let x = new Foo();
                x.method();
            } catch (e) {}
        }
    ";
    let stream = build_test_facts(source, "try-inside.js");
    assert!(
        count_instance_calls(&stream) > 0,
        "x.method() after new Foo() inside try should have instance_class"
    );
}

/// A value constructed inside try (and copied through a local that the
/// prepass cannot prove is always constructed) must not carry its instance
/// origin into the catch handler, because the throw may have occurred before
/// the assignment.
#[test]
fn try_origin_does_not_leak_into_catch_handler() {
    let source = r"
        import { Foo } from 'lib';
        function test() {
            let y;
            try {
                let x = new Foo();
                y = x;
            } catch (e) {
                y.method();
            }
        }
    ";
    let stream = build_test_facts(source, "try-catch-leak.js");
    assert_eq!(
        count_instance_calls(&stream),
        0,
        "y.method() in catch should not see instance origin from try"
    );
}

/// A value constructed only in the try path must not carry its instance
/// origin into the finalizer, because the throw may have prevented the
/// assignment.
#[test]
fn try_only_origin_does_not_leak_into_finally() {
    let source = r"
        import { Foo } from 'lib';
        function test() {
            let y;
            try {
                let x = new Foo();
                y = x;
            } catch (e) {
            } finally {
                y.method();
            }
        }
    ";
    let stream = build_test_facts(source, "try-only-finally.js");
    assert_eq!(
        count_instance_calls(&stream),
        0,
        "y.method() in finally should not see instance origin from only the try path"
    );
}

/// A value constructed before the try/catch retains its instance origin in
/// the finalizer because it is part of the incoming state.
#[test]
fn pre_try_origin_is_visible_in_finally() {
    let source = r"
        import { Foo } from 'lib';
        function test() {
            let y = new Foo();
            try {
            } catch (e) {
            } finally {
                y.method();
            }
        }
    ";
    let stream = build_test_facts(source, "pre-try-finally.js");
    assert!(
        count_instance_calls(&stream) > 0,
        "y.method() in finally should see instance origin when y was constructed before try"
    );
}
