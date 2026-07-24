//! Browser persistent-storage rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

const WEB_STORAGE_ROOTS: &[&str] = &["localStorage", "sessionStorage"];
const WEB_STORAGE_METHODS: &[&str] = &["getItem", "setItem", "removeItem", "clear", "key"];
const DATABASE_ROOTS: &[&str] = &["indexedDB"];
const DATABASE_METHODS: &[&str] = &["open", "deleteDatabase", "databases"];
const CACHE_ROOTS: &[&str] = &["caches"];
const CACHE_METHODS: &[&str] = &["open", "match", "has", "delete", "keys"];
const STORAGE_MANAGER_ROOTS: &[&str] = &["navigator.storage"];
const STORAGE_MANAGER_METHODS: &[&str] = &["persist", "persisted", "estimate", "getDirectory"];
const DIRECTORY_METHODS: &[&str] = &[
    "getFileHandle",
    "getDirectoryHandle",
    "removeEntry",
    "entries",
];
const COOKIE_METHODS: &[&str] = &["get", "getAll", "set", "delete"];

/// Detects the listed unshadowed browser storage calls and aliases derived
/// from them: `getItem`/`setItem` on local and session storage,
/// `removeItem`, `clear`, and `key` on those stores, plus `indexedDB.open` and
/// `caches.open`. It also covers window/worker-qualified storage roots and
/// exact Cookie Store operations through the configured `window.cookieStore`
/// root. Direct property access, shadowed globals, and reassigned aliases are
/// outside this rule's scope.
pub fn rule() -> Rule {
    let mut builder = Rule::builder("browser.persistent-storage")
        .description("Uses persistent browser storage")
        .category(Category::new("browser/storage").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("document.cookie")
                .build()
                .expect("valid matcher declaration"),
        );

    for root in WEB_STORAGE_ROOTS {
        for method in WEB_STORAGE_METHODS {
            builder = builder.declaration(
                MatcherDecl::builder()
                    .member_call_rooted(format!("{root}.{method}"))
                    .build()
                    .expect("valid matcher declaration"),
            );
        }
    }
    for root in DATABASE_ROOTS {
        for method in DATABASE_METHODS {
            builder = builder.declaration(
                MatcherDecl::builder()
                    .member_call_rooted(format!("{root}.{method}"))
                    .build()
                    .expect("valid matcher declaration"),
            );
        }
    }
    for root in CACHE_ROOTS {
        for method in CACHE_METHODS {
            builder = builder.declaration(
                MatcherDecl::builder()
                    .member_call_rooted(format!("{root}.{method}"))
                    .build()
                    .expect("valid matcher declaration"),
            );
        }
    }
    for root in STORAGE_MANAGER_ROOTS {
        for method in STORAGE_MANAGER_METHODS {
            let path = format!("{root}.{method}");
            builder = builder.declaration(
                MatcherDecl::builder()
                    .member_call_rooted(path.clone())
                    .build()
                    .expect("valid matcher declaration"),
            );
            if *method == "getDirectory" {
                for member in DIRECTORY_METHODS {
                    builder = builder.declaration(
                        MatcherDecl::builder()
                            .member_call_returned(&path, *member)
                            .build()
                            .expect("valid matcher declaration"),
                    );
                }
            }
        }
    }
    for method in COOKIE_METHODS {
        builder = builder.declaration(
            MatcherDecl::builder()
                .member_call_rooted(format!("cookieStore.{method}"))
                .build()
                .expect("valid matcher declaration"),
        );
    }

    builder.build().unwrap()
}
