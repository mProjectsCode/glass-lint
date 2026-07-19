use super::*;

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

#[test]
fn debug_urlalias_global_constructor() {
    assert_count(
        "new globalThis.URL('/a')",
        rule("test.debug1")
            .matcher(Matcher::global_constructor("URL"))
            .build()
            .unwrap(),
        1,
    );
    assert_count(
        "const U=globalThis.URL;new U('/a')",
        rule("test.debug2")
            .matcher(Matcher::global_constructor("URL"))
            .build()
            .unwrap(),
        1,
    );
}
