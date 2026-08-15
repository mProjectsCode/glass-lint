use super::*;

#[test]
fn literal_computed_member_chains_are_rooted() {
    assert_count(
        r#"host["files"]["read"]();"#,
        rule("test.literal-computed")
            .query(EventQuery::member_call_rooted("host.files.read"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn this_rooted_literal_computed_member_chains_are_rooted() {
    assert_count(
        r#"class PluginChild extends Plugin { onload() { this.app.vault["on"]("modify", handler); } }"#,
        rule("test.this-literal-computed")
            .query(
                EventQuery::member_call_rooted("app.vault.on")
                    .unwrap()
                    .with_arg(
                        0,
                        ValueMatcher::static_string().try_equals("modify").unwrap(),
                    )
                    .unwrap()
                    .into_query(),
            )
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn concatenated_static_property_names_are_rooted() {
    assert_count(
        r#"window["fet"+"ch"]("/x");"#,
        rule("test.concatenated-computed")
            .query(EventQuery::member_call_rooted("window.fetch"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn constant_property_aliases_are_rooted() {
    assert_count(
        r#"const k="read";host.files[k]();"#,
        rule("test.constant-computed")
            .query(EventQuery::member_call_rooted("host.files.read"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn static_string_table_property_aliases_are_rooted() {
    assert_count(
        r#"const k=["read"];host.files[k[0]]();"#,
        rule("test.string-table-computed")
            .query(EventQuery::member_call_rooted("host.files.read"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn dynamic_computed_properties_do_not_match_rooted_members() {
    assert_count(
        r#"var k=Date.now()>0?"read":"write";host.files[k]();"#,
        rule("test.dynamic-computed-negative")
            .query(EventQuery::member_call_rooted("host.files.read"))
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn sequence_global_calls_preserve_global_provenance() {
    assert_count(
        r#"(0,fetch)("/x");"#,
        rule("test.sequence-global")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn bound_global_calls_preserve_global_provenance() {
    assert_count(
        r#"var f=fetch.bind(null);f("/x");"#,
        rule("test.bound-global")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn call_and_apply_preserve_global_provenance_when_receiver_is_static() {
    assert_count(
        r#"var f=fetch;f.call(null,"/x");f.apply(null,["/y"]);"#,
        rule("test.call-apply-global")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
        2,
    );
}

#[test]
fn optional_chained_aliases_preserve_rooted_member_arguments() {
    assert_count(
        r#"var c=app.commands;c?.execute?.("open");"#,
        rule("test.optional")
            .query(
                EventQuery::member_call_rooted("app.commands.execute")
                    .unwrap()
                    .with_arg_static_strings(0, ["open"])
                    .unwrap()
                    .into_query(),
            )
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn shadowed_globals_do_not_match_global_calls() {
    assert_count(
        r#"function a(fetch){fetch("/local")}a(function(){});"#,
        rule("test.shadowed-global-negative")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn static_string_arguments_follow_aliases_but_reject_dynamic_strings() {
    assert_count(
        r#"var f=fetch,u="/x";f(u);f("/"+name);"#,
        rule("test.static-string-arg")
            .query(
                EventQuery::call_global("fetch")
                    .unwrap()
                    .with_arg_static_string(0)
                    .unwrap()
                    .into_query(),
            )
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn static_object_arguments_are_reused_for_key_matching() {
    assert_count(
        r#"var o={url:"/x",method:"GET"};client.request(o);"#,
        rule("test.object-arg")
            .query(
                EventQuery::member_call_rooted("client.request")
                    .unwrap()
                    .with_arg_object_keys(0, ["url", "method"])
                    .unwrap()
                    .into_query(),
            )
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn sequence_object_arguments_are_reused_for_key_matching() {
    assert_count(
        r#"var o;(o={url:"/x",method:"GET"},client.request(o));"#,
        rule("test.sequence-object-arg")
            .query(
                EventQuery::member_call_rooted("client.request")
                    .unwrap()
                    .with_arg_object_keys(0, ["url", "method"])
                    .unwrap()
                    .into_query(),
            )
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn rooted_expression_arguments_follow_one_letter_aliases() {
    assert_count(
        r#"var f=vault.file,o=app;o.open(f);"#,
        rule("test.rooted-arg")
            .query(
                EventQuery::member_call_rooted("app.open")
                    .unwrap()
                    .with_arg(
                        0,
                        ArgumentMatcher::rooted_expressions(["vault.file"]).unwrap(),
                    )
                    .unwrap()
                    .into_query(),
            )
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn spread_object_arguments_do_not_satisfy_exact_key_matching() {
    assert_count(
        r#"var b={url:"/x"};client.request({...b,method:"GET"});"#,
        rule("test.spread-object-negative")
            .query(
                EventQuery::member_call_rooted("client.request")
                    .unwrap()
                    .with_arg_object_keys(0, ["url", "method"])
                    .unwrap()
                    .into_query(),
            )
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn named_helper_parameter_aliases_preserve_global_calls() {
    assert_count(
        r#"function n(t){t("/x")}n(fetch);"#,
        rule("test.named-helper")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn arrow_helper_parameter_aliases_preserve_global_calls() {
    assert_count(
        r#"var n=t=>t("/x");n(fetch);"#,
        rule("test.arrow-helper")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn helper_argument_objects_flow_to_member_call_key_matching() {
    assert_count(
        r#"function n(x){client.request(x)}n({url:"/x",method:"GET"});"#,
        rule("test.helper-object-flow")
            .query(
                EventQuery::member_call_rooted("client.request")
                    .unwrap()
                    .with_arg_object_keys(0, ["url", "method"])
                    .unwrap()
                    .into_query(),
            )
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn inconsistent_helper_calls_do_not_infer_parameter_aliases() {
    assert_count(
        r#"function n(t){t("/x")}n(fetch);n(localFetch);"#,
        rule("test.inconsistent-helper-negative")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn incomplete_helper_invocations_do_not_infer_parameter_aliases() {
    assert_count(
        r#"function n(t){t(\"/x\")}n();n(fetch);"#,
        rule("test.incomplete-helper-negative")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
        0,
    );
    assert_count(
        r#"function n(t){t(\"/x\")}n(fetch,local);"#,
        rule("test.extra-helper-argument-negative")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn module_constructor_aliases_preserve_constructor_provenance() {
    assert_count(
        r#"var M=require("sdk").Modal;new M();"#,
        rule("test.module-constructor")
            .query(EventQuery::constructor_module("sdk", "Modal"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn derived_function_constructors_preserve_global_constructor_provenance() {
    let dynamic_function = rule("test.function-constructor")
        .query(EventQuery::constructor_global("Function"))
        .build()
        .unwrap();

    assert_count(r#"new Function("return 1")"#, dynamic_function.clone(), 1);
    assert_count(
        r#"const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;new AsyncFunction("return 1")"#,
        dynamic_function.clone(),
        1,
    );
    assert_count(
        r#"const Object={getPrototypeOf(){return {constructor: class Local {}}}};const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;new AsyncFunction()"#,
        dynamic_function,
        0,
    );
    assert_count(
        r#"function evaluate(){eval("code")}new Function("return 1");const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;new AsyncFunction("return 1")"#,
        rule("test.combined-function-constructor")
            .query(EventQuery::call_global("eval"))
            .query(EventQuery::call_global("Function"))
            .query(EventQuery::constructor_global("Function"))
            .build()
            .unwrap(),
        3,
    );
}
