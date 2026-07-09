# Core matcher strengthening plan

## Objective

Make `glass-lint-core` recognize the realistic, provenance-preserving JavaScript
patterns in `tests/matcher_targets.rs` without weakening the project’s
precision-first contract. The desired end state is one per-file semantic analysis
that resolves values, calls, arguments, and object flow once, then lets every
declarative matcher query that analysis in near constant time.

This is intentionally a clean break. The current `AliasInfo`, string-chain
representation, `SymbolIndexVisitor`, and `FlowVisitor` should not be extended
piecemeal: they encode overlapping, incomplete views of the same program and
would become slower and harder to keep sound.

## Target-to-capability map

| Failing target(s) | Required capability |
| --- | --- |
| `default_import_namespace_members`, `destructured_esm_namespace_*`, `interop_members_extracted` | Track module namespace/default values and project named properties through bindings and destructuring. |
| `module_provenance_through_sequence_calls`, `module_provenance_through_bound_exports` | Normalize sequence/parenthesized callees and model `.bind`, `.call`, and `.apply` as callable-value transforms. |
| `destructured_rooted_members`, `renamed_*`, `nested_*` | Recursively project object patterns from a rooted value while retaining lexical binding identity. |
| `bound_rooted_members_and_their_arguments` | Preserve the original callable provenance and matcher-visible argument positions on a bound function. |
| `constant_template_literal_substitutions`, `static_array_property_names_through_constant_indexes` | Evaluate a bounded subset of constant expressions, including templates, concatenation, integer indexes, and static arrays. |
| `global_callbacks_through_immediately_invoked_arrows`, `array_iteration`, `promise_handlers` | Apply parameter-to-argument flow to direct IIFEs and explicit, semantics-aware callback models. |
| `rooted_arguments_through_destructured_parameters` | Bind parameter patterns to call-site abstract values and project their fields before matching the callee arguments. |
| `object_argument_keys_through_const_spreads`, `object_assign` | Evaluate immutable object shapes through static spreads and a strictly recognized `Object.assign` form. |
| `object_argument_keys_through_member_function_aliases` | Resolve an identifier callee to its underlying member-call fact before applying argument matchers. |
| `flow_configuration_through_a_source_alias` | Use allocation/object identities, not string chains, for source state; aliases must point to the same identity. |
| `flow_sinks_through_rooted_member_aliases`, `optional_chains` | Resolve every sink call through the shared call-site analysis, including aliases and optional calls. |

The four currently passing targets remain regression cases for the new analysis;
they are not evidence that the current design is sufficient.

## 1. Replace alias strings with a per-file semantic model

Replace `matcher/symbol_index/alias/{mod.rs,collector.rs,collector_helpers.rs}`
with `matcher/semantic/`:

* `scope.rs`: build lexical scopes and assign every declaration and reference a
  stable `SymbolId`. Do not use identifier text as a binding key.
* `value.rs`: define an interned `ValueId` graph with `Unknown`, `Local`,
  `Global`, `RootedMember`, `ModuleNamespace`, `ModuleExport`, `StaticString`,
  `StaticArray`, `StaticObject`, `Callable`, and `Object(ObjectId)` variants.
  A callable records its target, receiver, and bound leading arguments.
* `events.rs`: emit ordered declaration, assignment, property-write, call, and
  construction events. Every event has a lexical scope and source span.
* `resolver.rs`: evaluate expressions at an event with an environment keyed by
  `SymbolId`. Keep a small set of alternatives only while all alternatives agree
  for the requested fact; otherwise return `Unknown`. Cap alternatives and
  evaluation depth to prevent bundle-size blowups.

Bindings must be versioned at writes and read at the source position of each
reference. This replaces `AliasInfo::binding_at`, the separate assignment list,
and property assignment string-prefix rewriting. It preserves the current
reassignment and shadowing guarantees while making destructuring and aliases
composable.

Implement recursive pattern projection in `resolver.rs` for object patterns,
renames, nested patterns, assignment patterns, and statically known array
indexes. A rest pattern is `Unknown` unless the complete finite object shape is
known. Use this both for declarations/assignments and function parameters.

Treat a default import as a module namespace only for member access. Do not make
the default binding itself a named export; this keeps `sdk.send()` precise.
Recognize only unshadowed `require` and the existing unshadowed interop helpers.

## 2. Centralize expression and call resolution

Replace `symbol_index/visitor.rs` with a collector that consumes the semantic
model and emits one `ResolvedCall` per syntactic call/optional call:

```text
ResolvedCall { span, target: ValueId, member_chain, module, arguments, receiver }
```

The expression resolver must unwrap parentheses and sequence expressions, then
resolve member access through the target value. Model these transforms exactly:

* `fn.bind(receiver, ...bound)` creates `Callable { target: fn, receiver, bound }`.
* `fn.call(receiver, ...args)` and `fn.apply(receiver, args)` invoke `fn` only
  when the receiver/argument container is statically analyzable.
* Member aliases retain a `Callable` target rather than collapsing to a textual
  rooted chain.
