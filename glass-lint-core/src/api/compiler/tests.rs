#[cfg(test)]
mod normalize;
#[cfg(test)]
mod physical;
#[cfg(test)]
mod reference;
#[cfg(test)]
mod rule;
#[cfg(test)]
mod validate;

#[test]
fn compiler_invariants_do_not_become_authored_query_diagnostics() {
    let internal = super::validate::QueryCompileError::InternalInvariant {
        detail: "normalized slots are not dense".into(),
    };
    assert!(matches!(
        super::map_query_compile_error(internal),
        super::MatcherBuildError::CompilerInvariant(
            crate::api::rule::CompilerInvariantDiagnostic::Internal { detail },
        ) if detail == "normalized slots are not dense"
    ));

    let authored = super::validate::QueryCompileError::MissingBinding {
        primary_var: crate::api::rule::query::VarId::new(0),
    };
    assert!(matches!(
        super::map_query_compile_error(authored),
        super::MatcherBuildError::QueryCompileError(diagnostic)
            if diagnostic.code() == "missing_binding"
    ));
}

#[test]
fn compiled_and_authored_identity_emptiness_policies_agree() {
    use glass_lint_datastructures::SymbolPath;

    use crate::api::{
        compiler::IdentityConstraint,
        rule::{ModuleSpecifierPattern, query::IdentitySpec},
    };

    let identities = [
        IdentitySpec::Global { name: "  ".into() },
        IdentitySpec::Heuristic {
            name: "fetch".into(),
        },
        IdentitySpec::ModuleExport {
            module: "module".into(),
            export: "  ".into(),
        },
        IdentitySpec::PackageModuleExport {
            module: ModuleSpecifierPattern::package("pkg").unwrap(),
            export: "export".into(),
        },
        IdentitySpec::ModuleNamespace {
            module: "module".into(),
        },
        IdentitySpec::PackageModuleNamespace {
            module: ModuleSpecifierPattern::package("pkg").unwrap(),
        },
        IdentitySpec::Rooted {
            path: SymbolPath::from("global.object"),
        },
        IdentitySpec::LiteralString {
            predicate: "value".into(),
        },
        IdentitySpec::PackageSpecifier {
            pattern: ModuleSpecifierPattern::package("pkg").unwrap(),
        },
        IdentitySpec::PrivateNetworkAddress,
    ];

    for identity in identities {
        assert_eq!(
            IdentityConstraint::from(&identity).is_empty(),
            super::validate::is_identity_empty(&identity),
            "emptiness policies diverged for {identity:?}",
        );
    }
}
