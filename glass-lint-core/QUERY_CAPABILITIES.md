# Query capability matrix

Every author-visible capability with its owner, physical route, provider users,
and focused test coverage.

## Legend

| Column | Content |
|---|---|
| Authoring constructor | Exact `EventQuery`, `QueryDecl`, or lifecycle entry point |
| Logical identity | Global, heuristic, rooted, exact module, package module, literal |
| Event | Call, construction, member call/read, class, import, string |
| Subject relation | Direct, returned object, constructed instance, lifecycle object |
| Constraints | Supported argument/value forms and applicable event kinds |
| Evidence | Default kind, symbol, primary event, support evidence |
| Local operator | Exact physical root and owning index/service |
| Project behavior | Overlay, masking, cross-file identity, or none |
| Certainty behavior | Definite/possible/unknown rules |
| Provider users | Built-in rule families using the capability |
| Focused tests | Unit, core integration, project, and provider coverage |

---

## Capability rows

### `call_global` — strict global call

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::call_global(name)` / `QueryDecl::call_global(name)` |
| Logical identity | `IdentitySpec::Global { name }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::Call` |
| Subject relation | `Direct` |
| Constraints | All: `with_arg`, `with_arg_static_string`, `with_arg_static_strings`, `with_arg_static_string_contains`, `with_arg_object_property_value`, `with_arg_object_keys`, `rooted_expressions` |
| Evidence | `MatchKind::Call`, symbol = name |
| Local operator | `PhysicalRoot::IndexedScan` (unconstrained) / `ConstrainedScan` (with argument constraints) — `OccurrenceIndexes::occurrences_for_indexed` via `CallIndexes::global_calls` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None |
| Certainty behavior | `Definite` when global exists in environment and identity is proven; `Possible` when alias chain is incomplete; unknown when environment does not contain the global |
| Provider users | `js:network.request` (`fetch`), `js:browser.clipboard-write` (`navigator.clipboard.writeText` → rooted), `node:network` (`fetch`, `require`) |
| Focused tests | Unit: `physical::tests::global_call_produces_indexed_scan`. Integration: `declarative_matching::global_call` |

### `call_heuristic` — heuristic spelling call

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::call_heuristic(name)` / `QueryDecl::call_heuristic(name)` |
| Logical identity | `IdentitySpec::Heuristic { name }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::Call` |
| Subject relation | `Direct` |
| Constraints | All argument forms |
| Evidence | `MatchKind::Call`, symbol = name |
| Local operator | `PhysicalRoot::IndexedScan` — `OccurrenceIndexes::occurrences_for_indexed` via `CallIndexes::calls` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None |
| Certainty behavior | `Possible` (heuristic identity is never strict) |
| Provider users | `js:telemetry.indicator` (heuristic sendBeacon) |
| Focused tests | Unit: `physical::tests::heuristic_call_produces_indexed_scan`. Integration: `declarative_matching::heuristic_call` |

### `call_module` — exact module export call

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::call_module(module, export)` / `QueryDecl::call_module(module, export)` |
| Logical identity | `IdentitySpec::ModuleExport { module, export }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::Call` |
| Subject relation | `Direct` |
| Constraints | All argument forms |
| Evidence | `MatchKind::Call`, symbol = `module.export` |
| Local operator | `PhysicalRoot::IndexedScan` / `ConstrainedScan` — `OccurrenceIndexes::occurrences_for_indexed` via `CallIndexes::module_calls` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | Overlay: `LinkedOccurrenceView` remaps local occurrences through module export identities |
| Certainty behavior | `Definite` when export identity is proven; `Unknown` when module is unresolved or ambiguous |
| Provider users | `js:node.filesystem` (`fs` module calls), `js:browser.remote-resource` |
| Focused tests | Unit: `physical::tests::module_call_produces_indexed_scan`. Integration: `declarative_matching::module_call` |

### `call_package` — package module export call

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::call_package(module, export)` / `QueryDecl::call_package(module, export)` |
| Logical identity | `IdentitySpec::PackageModuleExport { module: ModuleSpecifierPattern, export }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::Call` |
| Subject relation | `Direct` |
| Constraints | All argument forms |
| Evidence | `MatchKind::Call`, symbol = `module.export` |
| Local operator | `PhysicalRoot::IndexedScan` / `ConstrainedScan` — `OccurrenceIndexes::occurrences_for_indexed` via pattern match on package buckets |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | Overlay: package-scoped identity resolution |
| Certainty behavior | `Definite` when package identity resolves; `Possible` for pattern match with multiple candidates |
| Provider users | `obsidian:network.request` (`@obsidian/` packages) |
| Focused tests | Unit: `physical::tests::module_call_produces_indexed_scan`. Integration: `declarative_matching::package_call` |

