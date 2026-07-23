use super::*;

#[test]
fn flow_calls_use_effective_call_and_apply_arguments() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "const first = document.createElement.call(document, 'script'); first.src = url;
         document.head.appendChild.call(document.head, first);
         const args = [second]; const second = document.createElement.apply(document, ['script']);
         second.src = url; document.head.appendChild.apply(document.head, [second]);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 2);
}

#[test]
fn flow_control_boundaries_fail_closed_after_loops_try_and_destructuring() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "let loopValue; while (condition) { loopValue = document.createElement('script'); loopValue.src = url; }
         document.head.appendChild(loopValue);
         let tryValue; try { tryValue = document.createElement('script'); tryValue.src = url; } catch (error) {}
         document.head.appendChild(tryValue);
         const source = document.createElement('script'); const { node } = source; node.src = url; document.head.appendChild(node);",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn flow_state_does_not_cross_conditional_branches_or_duplicate_sinks() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    assert_eq!(
        classify(
            "let script; if (condition) { script = document.createElement('script'); script.src = url; } else { script = local; } document.head.appendChild(script);",
            &rules,
        )
        .finding_count,
        0
    );
    assert_eq!(
        classify(
            "const script = document.createElement('script'); script.src = url; document.head.appendChild(script); document.head.appendChild(script);",
            &rules,
        )
        .finding_count,
        2
    );
}

#[test]
fn value_flow_respects_reassignment_and_order() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "let script = document.createElement('script'); script.src = getUrl(); script = document.createElement('div'); document.head.appendChild(script);
         const future = document.createElement('script'); document.head.appendChild(future); future.src = getUrl();",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn flow_kills_object_state_for_compound_writes_updates_and_delete() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "const compound = document.createElement('script'); compound.src = url; compound.src += suffix; document.head.appendChild(compound);
         const updated = document.createElement('script'); updated.src = url; updated.src++; document.head.appendChild(updated);
         const deleted = document.createElement('script'); deleted.src = url; delete deleted.src; document.head.appendChild(deleted);",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn value_flow_supports_member_call_configuration_and_helper_sinks() {
    let rules = [rule("test.flow")
        .declaration(MatcherDecl::from_object_flow(
            &ObjectFlowMatcher::builder("script insertion")
                .source(
                    ObjectSourceMatcher::returned_by("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("script")),
                )
                .configured_by(FlowCondition::event(
                    ObjectEventMatcher::member_call("setAttribute")
                        .arg(0, ValueMatcher::static_string().equals("src"))
                        .arg(1, ValueMatcher::any_value()),
                ))
                .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                    "document.head.appendChild",
                    0,
                )]))
                .build()
                .unwrap(),
        ))
        .build()
        .unwrap()];
    let result = classify(
        "function appendToHead(node) { document.head.appendChild(node); }
         const script = document.createElement('script'); script.setAttribute('src', getUrl()); appendToHead(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn flow_helpers_are_scope_and_assignment_aware() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "function append(node) { document.head.appendChild(node); }
         function local() { function append(node) { localSink(node); }
             const script = document.createElement('script'); script.src = url; append(script); }
         append = localAppend;
         const other = document.createElement('script'); other.src = url; append(other);",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn value_flow_supports_const_arrow_helper_sinks() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "const appendToHead = node => document.head.appendChild(node);
         const script = document.createElement('script'); script.src = getUrl(); appendToHead(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn value_flow_projects_nested_destructured_helper_arguments() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "function append([{ node }]) { document.head.appendChild(node); }
         const script = document.createElement('script'); script.src = url;
         append([{ node: script }]);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
    assert_eq!(
        classify(
            "function append([{ node }]) { document.head.appendChild(node); }
             const script = document.createElement('script'); script.src = url;
             append([{ other: script }]);",
            &rules,
        )
        .finding_count,
        0
    );
}

#[test]
fn value_flow_reaches_sinks_through_mutually_recursive_helpers() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "function first(node) { second(node); }
         function second(node) { first(node); document.head.appendChild(node); }
         const script = document.createElement('script'); script.src = url; first(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn value_flow_uses_precise_helper_parameter_defaults() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "const script = document.createElement('script'); script.src = url;
         function append(node = script) { document.head.appendChild(node); }
         append();",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
    let nested_default = classify(
        "const script = document.createElement('script'); script.src = url;
         function append({ node = script }) { document.head.appendChild(node); }
         append({});",
        &rules,
    );
    assert_capability_count(&nested_default, "test.flow", 1);
    let rest_parameter = classify(
        "const script = document.createElement('script'); script.src = url;
         function append(...nodes) { document.head.appendChild(nodes[0]); }
         append(script);",
        &rules,
    );
    assert_capability_count(&rest_parameter, "test.flow", 1);
}

#[test]
fn value_flow_follows_function_aliases_by_function_id() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "function append(node) { document.head.appendChild(node); }
         const alias = append;
         const script = document.createElement('script'); script.src = url; alias(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn helper_summaries_fail_closed_for_incompatible_invocations() {
    let rules = [rule("test.flow")
        .declaration(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "function append(node) { document.head.appendChild(node); }
         const script = document.createElement('script'); script.src = url;
         append(); append(script, extra);",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn value_flow_static_prefix_requires_static_values() {
    let rules = [rule("test.flow")
        .declaration(MatcherDecl::from_object_flow(
            &ObjectFlowMatcher::builder("remote element")
                .source(
                    ObjectSourceMatcher::returned_by("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("img")),
                )
                .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                    "src",
                    ValueMatcher::static_string().starts_with_any(["https://", "http://"]),
                )))
                .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                    "document.body.appendChild",
                    0,
                )]))
                .build()
                .unwrap(),
        ))
        .build()
        .unwrap()];
    let result = classify(
        "const remote = document.createElement('img'); remote.src = 'https://example.com/a.png'; document.body.appendChild(remote);
         const local = document.createElement('img'); local.src = '/a.png'; document.body.appendChild(local);
         const dynamic = document.createElement('img'); dynamic.src = getUrl(); document.body.appendChild(dynamic);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn flow_can_require_all_requirements() {
    let rules = [rule("test.flow")
        .declaration(MatcherDecl::from_object_flow(
            &ObjectFlowMatcher::builder("remote stylesheet")
                .source(
                    ObjectSourceMatcher::returned_by("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("link")),
                )
                .configured_by(FlowCondition::all_of([
                    ObjectEventMatcher::property_write(
                        "rel",
                        ValueMatcher::static_string().equals("stylesheet"),
                    ),
                    ObjectEventMatcher::property_write(
                        "href",
                        ValueMatcher::static_string().starts_with_any(["https://"]),
                    ),
                ]))
                .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                    "document.head.appendChild",
                    0,
                )]))
                .build()
                .unwrap(),
        ))
        .build()
        .unwrap()];
    let result = classify(
        "const good = document.createElement('link'); good.rel = 'stylesheet'; good.href = 'https://example.com/a.css'; document.head.appendChild(good);
         const missing = document.createElement('link'); missing.href = 'https://example.com/a.css'; document.head.appendChild(missing);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}
