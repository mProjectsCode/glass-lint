//! Logical query composition constructors.

use crate::api::rule::query::{
    AllExpr, AnyExpr, EmissionDecl, EventQuery, EventRequirement, EventSpec, IdentitySpec,
    MatchKind, MemberChain, QueryBuildError, QueryDecl, QueryExpr, QueryPredicate, VarId,
    checked_module_export, explain_expression, lifecycle::IntoLifecycleQuery, limits,
};

impl QueryDecl {
    pub fn expression(&self) -> &QueryExpr {
        &self.expression
    }

    pub fn emission(&self) -> &EmissionDecl {
        &self.emission
    }

    /// Generate a human-readable explanation of the declared matcher.
    ///
    /// This is derived from the validated query structure and is intended for
    /// catalogs, documentation, and diagnostics rather than query execution.
    pub fn explanation(&self) -> String {
        format!(
            "Emit `{}` when {}.",
            self.emission.symbol,
            explain_expression(&self.expression)
        )
    }

    /// Member call on an instance created by a module export.
    ///
    /// The semantic matcher follows proven receiver aliases and extracted
    /// callable aliases. Shadowing, reassignment, ambiguity, and unsupported
    /// receiver identities remain fail-closed.
    pub fn member_call_instance(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let (module_str, export_str) = checked_module_export(module, export)?;
        let member_path = MemberChain::parse(member)?.into_path();
        let symbol = format!("{module_str}.{export_str}");
        let identity = IdentitySpec::ModuleExport {
            module: module_str,
            export: export_str,
        };
        member_subject_query(
            EventSpec::MemberCall {
                member: member_path,
            },
            identity,
            MemberObjectBinding::Constructed,
            symbol,
        )
    }

    /// Member call on an object returned by a rooted source.
    pub fn member_call_returned(
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let source_str: String = source.into();
        let source_chain = MemberChain::parse(source_str)?;
        let member_chain = MemberChain::parse(member)?;
        let source_path = source_chain.path().clone();
        let member_path = member_chain.path().clone();
        let identity = IdentitySpec::Rooted { path: source_path };
        member_subject_query(
            EventSpec::MemberCall {
                member: member_path,
            },
            identity,
            MemberObjectBinding::Returned,
            source_chain.as_str(),
        )
    }

    /// Member read on an object returned by a rooted source.
    pub fn member_read_returned(
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let source_str: String = source.into();
        let source_chain = MemberChain::parse(source_str)?;
        let member_chain = MemberChain::parse(member)?;
        let source_path = source_chain.path().clone();
        let member_path = member_chain.path().clone();
        let identity = IdentitySpec::Rooted { path: source_path };
        member_subject_query(
            EventSpec::MemberRead {
                member: member_path,
            },
            identity,
            MemberObjectBinding::Returned,
            source_chain.as_str(),
        )
    }

    // ── Evidence override ─────────────────────────────────────────

    /// Override the evidence kind and symbol.
    #[cfg(test)]
    pub(crate) fn with_evidence(mut self, kind: MatchKind, symbol: impl Into<String>) -> Self {
        self.emission.kind = kind;
        self.emission.symbol = symbol.into();
        self
    }

    /// Construct an `Any` expression from an iterable of fallible query
    /// declarations.
    ///
    /// Each branch is a [`QueryDecl`] or
    /// `Result<QueryDecl, QueryBuildError>`. Returns
    /// [`QueryBuildError::EmptyAlternatives`] if the iterator yields no
    /// branches. Branch scopes are independent: the same variable name may
    /// be bound in different branches with compatible types.
    ///
    /// # Example
    ///
    /// ```ignore
    /// QueryDecl::any_with_evidence([
    ///     EventQuery::call_global("fetch").map(EventQuery::into_query),
    ///     EventQuery::call_global("navigate").map(EventQuery::into_query),
    /// ], "network.request")?;
    /// ```
    pub fn any(
        branches: impl IntoIterator<Item = Result<Self, QueryBuildError>>,
    ) -> Result<Self, QueryBuildError> {
        Self::any_impl(branches, None)
    }

    /// Construct alternatives with an explicit aggregate evidence symbol.
    ///
    /// Use this when branches intentionally select different identities but
    /// should be reported under one caller-chosen symbol.
    pub fn any_with_evidence(
        branches: impl IntoIterator<Item = Result<Self, QueryBuildError>>,
        symbol: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let symbol = symbol.into();
        if symbol.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Self::any_impl(branches, Some(symbol))
    }