### `member_call_rooted` — rooted chain member call

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::member_call_rooted(chain)` / `QueryDecl::member_call_rooted(chain)` |
| Logical identity | `IdentitySpec::Rooted { path: SymbolPath }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::MemberCall { member: SymbolPath }` |
| Subject relation | `Direct` |
| Constraints | All argument forms |
| Evidence | `MatchKind::MemberCall`, symbol = chain |
| Local operator | `PhysicalRoot::IndexedScan` / `ConstrainedScan` — `OccurrenceIndexes::occurrences_for_indexed` via `MemberIndexes::rooted_calls` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None (rooted paths are local) |
| Certainty behavior | `Definite` when the root matches a global object alias; `Unknown` when the root is not a known global |
| Provider users | `obsidian:storage.*`, `obsidian:vault.*`, `obsidian:workspace.*`, `js:browser.environment` (`document.cookie`, `window.localStorage`) |
| Focused tests | Unit: `physical::tests::rooted_member_call_produces_indexed_scan`. Integration: `declarative_matching::rooted_member_call` |

### `member_call_heuristic` — heuristic member call

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::member_call_heuristic(chain)` / `QueryDecl::member_call_heuristic(chain)` |
| Logical identity | `IdentitySpec::Heuristic { name }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::MemberCall { member: SymbolPath }` |
| Subject relation | `Direct` |
| Constraints | All argument forms |
| Evidence | `MatchKind::MemberCall`, symbol = chain |
| Local operator | `PhysicalRoot::IndexedScan` — `MemberIndexes::calls` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None |
| Certainty behavior | `Possible` (heuristic identity) |
| Provider users | General fallback |
| Focused tests | Integration: `declarative_matching::heuristic_member_call` |

### `member_call_module` — module namespace member call

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::member_call_module(module, member)` / `QueryDecl::member_call_module(module, member)` |
| Logical identity | `IdentitySpec::ModuleNamespace { module }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::MemberCall { member }` |
| Subject relation | `Direct` |
| Constraints | All argument forms |
| Evidence | `MatchKind::MemberCall`, symbol = module |
| Local operator | `PhysicalRoot::IndexedScan` / `ConstrainedScan` — `MemberIndexes::module_calls` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | Overlay: namespace identity resolution |
| Certainty behavior | `Definite` when module namespace is proven |
| Provider users | `js:node.filesystem`, `obsidian:network.request` |
| Focused tests | Integration: `declarative_matching::module_member_call` |

### `member_call_instance` — constructed instance member call

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::member_call_instance(module, export, member)` / `QueryDecl::member_call_instance(module, export, member)` |
| Logical identity | `IdentitySpec::ModuleExport { module, export }` (constructor identity) |
| Normalized relation | `NormalizedSubject::Instance` in `normalize.rs` |
| Event | `EventSpec::MemberCall { member }` |
| Subject relation | `ConstructedInstance` |
| Constraints | All argument forms (on the member call) |
| Evidence | `MatchKind::MemberCall`, symbol = `module.export` |
| Local operator | `PhysicalRoot::InstanceSubject` — `OccurrenceIndexes::occurrences_for_instance` via `MemberIndexes::instance_calls` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | Overlay: constructor module identity resolution |
| Certainty behavior | `Definite` when constructor identity is proven and instance correlation holds |
| Provider users | `obsidian:lifecycle.*` (`obsidian.Plugin.loadData`), `obsidian:editor.*`, `obsidian:view.*` |
| Focused tests | Unit: `physical::tests::instance_subject_produces_instance_scan`. Integration: `declarative_matching::instance_member_call` |

