//! Matcher coverage for compact and bundled JavaScript.
//!
//! These tests exercise the public linting API so matcher behavior is verified
//! exactly as provider crates consume it.

use glass_lint_core::{
    Linter, RuleCatalog,
    rules::{
        BuildError, Builder, CallMatcher, Confidence, Matcher, MemberCallMatcher, Rule, Severity,
    },
};

fn rule(id: &str) -> Builder {
    Rule::builder(id)
        .label(id)
        .category("test")
        .severity(Severity::Info)
        .confidence(Confidence::High)
}

fn assert_count(source: &str, rule: Rule, expected: usize) {
    let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
    let count = Linter::new(catalog)
        .lint(source, "minified.js")
        .findings
        .len();
    assert_eq!(count, expected, "{source}");
}

#[test]
fn commonjs_namespace_export_aliases_preserve_module_calls() {
    assert_count(
        r#"var r=require("sdk"),s=r.send;s();"#,
        rule("test.module")
            .matcher(Matcher::module_call("sdk", "send"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn commonjs_interop_namespace_calls_preserve_module_members() {
    assert_count(
        r#"var e=__toESM(require("sdk"));e.send();"#,
        rule("test.module-member")
            .matcher(Matcher::module_member_call("sdk", "send"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn assignment_expression_aliases_preserve_module_exports() {
    assert_count(
        r#"var s;(s=require("sdk").send)();"#,
        rule("test.assignment-module")
            .matcher(Matcher::module_call("sdk", "send"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn module_provenance_rejects_local_require_and_wrapper_lookalikes() {
    assert_count(
        r#"
        function require(){return {send(){}}}
        function __toESM(x){return {send:x.send}}
        var e=__toESM(require("sdk")),send=function(){};
        e.send();send();
        "#,
        rule("test.module-negative")
            .matcher(Matcher::module_call("sdk", "send"))
            .matcher(Matcher::module_member_call("sdk", "send"))
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn rooted_member_aliases_follow_one_letter_bindings() {
    assert_count(
        r#"var v=host.files;v.read();"#,
        rule("test.rooted")
            .matcher(Matcher::rooted_member_call("host.files.read"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn nested_rooted_aliases_follow_cached_subobjects() {
    assert_count(
        r#"var a=host,b=a.files,c=b;c.read();"#,
        rule("test.nested-rooted")
            .matcher(Matcher::rooted_member_call("host.files.read"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn this_root_aliases_canonicalize_to_rooted_members() {
    assert_count(
        r#"var a=this.app.files;a.read();"#,
        rule("test.this-root")
            .matcher(Matcher::rooted_member_call("app.files.read"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn returned_objects_follow_direct_calls_aliases_and_reassignment() {
    let matcher = Matcher::returned_member_call("app.workspace.getLeaf", "openFile");
    assert_count(
        r#"
        app.workspace.getLeaf().openFile(file);
        const leaf = app.workspace.getLeaf();
        const alias = leaf;
        alias.openFile(file);
        let changed = app.workspace.getLeaf();
        changed = localLeaf;
        changed.openFile(file);
        function local(app) { app.workspace.getLeaf().openFile(file); }
        localWorkspace.getLeaf().openFile(file);
        "#,
        rule("test.returned").matcher(matcher).build().unwrap(),
        2,
    );
}

#[test]
fn returned_object_reads_are_provenance_aware() {
    assert_count(
        r#"
        const plugin = app.plugins.getPlugin("calendar");
        plugin.manifest;
        const local = { manifest: {} };
        local.manifest;
        "#,
        rule("test.returned-read")
            .matcher(Matcher::returned_member_read(
                "app.plugins.getPlugin",
                "manifest",
            ))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn inline_commonjs_members_share_module_provenance() {
    assert_count(
        r#"
        require("electron").shell.openExternal(url);
        require("electron")["shell"].openPath(path);
        require(name).shell.openExternal(url);
        require("electrons").shell.openExternal(url);
        function f(require) { require("electron").shell.openExternal(url); }
        "#,
        rule("test.inline")
            .matcher(Matcher::module_member_call(
                "electron",
                "shell.openExternal",
            ))
            .matcher(Matcher::module_member_call("electron", "shell.openPath"))
            .build()
            .unwrap(),
        2,
    );
}

#[test]
fn instance_matchers_require_proven_module_subclasses() {
    assert_count(
        r#"
        import { Base as Renamed } from "framework";
        class Child extends Renamed {
            run() { this.registerThing(); const self = this; self.registerThing();
                (() => this.registerThing())(); function nested() { this.registerThing(); } }
        }
        const framework = require("framework");
        class CommonChild extends framework.Base { run() { this.registerThing(); } }
        class Lookalike extends Base { run() { this.registerThing(); } }
        function unrelated() { this.registerThing(); }
        "#,
        rule("test.instance")
            .matcher(Matcher::instance_member_call(
                "framework",
                "Base",
                "registerThing",
            ))
            .build()
            .unwrap(),
        4,
    );
}

#[test]
fn instance_matchers_respect_alias_scope_and_static_methods() {
    assert_count(
        r#"
        import { Base } from "framework";
        class Child extends Base {
            run() {
                const self = this;
                self.registerThing();
                { const self = local; self.registerThing(); }
                self.registerThing();
            }
            static configure() { this.registerThing(); }
        }
        "#,
        rule("test.instance-scope")
            .matcher(Matcher::instance_member_call(
                "framework",
                "Base",
                "registerThing",
            ))
            .build()
            .unwrap(),
        2,
    );
}

#[test]
fn new_semantic_matchers_are_normalized_and_validated() {
    assert_count(
        r#"const value = app.workspace["getLeaf"](); value.openFile(file);"#,
        rule("test.normalized-return")
            .matcher(Matcher::returned_member_call(
                " app.workspace.getLeaf ",
                " openFile ",
            ))
            .build()
            .unwrap(),
        1,
    );

    let invalid = rule("test.invalid-semantic")
        .matcher(Matcher::returned_member_call(" ", " "))
        .matcher(Matcher::returned_member_read(" ", "manifest"))
        .matcher(Matcher::instance_member_call("framework", " ", "run"))
        .build();
    assert_eq!(invalid.unwrap_err(), BuildError::MissingMatcher);
}

#[test]
fn ordinary_member_argument_predicates_reuse_static_values() {
    assert_count(
        r#"
        app.vault.on("delete", handler);
        app.vault.on(`rename`, handler);
        app.vault.on(eventName, handler);
        app.vault.on("unrelated", handler);
        "#,
        rule("test.event")
            .matcher(MemberCallMatcher::rooted_chain("app.vault.on").arg_value(
                0,
                glass_lint_core::rules::FlowValueMatcher::StaticExact(vec![
                    "delete".into(),
                    "rename".into(),
                ]),
            ))
            .build()
            .unwrap(),
        2,
    );
}

#[test]
fn reassignment_order_keeps_only_pre_reassignment_rooted_calls() {
    assert_count(
        r#"var v=host.files;v.read();v=local.files;v.read();"#,
        rule("test.reassignment")
            .matcher(Matcher::rooted_member_call("host.files.read"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn sibling_scope_reuse_does_not_leak_rooted_aliases() {
    assert_count(
        r#"
        function a(){var x=host.files;x.read()}
        function b(){var x=local.files;x.read()}
        "#,
        rule("test.scope-reuse")
            .matcher(Matcher::rooted_member_call("host.files.read"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn literal_computed_member_chains_are_rooted() {
    assert_count(
        r#"host["files"]["read"]();"#,
        rule("test.literal-computed")
            .matcher(Matcher::rooted_member_call("host.files.read"))
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
            .matcher(Matcher::rooted_member_call("window.fetch"))
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
            .matcher(Matcher::rooted_member_call("host.files.read"))
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
            .matcher(Matcher::rooted_member_call("host.files.read"))
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
            .matcher(Matcher::rooted_member_call("host.files.read"))
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
            .matcher(Matcher::global_call("fetch"))
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
            .matcher(Matcher::global_call("fetch"))
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
            .matcher(Matcher::global_call("fetch"))
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
            .matcher(
                MemberCallMatcher::rooted_chain("app.commands.execute").arg_string(0, ["open"]),
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
            .matcher(Matcher::global_call("fetch"))
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
            .matcher(CallMatcher::global("fetch").static_string_arg(0))
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
            .matcher(
                MemberCallMatcher::rooted_chain("client.request")
                    .arg_object_keys(0, ["url", "method"]),
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
            .matcher(
                MemberCallMatcher::rooted_chain("client.request")
                    .arg_object_keys(0, ["url", "method"]),
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
            .matcher(
                MemberCallMatcher::rooted_chain("app.open").arg_rooted_exprs(0, ["vault.file"]),
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
            .matcher(
                MemberCallMatcher::rooted_chain("client.request")
                    .arg_object_keys(0, ["url", "method"]),
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
            .matcher(Matcher::global_call("fetch"))
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
            .matcher(Matcher::global_call("fetch"))
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
            .matcher(
                MemberCallMatcher::rooted_chain("client.request")
                    .arg_object_keys(0, ["url", "method"]),
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
            .matcher(Matcher::global_call("fetch"))
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
            .matcher(Matcher::module_constructor("sdk", "Modal"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn derived_function_constructors_preserve_global_constructor_provenance() {
    let dynamic_function = rule("test.function-constructor")
        .matcher(Matcher::global_constructor("Function"))
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
            .matcher(Matcher::global_call("eval"))
            .matcher(Matcher::global_call("Function"))
            .matcher(Matcher::global_constructor("Function"))
            .build()
            .unwrap(),
        3,
    );
}

#[test]
fn global_constructors_survive_transparent_callee_wrappers() {
    let url_constructor = rule("test.wrapped-global-constructor")
        .matcher(Matcher::global_constructor("URL"))
        .build()
        .unwrap();

    assert_count(r#"new (URL)("/wrapped")"#, url_constructor.clone(), 1);
    assert_count(r#"new (0, URL)("/sequence")"#, url_constructor, 1);
}

#[test]
fn rooted_global_constructors_and_their_aliases_match_global_constructors() {
    let url_constructor = rule("test.rooted-global-constructor")
        .matcher(Matcher::global_constructor("URL"))
        .build()
        .unwrap();

    assert_count(
        r#"new globalThis.URL("/rooted")"#,
        url_constructor.clone(),
        1,
    );
    assert_count(
        r#"const URLAlias=globalThis.URL;new URLAlias("/aliased")"#,
        url_constructor,
        1,
    );
}

#[test]
fn destructured_derived_function_constructors_preserve_provenance() {
    assert_count(
        r#"const {constructor:AsyncFunction}=Object.getPrototypeOf(async function(){});new AsyncFunction("return 1")"#,
        rule("test.destructured-function-constructor")
            .matcher(Matcher::global_constructor("Function"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn reflect_derived_function_constructors_preserve_provenance() {
    assert_count(
        r#"const AsyncFunction=Reflect.getPrototypeOf(async function(){}).constructor;new AsyncFunction("return 1")"#,
        rule("test.reflect-function-constructor")
            .matcher(Matcher::global_constructor("Function"))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn constructor_provenance_rejects_shadowed_global_roots_and_wrapped_lookalikes() {
    let url_constructor = rule("test.constructor-shadowing-negative")
        .matcher(Matcher::global_constructor("URL"))
        .matcher(Matcher::global_constructor("Function"))
        .build()
        .unwrap();

    assert_count(
        r#"const URL=class Local{};new (0,URL)();"#,
        url_constructor.clone(),
        0,
    );
    assert_count(
        r#"const globalThis={URL:class Local{}};const Alias=globalThis.URL;new Alias();"#,
        url_constructor.clone(),
        0,
    );
    assert_count(
        r#"const Reflect={getPrototypeOf(){return {constructor:class Local{}}}};const C=Reflect.getPrototypeOf(async()=>{}).constructor;new C();"#,
        url_constructor.clone(),
        0,
    );
    assert_count(
        r#"const Object={getPrototypeOf(){return {constructor:class Local{}}}};const {constructor:C}=Object.getPrototypeOf(async()=>{});new C();"#,
        url_constructor,
        0,
    );
}

#[test]
fn module_class_references_preserve_class_provenance() {
    assert_count(
        r#"var s=require("sdk");class X extends s.Modal{};x instanceof s.Modal;"#,
        rule("test.module-class")
            .matcher(Matcher::module_class("sdk", "Modal"))
            .build()
            .unwrap(),
        2,
    );
}

#[test]
fn local_class_lookalikes_do_not_match_module_class_or_constructor() {
    assert_count(
        r#"class Modal{};new Modal();x instanceof Modal;"#,
        rule("test.local-class-negative")
            .matcher(Matcher::module_class("sdk", "Modal"))
            .matcher(Matcher::module_constructor("sdk", "Modal"))
            .build()
            .unwrap(),
        0,
    );
}