* Optional calls use the same collector with an `optional` bit; optionality must
  not discard a statically resolved target.

Matcher evaluation should consume `ResolvedCall` facts. Delete the parallel
`calls`, `global_calls`, `module_calls`, `member_calls`, and pending-callee-read
maps in `SymbolIndex`. Retain only compact, matcher-oriented indexes keyed by
interned value/module/chain IDs and sorted spans. This fixes sequence and alias
argument matching without a second AST walk.

## 3. Add a bounded, immutable constant evaluator

Add `semantic/constant.rs`, used by both property resolution and argument
matching. It must evaluate only immutable values with a declared maximum depth,
node count, string length, array length, and object key count.

Support literals; parentheses and sequences; `+` when both operands are static
strings; templates whose substitutions are static strings; const bindings;
integer array indexing; static object literals; `{ ...static, key: static }`; and
`Object.assign({}, static_objects...)` when `Object` is unshadowed. Any write,
unknown spread, getter, computed dynamic key, non-const binding, or limit breach
returns `Unknown`.

Make `arg_string`, computed member names, and `arg_object_keys` query this one
evaluator. Remove `static_string`, `StaticStringArray`, and `StaticObjectKeys`
as separate provenance variants once all callers use `ConstValue`.

## 4. Introduce explicit interprocedural summaries

Add `semantic/summary.rs`. During the per-file pass, record a summary for each
function/arrow/IIFE with parameter-pattern projections, returned values, member
calls, property writes, and sink calls. Instantiate a summary only when every
observed invocation gives the relevant parameter a compatible fact; conflicting
calls keep the parameter unknown, preserving the existing inconsistent-helper
negative test.

Implement models as data, not name heuristics:

* Direct function and arrow IIFEs bind their actual arguments immediately.
* `Array.prototype.forEach` is modeled only for a statically finite array; bind
  its callback value parameter to the joined element fact.
* `Promise.resolve(value).then(callback)` is modeled only when `Promise` and
  `resolve` are unshadowed; bind the callback fulfillment parameter to `value`.

Do not infer behavior for arbitrary `.then`, `.map`, or callback-shaped methods.
Keep summary instantiation memoized by `(FunctionId, abstract argument tuple)`
and stop on recursion/depth limits.

## 5. Rebuild flow matching on object identities

Replace `symbol_index/value_flow.rs` with `semantic/object_flow.rs`. A call such
as `document.createElement('script')` creates a fresh `ObjectId` with a source
fact. Bindings and aliases carry that identity; property writes, configuration
calls, and sinks update/query the same object state regardless of the spelling
used at each site.

The flow engine consumes `ResolvedCall` and ordered write events, so it gets
member aliases and optional calls automatically. Function summaries carry object
identities into helper sinks. On reassignment, replace the binding's identity;
never merge a later object into an earlier one. Keep the current source-before-
sink ordering rule and `require_all` semantics.

Represent state as bitsets of rule requirement IDs on each `(ObjectId, FlowId)`;
avoid cloning `FlowState` vectors for every event. Pre-index sources,
configuration names, and sinks by interned call target to make updates
proportional to relevant rules.

## 6. Migration order and acceptance gates

1. Add semantic scopes, event order, and versioned `ValueId` resolution with
   parity tests from `minified_matchers.rs` and `matcher_behavior.rs`.
2. Move all direct call/member/class/constructor matchers to `ResolvedCall`;
   delete `AliasInfo` and the old visitor only after parity passes.
3. Add constants and pattern projection; make the destructuring/template/object
   target groups pass.
4. Add callable transforms and direct-IIFE summaries; make bind/sequence/IIFE
   targets pass.
5. Add the two constrained library callback models and parameter projection;
   make callback/parameter targets pass.
6. Move flow matching to object identities; make the four flow targets pass.

After each stage, run the complete core integration suite. Add a paired
adversarial negative for every new positive model: shadowed `Object`, mutable
object spread, dynamic array index, reassigned callback, non-finite array,
shadowed `Promise`, and conflicting helper calls.

## Performance and correctness gates

* Parse once and build the semantic model once per file. Matchers must never
  traverse the SWC AST or re-resolve aliases.
* Intern strings, paths, modules, symbols, values, and call targets. Prefer
  `Vec` plus sorted/deduplicated ranges over `BTreeMap<String, ...>` in hot
  per-file paths.
* Use source-order event IDs and binary-search only where a persistent
  environment is unavoidable; otherwise resolve in one forward pass per scope.
* Bound constant evaluation, summary contexts, alias alternatives, and object
  identities, returning unknown rather than degrading precision or latency.
* Add Criterion-style fixture benchmarks (small source, minified bundle, and
  alias-heavy source) before migration. Gate the rewrite on no regression in
  the bundle fixture and record analysis time, allocations, and peak facts.
* Preserve deterministic evidence ordering by sorting on source span before
  applying `ApiRule::EVIDENCE_LIMIT`.

Completion means `cargo test -p glass-lint-core` passes including all 25 target
tests, existing negative tests remain green, Clippy is warning-free, and the
benchmark gate is met.
