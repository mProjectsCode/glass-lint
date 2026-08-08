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