### `member_call_returned` — returned-object member call

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::member_call_returned(source, member)` / `QueryDecl::member_call_returned(source, member)` |
| Logical identity | `IdentitySpec::Rooted { path }` (producer identity) |
| Normalized relation | `NormalizedSubject::Returned` in `normalize.rs` |
| Event | `EventSpec::MemberCall { member }` |
| Subject relation | `ReturnedObject` |
| Constraints | All argument forms |
| Evidence | `MatchKind::MemberCall`, symbol = source |
| Local operator | `PhysicalRoot::ReturnedSubject` — `OccurrenceIndexes::occurrences_for_returned` via `MemberIndexes::returned_calls` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None (rooted producer) |
| Certainty behavior | `Definite` when producer identity and object correlation are proven |
| Provider users | `js:browser.filesystem` (`showDirectoryPicker.getFileHandle`), `obsidian:markdown.*` |
| Focused tests | Unit: `physical::tests::returned_subject_produces_returned_scan`. Integration: `declarative_matching::returned_member_call` |

### `member_read_rooted` — rooted member read

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::member_read_rooted(chain)` / `QueryDecl::member_read_rooted(chain)` |
| Logical identity | `IdentitySpec::Rooted { path }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::MemberRead { member }` |
| Subject relation | `Direct` |
| Constraints | Not applicable (member reads have no arguments) |
| Evidence | `MatchKind::MemberRead`, symbol = chain |
| Local operator | `PhysicalRoot::IndexedScan` — `MemberIndexes::rooted_reads` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None |
| Certainty behavior | Same as `member_call_rooted` |
| Provider users | `obsidian:platform.*` (`obsidian.Platform.isMobile`), `js:browser.permissions*` |
| Focused tests | Unit: `physical::tests` covers via `member_read_rooted`. Integration: `declarative_matching::rooted_member_read` |

### `member_read_module` — module member read

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::member_read_module(module, member)` / `QueryDecl::member_read_module(module, member)` |
| Logical identity | `IdentitySpec::ModuleNamespace { module }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::MemberRead { member }` |
| Subject relation | `Direct` |
| Constraints | Not applicable |
| Evidence | `MatchKind::MemberRead`, symbol = module |
| Local operator | `PhysicalRoot::IndexedScan` — `MemberIndexes::module_reads` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | Overlay: namespace identity resolution |
| Certainty behavior | Same as `member_call_module` |
| Provider users | `obsidian:platform.*` |
| Focused tests | Integration: `declarative_matching::module_member_read` |

### `member_read_returned` — returned-object member read

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::member_read_returned(source, member)` / `QueryDecl::member_read_returned(source, member)` |
| Logical identity | `IdentitySpec::Rooted { path }` (producer identity) |
| Normalized relation | `NormalizedSubject::Returned` in `normalize.rs` |
| Event | `EventSpec::MemberRead { member }` |
| Subject relation | `ReturnedObject` |
| Constraints | Not applicable |
| Evidence | `MatchKind::MemberRead`, symbol = source |
| Local operator | `PhysicalRoot::ReturnedSubject` — `MemberIndexes::returned_reads` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None (rooted producer) |
| Certainty behavior | Same as `member_call_returned` |
| Provider users | `obsidian:storage.*` |
| Focused tests | Unit: `physical::tests::member_read_returned_produces_returned_scan`. Integration: `declarative_matching::returned_member_read` |

### `member_call_package` / `member_read_package` — package member call/read

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::member_call_package(module, member)` / `member_read_package(module, member)` |
| Logical identity | `IdentitySpec::PackageModuleNamespace { pattern }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::MemberCall` / `MemberRead` |
| Subject relation | `Direct` |
| Constraints | All argument forms (calls only) |
| Evidence | `MatchKind::MemberCall` / `MemberRead`, symbol = module |
| Local operator | `PhysicalRoot::IndexedScan` — package bucket scan |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | Overlay: package-scoped identity |
| Certainty behavior | `Definite` when package identity matches |
| Provider users | `obsidian:codemirror.*` |
| Focused tests | Integration: `declarative_matching::package_member_call` |

### `import_exact` — exact module specifier import

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::import_exact(module)` / `QueryDecl::import_exact(module)` |
| Logical identity | `IdentitySpec::LiteralString { predicate }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::Import` |
| Subject relation | `Direct` |
| Constraints | Not applicable |
| Evidence | `MatchKind::Import`, symbol = module |
| Local operator | `PhysicalRoot::IndexedScan` — `LiteralIndexes::imports` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | Project identity matching for module specifiers |
| Certainty behavior | `Definite` when import specifier matches exactly |
| Provider users | `js:node.network` (`require('http')`), `js:browser.remote-resource` |
| Focused tests | Unit: `physical::tests::import_exact_produces_indexed_scan`. Integration: `declarative_matching::exact_import` |

