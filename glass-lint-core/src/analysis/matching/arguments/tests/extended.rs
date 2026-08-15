use super::*;

#[test]
fn object_keys_matcher_accepts_expected_keys() {
    let stream = stream(
        "fetch({url: '/api', method: 'POST'});",
        &Environment::default(),
    );
    let root = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        constraints: compile_argument_constraints(&[ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ArgumentMatcher::object_keys(["url", "method"]).unwrap(),
        )]),
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };
    let index = build_index(&stream);
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
    );
    assert!(!evidence[0].is_empty(), "object keys should match");
}

#[test]
fn object_property_value_matcher_accepts_matching_property() {
    let stream = stream(
        "fetch({url: '/api', method: 'POST'});",
        &Environment::default(),
    );
    let root = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        constraints: compile_argument_constraints(&[ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ArgumentMatcher::object_property_value(
                "method",
                ValueMatcher::static_string().try_equals("POST").unwrap(),
            )
            .unwrap(),
        )]),
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };
    let index = build_index(&stream);
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
    );
    assert!(
        !evidence[0].is_empty(),
        "object property value should match"
    );
}

#[test]
fn argument_overlay_applies_static_string_from_identity_map() {
    let mut identities = ModuleIdentityMap::new();
    identities.insert(
        ModuleExportKey::new("api", "request"),
        ExportResolution::StaticString {
            value: "https://example.test".into(),
        },
    );
    let argument = CallArgInfo {
        value: ValueId::from_test(7),
        base_value: ValueId::UNKNOWN,
        base_path: PathId::EMPTY,
        spread: false,
        provenance: SymbolCallProvenance::ModuleExport {
            module: "api".into(),
            export: "request".into(),
        },
    };
    assert_eq!(
        MatcherEvaluator::new(
            &glass_lint_datastructures::NameTable::default(),
            &crate::analysis::model::value::ValueTable::default(),
            MatcherProjectOverlay::new(Some(&identities), None),
        )
        .argument_with_overlay(&argument)
        .static_string,
        Some("https://example.test")
    );
}

// ── Package 6: operation and argument-preparation tests ────────

#[test]
fn two_predicates_on_one_arg_prepare_argument_once() {
    let stream = stream("fetch('/api');", &Environment::default());
    let index = build_index(&stream);

    // Two constraints on the same argument index (0):
    //   equals("/api") AND starts_with_any(["/"])
    let constraints = compile_argument_constraints(&[
        ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string().try_equals("/api").unwrap(),
        ),
        ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string()
                .starts_with_any(["/"])
                .unwrap(),
        ),
    ]);
    assert_eq!(constraints.groups().len(), 1, "should be one group");

    let root = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        constraints,
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };

    let ops = run_with_ops(&stream, &index, &[root_input(&root)], None);

    // One candidate, one group → 1 argument preparation, 2 predicates
    assert_eq!(ops.candidates, 1, "one candidate (fetch call)");
    assert_eq!(ops.groups, 1, "one group");
    assert_eq!(ops.argument_preparations, 1, "argument prepared once");
    assert_eq!(ops.value_resolutions, 1, "argument value resolved once");
    assert_eq!(ops.predicates, 2, "two predicates applied");
}

#[test]
fn mixed_object_predicates_share_one_prepared_projection() {
    let stream = stream(
        "fetch({url: '/api', method: 'POST'});",
        &Environment::default(),
    );
    let index = build_index(&stream);
    let constraints = compile_argument_constraints(&[
        ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ArgumentMatcher::object_keys(["url", "method"]).unwrap(),
        ),
        ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ArgumentMatcher::object_property_value(
                "method",
                ValueMatcher::static_string().try_equals("POST").unwrap(),
            )
            .unwrap(),
        ),
    ]);
    let root = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        constraints,
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };

    let ops = run_with_ops(&stream, &index, &[root_input(&root)], None);
    assert_eq!(ops.candidates, 1);
    assert_eq!(ops.groups, 1);
    assert_eq!(ops.argument_preparations, 1);
    assert_eq!(ops.value_resolutions, 1);
    assert_eq!(ops.predicates, 2);
}