    fn any_impl(
        branches: impl IntoIterator<Item = Result<Self, QueryBuildError>>,
        explicit_symbol: Option<String>,
    ) -> Result<Self, QueryBuildError> {
        let mut exprs = Vec::new();
        let mut first_emission: Option<EmissionDecl> = None;
        for branch in branches {
            if exprs.len() >= limits::MAX_EXPR_CHILDREN {
                return Err(QueryBuildError::CollectionTooLarge(
                    "any expression branches",
                    exprs.len() + 1,
                ));
            }
            let decl = branch?;
            if let Some(first) = &first_emission {
                let compatible = if explicit_symbol.is_some() {
                    first.is_compatible_with_aggregate_symbol(&decl.emission)
                } else {
                    first.is_compatible(&decl.emission)
                };
                if !compatible {
                    return Err(QueryBuildError::EvidenceProjection);
                }
            } else {
                first_emission = Some(decl.emission.clone());
            }
            exprs.push(decl.expression);
        }
        let Some(mut first) = first_emission else {
            return Err(QueryBuildError::EmptyAlternatives);
        };
        let any = AnyExpr::new(exprs)?;
        if !any.all_branches_contain(first.primary_var) {
            return Err(QueryBuildError::EvidenceProjection);
        }
        if let Some(symbol) = explicit_symbol {
            first.symbol = symbol;
        }
        Ok(Self {
            expression: QueryExpr::any(any),
            emission: first,
        })
    }

    /// Construct a same-event `All` expression from one event selection
    /// and zero or more [`EventRequirement`] constraints.
    ///
    /// The result is an `All` with the event selection and requirement atoms
    /// as branches. Uncorrelated multi-event conjunctions are rejected
    /// during validation.
    ///
    /// # Example
    ///
    /// ```ignore
    /// QueryDecl::all(
    ///     EventQuery::call_global("fetch"),
    ///     [EventRequirement::argument(0, ValueMatcher::static_string())?],
    /// )?;
    /// ```
    pub fn all(
        event: Result<EventQuery, QueryBuildError>,
        requirements: impl IntoIterator<Item = Result<EventRequirement, QueryBuildError>>,
    ) -> Result<Self, QueryBuildError> {
        let selection = event?.into_selection_assembly();
        let var = selection.var();
        let emission = selection.emission().clone();
        let mut branches = selection.branches();

        for req_result in requirements {
            if branches.len() >= limits::MAX_EXPR_CHILDREN {
                return Err(QueryBuildError::CollectionTooLarge(
                    "all expression branches",
                    branches.len() + 1,
                ));
            }
            let req = req_result?;
            if !selection.event.event().supports_arguments() {
                return Err(QueryBuildError::ArgumentsRequireCallEvent);
            }
            let EventRequirement { index, matcher } = req;
            branches.push(QueryExpr::require(QueryPredicate::Argument {
                call: var,
                index,
                matcher,
            }));
        }

        let expression = QueryExpr::all(AllExpr::new(branches)?);
        Ok(Self {
            expression,
            emission,
        })
    }

    /// Wrap a [`LifecycleQuery`] into a [`QueryDecl`] with inferred evidence.
    /// Accepts a [`LifecycleQuery`] or a `Result` from a builder for direct
    /// use in the rule builder's `query` method.
    pub fn lifecycle(lc: impl IntoLifecycleQuery) -> Result<Self, QueryBuildError> {
        let lc = lc.into_lifecycle_query()?;
        let symbol = lc.symbol.clone();
        debug_assert_ne!(symbol.trim(), "");
        Ok(Self {
            expression: QueryExpr::lifecycle(lc),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::CallArgument,
                symbol,
            },
        })
    }
}

#[derive(Clone, Copy)]
enum MemberObjectBinding {
    Constructed,
    Returned,
}

fn member_subject_query(
    event: EventSpec,
    identity: IdentitySpec,
    object_binding: MemberObjectBinding,
    symbol: impl Into<String>,
) -> Result<QueryDecl, QueryBuildError> {
    let selection = EventQuery::from_parts(event, identity.clone()).into_selection_assembly();
    let event_var = selection.var();
    let object_var = VarId::new(1);
    let mut emission = selection.emission().clone();
    let mut branches = selection.branches();
    let object_binding = match object_binding {
        MemberObjectBinding::Constructed => QueryPredicate::ConstructedObject {
            bind: object_var,
            identity,
        },
        MemberObjectBinding::Returned => QueryPredicate::ReturnedObject {
            bind: object_var,
            identity,
        },
    };
    branches.push(QueryExpr::require(object_binding));
    branches.push(QueryExpr::require(QueryPredicate::MemberSubject {
        event: event_var,
        object: object_var,
    }));
    emission.symbol = symbol.into();
    Ok(QueryDecl {
        expression: QueryExpr::all(AllExpr::new(branches)?),
        emission,
    })
}