### `import_package` — package pattern import

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::import_package(module)` / `QueryDecl::import_package(module)` |
| Logical identity | `IdentitySpec::PackageSpecifier { pattern }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::Import` |
| Subject relation | `Direct` |
| Constraints | Not applicable |
| Evidence | `MatchKind::Import`, symbol = module |
| Local operator | `PhysicalRoot::IndexedScan` — pattern scan on import buckets |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | Package-scoped pattern matching |
| Certainty behavior | `Definite` when import matches package pattern |
| Provider users | `js:node.filesystem`, `obsidian:network.request` |
| Focused tests | Integration: `declarative_matching::package_import` |

### `string_contains` — static string reference

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::string_contains(value)` / `QueryDecl::string_contains(value)` |
| Logical identity | `IdentitySpec::LiteralString { predicate }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::StringReference` |
| Subject relation | `Direct` |
| Constraints | Not applicable |
| Evidence | `MatchKind::StringContains`, symbol = value |
| Local operator | `PhysicalRoot::IndexedScan` — `LiteralIndexes::strings` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None |
| Certainty behavior | `Definite` when the literal string contains the predicate |
| Provider users | `js:telemetry.indicator`, `js:string_timer` |
| Focused tests | Unit: `physical::tests::string_contains_produces_indexed_scan`. Integration: `declarative_matching::string_reference` |

### `class_heuristic` — heuristic class reference

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::class_heuristic(name)` / `QueryDecl::class_heuristic(name)` |
| Logical identity | `IdentitySpec::Heuristic { name }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::ClassReference` |
| Subject relation | `Direct` |
| Constraints | Not applicable |
| Evidence | `MatchKind::Class`, symbol = name |
| Local operator | `PhysicalRoot::IndexedScan` — `ConstructionIndexes::classes` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None |
| Certainty behavior | `Possible` (heuristic identity) |
| Provider users | General heuristic class matching |
| Focused tests | Unit: `physical::tests::class_reference_produces_indexed_scan`. Integration: `declarative_matching::class_reference` |

### `class_module` — module class reference

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::class_module(module, export)` / `QueryDecl::class_module(module, export)` |
| Logical identity | `IdentitySpec::ModuleExport { module, export }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::ClassReference` |
| Subject relation | `Direct` |
| Constraints | Not applicable |
| Evidence | `MatchKind::Class`, symbol = `module.export` |
| Local operator | `PhysicalRoot::IndexedScan` — `ConstructionIndexes::module_classes` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | Overlay: module identity resolution |
| Certainty behavior | `Definite` when module class identity is proven |
| Provider users | `obsidian:bases.*`, `obsidian:view.*` |
| Focused tests | Integration: `declarative_matching::module_class` |

### `constructor_global` — strict global constructor

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::constructor_global(name)` / `QueryDecl::constructor_global(name)` |
| Logical identity | `IdentitySpec::Global { name }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::Construct` |
| Subject relation | `Direct` |
| Constraints | All argument forms |
| Evidence | `MatchKind::Constructor`, symbol = name |
| Local operator | `PhysicalRoot::IndexedScan` — `ConstructionIndexes::global_constructors` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None |
| Certainty behavior | `Definite` when global constructor exists in environment |
| Provider users | `js:url_construction` (`new URL(...)`) |
| Focused tests | Unit: `physical::tests::constructor_global_produces_indexed_scan`. Integration: `declarative_matching::global_constructor` |

### `constructor_rooted` — strict rooted constructor

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::constructor_rooted(chain)` / `QueryDecl::constructor_rooted(chain)` |
| Logical identity | `IdentitySpec::Rooted { path }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::Construct` |
| Subject relation | `Direct` |
| Constraints | All argument forms |
| Evidence | `MatchKind::Constructor`, symbol = chain |
| Local operator | `PhysicalRoot::IndexedScan` — `ConstructionIndexes::rooted_constructors` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None |
| Certainty behavior | `Definite` when the rooted constructor identity is proven |
| Provider users | `js:dynamic-code.webassembly` (`new WebAssembly.Module(...)`) |
| Focused tests | Provider contract: `glass-lint-js/src/rules/js/webassembly` |