#[test]
fn several_argument_positions_each_prepared_once() {
    let stream = stream("fetch('/api', '/path');", &Environment::default());
    let index = build_index(&stream);

    // Constraints on index 0 AND index 1
    let constraints = compile_argument_constraints(&[
        ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string().try_equals("/api").unwrap(),
        ),
        ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(1),
            ValueMatcher::static_string().try_equals("/path").unwrap(),
        ),
    ]);
    assert_eq!(constraints.groups().len(), 2, "should be two groups");

    let root = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        constraints,
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };

    let ops = run_with_ops(&stream, &index, &[root_input(&root)], None);

    // One candidate, two groups → 2 argument preparations, 2 predicates
    assert_eq!(ops.candidates, 1, "one candidate");
    assert_eq!(ops.groups, 2, "two groups (index 0 and index 1)");
    assert_eq!(ops.argument_preparations, 2, "each index prepared once");
    assert_eq!(
        ops.value_resolutions, 2,
        "each argument value resolved once"
    );
    assert_eq!(ops.predicates, 2, "one predicate per group");
}

#[test]
fn duplicate_constraints_do_not_inflate_operation_counts() {
    let stream = stream("fetch('/api');", &Environment::default());
    let index = build_index(&stream);

    // Four identical constraints on the same index — compile_argument_constraints
    // should deduplicate them down to one group with one predicate.
    let constraints = compile_argument_constraints(&[
        ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string().try_equals("/api").unwrap(),
        ),
        ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string().try_equals("/api").unwrap(),
        ),
        ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string().try_equals("/api").unwrap(),
        ),
        ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string().try_equals("/api").unwrap(),
        ),
    ]);
    assert_eq!(constraints.groups().len(), 1, "deduplicated to one group");
    assert_eq!(
        constraints.groups()[0].predicates().len(),
        1,
        "deduplicated to one predicate"
    );

    let root = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        constraints,
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };

    let ops = run_with_ops(&stream, &index, &[root_input(&root)], None);

    // One candidate, one group, one predicate — same as the simple case.
    assert_eq!(ops.candidates, 1, "one candidate");
    assert_eq!(ops.groups, 1, "one group (deduplicated)");
    assert_eq!(ops.predicates, 1, "one predicate (deduplicated)");
    assert_eq!(
        ops.argument_preparations, 1,
        "argument prepared once despite four raw constraints"
    );
    assert_eq!(ops.value_resolutions, 1);
}

#[test]
fn operation_counts_scale_with_candidates() {
    let stream = stream(
        "fetch('/a'); fetch('/b'); fetch('/c');",
        &Environment::default(),
    );
    let index = build_index(&stream);

    let constraints = compile_argument_constraints(&[ArgumentConstraint::new(
        crate::api::rule::ArgumentIndex::new_unchecked(0),
        ValueMatcher::static_string(),
    )]);

    let root = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        constraints,
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };

    let ops = run_with_ops(&stream, &index, &[root_input(&root)], None);

    assert_eq!(ops.candidates, 3, "three candidates (one per call)");
    assert_eq!(ops.groups, 3, "one group per candidate");
    assert_eq!(ops.predicates, 3, "one predicate per candidate");
}

#[test]
fn static_alias_and_reassignment_preserves_matching() {
    // A static alias (`const x = '/api'; fetch(x);`) should match
    // the same as `fetch('/api');`.
    let stream = stream("const x = '/api'; fetch(x);", &Environment::default());
    let index = build_index(&stream);

    let constraints = compile_argument_constraints(&[ArgumentConstraint::new(
        crate::api::rule::ArgumentIndex::new_unchecked(0),
        ValueMatcher::static_string().try_equals("/api").unwrap(),
    )]);

    let root = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        constraints,
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };

    let ops = run_with_ops(&stream, &index, &[root_input(&root)], None);

    // The alias resolves through the value table: fetch(x) with x='/api'
    // should produce one matching candidate.
    assert_eq!(ops.candidates, 1, "one candidate (fetch(x))");
    assert_eq!(ops.groups, 1, "one group");
}
