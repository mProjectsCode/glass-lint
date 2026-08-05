//! Logical query composition constructors.

use super::{
    AllExpr, AnyExpr, EmissionDecl, EventQuery, EventRequirement, EventRequirementKind, EventSpec,
    IdentitySpec, LifecycleQuery, MatchKind, QueryBuildError, QueryDecl, QueryExpr, QueryPredicate,
    VarId, checked_chain, checked_module_export, evidence_kind_for_event, explain_expression,
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
    pub fn member_call_instance(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let (module_str, export_str) = checked_module_export(module, export)?;
        let member_path = checked_chain(member)?.into_path();
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
            MatchKind::MemberCall,
            symbol,
        )
    }

    /// Member call on an object returned by a rooted source.
    pub fn member_call_returned(
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let source_str: String = source.into();
        let source_chain = checked_chain(source_str)?;
        let member_chain = checked_chain(member)?;
        let source_path = source_chain.path().clone();
        let member_path = member_chain.path().clone();
        let identity = IdentitySpec::Rooted { path: source_path };
        member_subject_query(
            EventSpec::MemberCall {
                member: member_path,
            },
            identity,
            MemberObjectBinding::Returned,
            MatchKind::MemberCall,
            source_chain.as_str(),
        )
    }

    /// Member read on an object returned by a rooted source.
    pub fn member_read_returned(
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let source_str: String = source.into();
        let source_chain = checked_chain(source_str)?;
        let member_chain = checked_chain(member)?;
        let source_path = source_chain.path().clone();
        let member_path = member_chain.path().clone();
        let identity = IdentitySpec::Rooted { path: source_path };
        member_subject_query(
            EventSpec::MemberRead {
                member: member_path,
            },
            identity,
            MemberObjectBinding::Returned,
            MatchKind::MemberRead,
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
            let decl = branch?;
            if let Some(first) = &first_emission {
                let primary_present = decl.expression.contains_var(first.primary_var);
                if !primary_present
                    || decl.emission.primary_var != first.primary_var
                    || decl.emission.kind != first.kind
                    || (explicit_symbol.is_none() && decl.emission.symbol != first.symbol)
                {
                    return Err(QueryBuildError::EvidenceProjection);
                }
            } else {
                first_emission = Some(decl.emission.clone());
            }
            exprs.push(decl.expression);
        }
        if exprs.is_empty() {
            return Err(QueryBuildError::EmptyAlternatives);
        }
        let mut first = first_emission.unwrap_or_else(Self::default_emission);
        if let Some(symbol) = explicit_symbol {
            first.symbol = symbol;
        }
        Ok(Self {
            expression: QueryExpr::any(AnyExpr::new(exprs)?),
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
        let eq = event?;
        let var = eq.var;
        let kind = evidence_kind_for_event(&eq.event);
        let symbol = eq.identity.display_name();

        // Build All branches: SelectEvent + EventKind + EventIdentity +
        // argument constraints as Require atoms.
        let event_spec = eq.event;
        let identity_spec = eq.identity;
        let mut branches: Vec<QueryExpr> = vec![
            QueryExpr::select_event(var),
            QueryExpr::require(QueryPredicate::EventKind {
                event: var,
                expected: event_spec,
            }),
            QueryExpr::require(QueryPredicate::EventIdentity {
                event: var,
                expected: identity_spec,
            }),
        ];

        for req_result in requirements {
            let req = req_result?;
            match req.kind {
                EventRequirementKind::Argument { index, matcher } => {
                    branches.push(QueryExpr::require(QueryPredicate::Argument {
                        call: var,
                        index,
                        matcher,
                    }));
                }
            }
        }

        let expression = QueryExpr::all(AllExpr::new(branches)?);
        Ok(Self {
            expression,
            emission: EmissionDecl {
                primary_var: var,
                kind,
                symbol,
            },
        })
    }

    /// Default emission for placeholder use.
    fn default_emission() -> EmissionDecl {
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: String::new(),
        }
    }

    /// Wrap a [`LifecycleQuery`] into a [`QueryDecl`] with inferred evidence.
    /// Accepts a `Result` from a builder for direct use in
    /// the rule builder's `query` method.
    pub fn lifecycle(
        lc_result: Result<LifecycleQuery, QueryBuildError>,
    ) -> Result<Self, QueryBuildError> {
        lc_result.map(|lc| {
            let symbol = lc.symbol.clone();
            debug_assert!(!symbol.trim().is_empty());
            Self {
                expression: QueryExpr::lifecycle(lc),
                emission: EmissionDecl {
                    primary_var: VarId::new(0),
                    kind: MatchKind::CallArgument,
                    symbol,
                },
            }
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
    kind: MatchKind,
    symbol: impl Into<String>,
) -> Result<QueryDecl, QueryBuildError> {
    let event_var = VarId::new(0);
    let object_var = VarId::new(1);
    let object_binding = match object_binding {
        MemberObjectBinding::Constructed => QueryPredicate::ConstructedObject {
            bind: object_var,
            identity: identity.clone(),
        },
        MemberObjectBinding::Returned => QueryPredicate::ReturnedObject {
            bind: object_var,
            identity: identity.clone(),
        },
    };
    let branches = vec![
        QueryExpr::select_event(event_var),
        QueryExpr::require(QueryPredicate::EventKind {
            event: event_var,
            expected: event,
        }),
        QueryExpr::require(QueryPredicate::EventIdentity {
            event: event_var,
            expected: identity,
        }),
        QueryExpr::require(object_binding),
        QueryExpr::require(QueryPredicate::MemberSubject {
            event: event_var,
            object: object_var,
        }),
    ];
    Ok(QueryDecl {
        expression: QueryExpr::all(AllExpr::new(branches)?),
        emission: EmissionDecl {
            primary_var: event_var,
            kind,
            symbol: symbol.into(),
        },
    })
}