### `constructor_heuristic` — heuristic constructor

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::constructor_heuristic(name)` / `QueryDecl::constructor_heuristic(name)` |
| Logical identity | `IdentitySpec::Heuristic { name }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::Construct` |
| Subject relation | `Direct` |
| Constraints | All argument forms |
| Evidence | `MatchKind::Constructor`, symbol = name |
| Local operator | `PhysicalRoot::IndexedScan` — `ConstructionIndexes::constructors` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | None |
| Certainty behavior | `Possible` (heuristic identity) |
| Provider users | General heuristic constructor matching |
| Focused tests | Integration: `declarative_matching::heuristic_constructor` |

### `constructor_module` — module constructor

| Field | Value |
|---|---|
| Authoring constructor | `EventQuery::constructor_module(module, export)` / `QueryDecl::constructor_module(module, export)` |
| Logical identity | `IdentitySpec::ModuleExport { module, export }` |
| Normalized relation | `NormalizedEvent::Direct` in `normalize.rs` |
| Event | `EventSpec::Construct` |
| Subject relation | `Direct` |
| Constraints | All argument forms |
| Evidence | `MatchKind::Constructor`, symbol = `module.export` |
| Local operator | `PhysicalRoot::IndexedScan` — `ConstructionIndexes::module_constructors` |
| Runtime owner | `analysis/matching/arguments` for constrained predicates; otherwise `analysis/matching/query` |
| Project behavior | Overlay: module identity resolution |
| Certainty behavior | `Definite` when module constructor identity is proven |
| Provider users | `obsidian:ui.*` (`new obsidian.Modal(...)`) |
| Focused tests | Integration: `declarative_matching::module_constructor` |

### Argument value predicates

| Capability | Constructor | Applicable events | Evidence scope |
|---|---|---|---|
| Any static string | `ValueMatcher::static_string()` / `with_arg_static_string(i)` | Call, Construct, MemberCall | `ConstrainedScan` |
| Exact values | `ValueMatcher::static_string().equals("v")` / `with_arg_static_strings(i, ["v"])` | Call, Construct, MemberCall | `ConstrainedScan` |
| Prefix (starts with) | `ValueMatcher::static_string().starts_with_any(["v"])` | Call, Construct, MemberCall | `ConstrainedScan` |
| Contains any | `ValueMatcher::static_string().contains_any(["v"])` | Call, Construct, MemberCall | `ConstrainedScan` |
| Contains all | `ValueMatcher::static_string().contains_all(["a", "b"])` | Call, Construct, MemberCall | `ConstrainedScan` |
| Object keys | `ArgumentMatcher::object_keys(["k"])` / `with_arg_object_keys(i, ["k"])` | Call, Construct, MemberCall | `ConstrainedScan` |
| Object property value | `ArgumentMatcher::object_property_value("p", matcher)` / `with_arg_object_property_value(i, "p", matcher)` | Call, Construct, MemberCall | `ConstrainedScan` |
| Rooted expressions | `ArgumentMatcher::rooted_expressions(["a.b"])` | Call, Construct, MemberCall | `ConstrainedScan` |

### `AnyOf` and `AllOf` lifecycle conditions

| Capability | Constructor | Details |
|---|---|---|
| AnyOf condition | `LifecycleCondition::any_of([...])` | Union of alternative lifecycle requirements |
| AllOf condition | `LifecycleCondition::all_of([...])` | Intersection of lifecycle requirements |
| Event condition | `LifecycleCondition::event(member)` | Single property-write or member-call requirement |
| Property write event | `LifecycleEvent::property_write(property, value_matcher)` | Property assignment on tracked object |
| Member call event | `LifecycleEvent::member_call(member, args)` | Method call on tracked object |

### Configuration and sink completion

| Capability | Constructor | Details |
|---|---|---|
| Configuration completion | `LifecycleCompletion::configuration()` | Object is configured when requirements are met |
| Any-sink completion | `LifecycleCompletion::any_sink([...])` | Object reaches a sink that accepts it |
| Exact argument sink | `LifecycleSink::argument_of(chain, index)` | Specific argument of a call chain is the sink |
| Any-argument sink | `LifecycleSink::any_argument_of(chain)` | Any argument of a call chain is the sink |

### Lifecycle sources

| Capability | Constructor | Details |
|---|---|---|
| Source from global return | `LifecycleSource::returned_by(chain).arg(i, matcher)` | Object produced by a rooted call |
| Source with arg constraints | `LifecycleSource::with_arg(i, matcher)` | Constrain source creation arguments |

### Lifecycle execution modes

| Mode | Owner | Entry | Bounded by |
|---|---|---|---|
| Local flow | `analysis::flow::projector` | `ObjectFlowProjector::collect_into()` | `FlowLimits` (objects, states, emissions, operations) |
| Cross-call flow | `analysis::flow::cross` | `cross::collect()` | `MAX_CONTEXTS`, `MAX_PENDING`, operation budget |
| Cross-file flow | `analysis::flow::cross` | `cross::collect()` (via call graph) | `MAX_SOURCE_REFINEMENT_ROUNDS`, fixed-point budget |

---

## Execution ownership inventory

| Phase | Owner module | Entry point | Consumes | Produces |
|---|---|---|---|---|
| Indexed occurrence execution | `analysis::matching::query` | `OccurrenceIndexes::evidence_for_with_overlay()` | `PhysicalRoot::IndexedScan`, `LinkedOccurrenceView` | `Vec<ClassificationEvidence>` |
| Constrained fact-stream projection | `analysis::matching::arguments` | `compute_constrained_evidence_from_stream_with_overlay()` | `PhysicalRoot::ConstrainedScan`, `FactStream`, argument matchers | Per-rule evidence vectors |
| Returned-subject execution | `analysis::matching::query` | `OccurrenceIndexes::occurrences_for_returned()` | `PhysicalRoot::ReturnedSubject`, `ReturnedMemberKey` | `CandidateOccurrences` |
| Instance-subject execution | `analysis::matching::query` | `OccurrenceIndexes::occurrences_for_instance()` | `PhysicalRoot::InstanceSubject`, `InstanceMemberKey` | `CandidateOccurrences` |
| Local lifecycle projection | `analysis::flow::projector` | `ObjectFlowProjector::collect_into()` | `PhysicalRoot::Lifecycle`, `FactStream`, `FunctionEffects` | `Vec<ClassificationEvidence>`, `LocalFlowProjectionOutcome` |
| Cross-call lifecycle summaries | `analysis::flow::cross` | `cross::collect()` | `PhysicalRoot::Lifecycle`, `ProjectSemanticModel`, call graph | Cross-module evidence map, `CrossProjectionOutcome` |
| Cross-file lifecycle projection | `analysis::flow::cross` | `cross::collect()` (via `QualifiedCallGraph`) | Linked module identities, flow roots | Per-module projected evidence |
| Module identity overlay construction | `analysis::matching` | `OccurrenceIndexes::module_overlay()` | `ModuleIdentityMap` | `LinkedOccurrenceView` with remapped/merged buckets |
| Evidence normalization and deduplication | `analysis::matching::evidence` | `normalize_evidence()` | Raw `Vec<ClassificationEvidence>` | Sorted, deduplicated, truncated evidence |
| Operation-count charging | `analysis::flow::projector` and `analysis::flow::cross` | `Budget::charge()`, `charge_operation()` | Flow projection steps | `ProjectionOutcome` / `CrossProjectionOutcome` operation counts |

---

## Certainty behavior under incomplete analysis

| Condition | Definite | Possible | Unknown |
|---|---|---|---|
| Complete strict path | ✓ All predicates proven | — | — |
| One complete path + one unknown alternative | — | ✓ Independent witness preserved | — |
| Unknown identity or module | — | — | ✓ No witness |
| Dynamic value in selective predicate | — | — | ✓ Predicate not satisfied |
| Heuristic identity | — | ✓ Always possible | — |
| Exhausted budget | — | ✓ Fallback to possible | — |
| Ambiguous export resolution | — | ✓ All alternatives possible | — |

## Test coverage footprint

Every capability row above links to:
- **Unit tests** in `api::compiler::physical::tests`, `api::compiler::normalize::tests`, `api::compiler::validate::tests`, `api::rule::query::mod::tests`
- **Core integration tests** in `glass-lint-core/tests/integration/matching/declarative.rs`, `tests/integration/matching/semantic.rs`, `tests/integration/query/composition.rs`, `tests/integration/query/baseline.rs`
- **Project tests** in `tests/projects/`
- **Provider fixture tests** in `glass-lint-js/src/rules/` and `glass-lint-obsidian/src/rules/`
- **E2E tests** in `tests/e2e/`
