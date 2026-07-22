use super::*;

#[test]
fn global_constructors_survive_transparent_callee_wrappers() {
    let url_constructor = rule("test.wrapped-global-constructor")
        .declaration(
            MatcherDecl::builder()
                .constructor_global("URL")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    assert_count(r#"new (URL)("/wrapped")"#, url_constructor.clone(), 1);
    assert_count(r#"new (0, URL)("/sequence")"#, url_constructor, 1);
}

#[test]
fn rooted_global_constructors_and_their_aliases_match_global_constructors() {
    let url_constructor = rule("test.rooted-global-constructor")
        .declaration(
            MatcherDecl::builder()
                .constructor_global("URL")
                .build()
                .expect("valid matcher declaration"),
        )
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
            .declaration(
                MatcherDecl::builder()
                    .constructor_global("Function")
                    .build()
                    .expect("valid matcher declaration"),
            )
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
            .declaration(
                MatcherDecl::builder()
                    .constructor_global("Function")
                    .build()
                    .expect("valid matcher declaration"),
            )
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn constructor_provenance_rejects_shadowed_global_roots_and_wrapped_lookalikes() {
    let url_constructor = rule("test.constructor-shadowing-negative")
        .declaration(
            MatcherDecl::builder()
                .constructor_global("URL")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .constructor_global("Function")
                .build()
                .expect("valid matcher declaration"),
        )
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
            .declaration(
                MatcherDecl::builder()
                    .class_module("sdk", "Modal")
                    .build()
                    .expect("valid matcher declaration"),
            )
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
            .declaration(
                MatcherDecl::builder()
                    .class_module("sdk", "Modal")
                    .build()
                    .expect("valid matcher declaration"),
            )
            .declaration(
                MatcherDecl::builder()
                    .constructor_module("sdk", "Modal")
                    .build()
                    .expect("valid matcher declaration"),
            )
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn constructor_global_alias() {
    assert_count(
        "new globalThis.URL('/a')",
        rule("test.debug1")
            .declaration(
                MatcherDecl::builder()
                    .constructor_global("URL")
                    .build()
                    .expect("valid matcher declaration"),
            )
            .build()
            .unwrap(),
        1,
    );
    assert_count(
        "const U=globalThis.URL;new U('/a')",
        rule("test.debug2")
            .declaration(
                MatcherDecl::builder()
                    .constructor_global("URL")
                    .build()
                    .expect("valid matcher declaration"),
            )
            .build()
            .unwrap(),
        1,
    );
}
