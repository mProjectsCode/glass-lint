use super::{
    ApiClassificationResult, ApiRule, ApiSeverity, Confidence, classify_api_usage,
    rule::ApiRuleBuilder,
};

fn rule(id: &str) -> ApiRuleBuilder {
    ApiRule::builder(id)
        .label(id)
        .category("test")
        .severity(ApiSeverity::Info)
        .confidence(Confidence::High)
}

fn classify(source: &str, rules: &[ApiRule]) -> ApiClassificationResult {
    let parsed = crate::parse(source, "input.js").unwrap();
    classify_api_usage(Some(&parsed.program), rules)
}

fn evidence_count(result: &ApiClassificationResult, id: &str) -> u32 {
    result
        .capabilities()
        .iter()
        .find(|capability| capability.id() == id)
        .map(|capability| {
            capability
                .evidence()
                .iter()
                .map(|evidence| evidence.count())
                .sum()
        })
        .unwrap_or(0)
}

fn assert_count(source: &str, rule: ApiRule, expected: u32) {
    let id = rule.id.clone();
    let rules = [rule];
    let result = classify(source, &rules);
    assert_eq!(evidence_count(&result, &id), expected, "{source}");
}

#[test]
fn minified_commonjs_namespace_export_aliases_preserve_module_calls() {
    assert_count(
        r#"var r=require("sdk"),s=r.send;s();"#,
        rule("test.module")
            .module_calls("sdk", ["send"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_commonjs_interop_namespace_calls_preserve_module_members() {
    assert_count(
        r#"var e=__toESM(require("sdk"));e.send();"#,
        rule("test.module-member")
            .module_member_calls("sdk", ["send"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_assignment_expression_aliases_preserve_module_exports() {
    assert_count(
        r#"var s;(s=require("sdk").send)();"#,
        rule("test.assignment-module")
            .module_calls("sdk", ["send"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_module_provenance_rejects_local_require_and_wrapper_lookalikes() {
    assert_count(
        r#"
        function require(){return {send(){}}}
        function __toESM(x){return {send:x.send}}
        var e=__toESM(require("sdk")),send=function(){};
        e.send();send();
        "#,
        rule("test.module-negative")
            .module_calls("sdk", ["send"])
            .module_member_calls("sdk", ["send"])
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn minified_rooted_member_aliases_follow_one_letter_bindings() {
    assert_count(
        r#"var v=host.files;v.read();"#,
        rule("test.rooted")
            .rooted_member_calls(["host.files.read"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_nested_rooted_aliases_follow_cached_subobjects() {
    assert_count(
        r#"var a=host,b=a.files,c=b;c.read();"#,
        rule("test.nested-rooted")
            .rooted_member_calls(["host.files.read"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_this_root_aliases_canonicalize_to_rooted_members() {
    assert_count(
        r#"var a=this.app.files;a.read();"#,
        rule("test.this-root")
            .rooted_member_calls(["app.files.read"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_reassignment_order_keeps_only_pre_reassignment_rooted_calls() {
    assert_count(
        r#"var v=host.files;v.read();v=local.files;v.read();"#,
        rule("test.reassignment")
            .rooted_member_calls(["host.files.read"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_sibling_scope_reuse_does_not_leak_rooted_aliases() {
    assert_count(
        r#"
        function a(){var x=host.files;x.read()}
        function b(){var x=local.files;x.read()}
        "#,
        rule("test.scope-reuse")
            .rooted_member_calls(["host.files.read"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_literal_computed_member_chains_are_rooted() {
    assert_count(
        r#"host["files"]["read"]();"#,
        rule("test.literal-computed")
            .rooted_member_calls(["host.files.read"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_concatenated_static_property_names_are_rooted() {
    assert_count(
        r#"window["fet"+"ch"]("/x");"#,
        rule("test.concatenated-computed")
            .rooted_member_calls(["window.fetch"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_constant_property_aliases_are_rooted() {
    assert_count(
        r#"const k="read";host.files[k]();"#,
        rule("test.constant-computed")
            .rooted_member_calls(["host.files.read"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_static_string_table_property_aliases_are_rooted() {
    assert_count(
        r#"const k=["read"];host.files[k[0]]();"#,
        rule("test.string-table-computed")
            .rooted_member_calls(["host.files.read"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_dynamic_computed_properties_do_not_match_rooted_members() {
    assert_count(
        r#"var k=Date.now()>0?"read":"write";host.files[k]();"#,
        rule("test.dynamic-computed-negative")
            .rooted_member_calls(["host.files.read"])
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn minified_sequence_global_calls_preserve_global_provenance() {
    assert_count(
        r#"(0,fetch)("/x");"#,
        rule("test.sequence-global")
            .global_calls(["fetch"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_bound_global_calls_preserve_global_provenance() {
    assert_count(
        r#"var f=fetch.bind(null);f("/x");"#,
        rule("test.bound-global")
            .global_calls(["fetch"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_call_and_apply_preserve_global_provenance_when_receiver_is_static() {
    assert_count(
        r#"var f=fetch;f.call(null,"/x");f.apply(null,["/y"]);"#,
        rule("test.call-apply-global")
            .global_calls(["fetch"])
            .build()
            .unwrap(),
        2,
    );
}

#[test]
fn minified_optional_chained_aliases_preserve_rooted_member_arguments() {
    assert_count(
        r#"var c=app.commands;c?.execute?.("open");"#,
        rule("test.optional")
            .rooted_member_call("app.commands.execute")
            .arg_string(0, ["open"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_shadowed_globals_do_not_match_global_calls() {
    assert_count(
        r#"function a(fetch){fetch("/local")}a(function(){});"#,
        rule("test.shadowed-global-negative")
            .global_calls(["fetch"])
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn minified_static_string_arguments_follow_aliases_but_reject_dynamic_strings() {
    assert_count(
        r#"var f=fetch,u="/x";f(u);f("/"+name);"#,
        rule("test.static-string-arg")
            .global_call("fetch")
            .static_string_call_arg(0)
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_static_object_arguments_are_reused_for_key_matching() {
    assert_count(
        r#"var o={url:"/x",method:"GET"};client.request(o);"#,
        rule("test.object-arg")
            .rooted_member_call("client.request")
            .arg_object_keys(0, ["url", "method"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_sequence_object_arguments_are_reused_for_key_matching() {
    assert_count(
        r#"var o;(o={url:"/x",method:"GET"},client.request(o));"#,
        rule("test.sequence-object-arg")
            .rooted_member_call("client.request")
            .arg_object_keys(0, ["url", "method"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_rooted_expression_arguments_follow_one_letter_aliases() {
    assert_count(
        r#"var f=vault.file,o=app;o.open(f);"#,
        rule("test.rooted-arg")
            .rooted_member_call("app.open")
            .arg_rooted_exprs(0, ["vault.file"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_spread_object_arguments_do_not_satisfy_exact_key_matching() {
    assert_count(
        r#"var b={url:"/x"};client.request({...b,method:"GET"});"#,
        rule("test.spread-object-negative")
            .rooted_member_call("client.request")
            .arg_object_keys(0, ["url", "method"])
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn minified_named_helper_parameter_aliases_preserve_global_calls() {
    assert_count(
        r#"function n(t){t("/x")}n(fetch);"#,
        rule("test.named-helper")
            .global_calls(["fetch"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_arrow_helper_parameter_aliases_preserve_global_calls() {
    assert_count(
        r#"var n=t=>t("/x");n(fetch);"#,
        rule("test.arrow-helper")
            .global_calls(["fetch"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_helper_argument_objects_flow_to_member_call_key_matching() {
    assert_count(
        r#"function n(x){client.request(x)}n({url:"/x",method:"GET"});"#,
        rule("test.helper-object-flow")
            .rooted_member_call("client.request")
            .arg_object_keys(0, ["url", "method"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_inconsistent_helper_calls_do_not_infer_parameter_aliases() {
    assert_count(
        r#"function n(t){t("/x")}n(fetch);n(localFetch);"#,
        rule("test.inconsistent-helper-negative")
            .global_calls(["fetch"])
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn minified_module_constructor_aliases_preserve_constructor_provenance() {
    assert_count(
        r#"var M=require("sdk").Modal;new M();"#,
        rule("test.module-constructor")
            .constructors(["sdk.Modal"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn minified_module_class_references_preserve_class_provenance() {
    assert_count(
        r#"var s=require("sdk");class X extends s.Modal{};x instanceof s.Modal;"#,
        rule("test.module-class")
            .classes(["sdk.Modal"])
            .build()
            .unwrap(),
        2,
    );
}

#[test]
fn minified_local_class_lookalikes_do_not_match_module_class_or_constructor() {
    assert_count(
        r#"class Modal{};new Modal();x instanceof Modal;"#,
        rule("test.local-class-negative")
            .classes(["sdk.Modal"])
            .constructors(["sdk.Modal"])
            .build()
            .unwrap(),
        0,
    );
}
