# Public matcher API redesign

## Recommendation

Make a clean break and redesign flow matchers around their actual semantic
model: they are currently object-lifecycle matchers, not general data-flow
matchers.

The new API should read as:

> Track an object returned by this call, require these events on that object,
> then report either when configured or when passed to one of these sinks.

## Problems in the current API

The current implementation has several avoidable hazards:

- `source_arg_string()` mutates the most recently added source. Calling it
  before a source silently does nothing.
- `require_all()` changes the meaning of every previously added requirement.
  Without it, requirements implicitly mean "any."
- `emit_when_requirements_met()` is a boolean switch whose interaction with
  sinks must be learned from validation.
- `FlowValueMatcher::Any` has context-dependent semantics: ordinary call
  arguments require a static value, while flow configuration allows dynamic
  values.
- Sources and sinks are raw strings without an explicit provenance or
  precision mode.
- Public fields allow callers to construct invalid or contradictory states
  directly.
- "Flow" suggests arbitrary source-to-sink data flow, but the engine
  specifically tracks an object returned by a call, its configuration
  lifecycle, and its later use.
- The public declaration types are also effectively the compiled
  representation. `compiler/rule.rs` mostly clones the normalized public
  matcher.

## Proposed conceptual API

Introduce these main concepts:

| Concept | Meaning |
|---|---|
| `ObjectFlowMatcher` | The complete lifecycle declaration |
| `ObjectSourceMatcher` | A call whose returned object starts the flow |
| `FlowCondition` | Explicit `any_of` or `all_of` configuration events |
| `ObjectEventMatcher` | A property write or receiver method call |
| `FlowCompletion` | Report on configuration or at a sink |
| `FlowSinkMatcher` | A tracked object appearing in selected call arguments |
| `ValueMatcher` | A context-independent value predicate |

A provider rule would look approximately like this:

```rust
fn remote_element_flow(tag: &str) -> ObjectFlowMatcher {
    let remote_url = ValueMatcher::static_string()
        .starts_with_any(["http://", "https://", "//"]);

    ObjectFlowMatcher::builder("remote element")
        .source(ObjectSourceMatcher::returned_by(
            MemberCallMatcher::rooted("document.createElement")
                .arg(0, ValueMatcher::static_string().equals(tag)),
        ))
        .configured_by(FlowCondition::any_of([
            ObjectEventMatcher::property_write("src", remote_url.clone()),
            ObjectEventMatcher::member_call("setAttribute")
                .arg(0, ValueMatcher::static_string().equals("src"))
                .arg(1, remote_url),
        ]))
        .complete_at(FlowCompletion::any_sink([
            FlowSinkMatcher::argument_of(
                MemberCallMatcher::rooted("document.head.appendChild"),
                0,
            ),
            FlowSinkMatcher::any_argument_of(
                MemberCallMatcher::rooted("document.body.append"),
            ),
        ]))
        .build()
        .unwrap()
}
```

A requirement-only rule becomes explicit:

```rust
ObjectFlowMatcher::builder("file input element")
    .source(ObjectSourceMatcher::returned_by(
        MemberCallMatcher::rooted("document.createElement")
            .arg(0, ValueMatcher::static_string().equals("input")),
    ))
    .configured_by(FlowCondition::event(
        ObjectEventMatcher::property_write(
            "type",
            ValueMatcher::static_string().equals("file"),
        ),
    ))
    .complete_at(FlowCompletion::configuration())
    .build()
```

This removes the two booleans `all_requirements_required` and
`emit_on_requirements`. More importantly, the declaration explains its own
semantics.

## Shared and unambiguous value predicates

`FlowValueMatcher` is already used by ordinary call matchers, so it should
become a general `ValueMatcher`.

Distinguish at least:

```rust
ValueMatcher::any_value()             // Static or dynamic
ValueMatcher::static_string()         // Any proven static string
ValueMatcher::static_string().equals("src")
ValueMatcher::static_string().equals_any(["script", "img"])
ValueMatcher::static_string().starts_with_any(["https://", "//"])
ValueMatcher::static_string().contains_any(["token", "secret"])
ValueMatcher::static_string().contains_all(["foo", "bar"])
```

This removes the context-sensitive interpretation of `Any` and makes
failure-closed behavior visible at the call site.

## Broader matcher cleanup

Apply the same vocabulary consistently across the rest of the public API:

```rust
.matcher(CallMatcher::global("fetch"))
.matcher(CallMatcher::module_export("sdk", "send"))
.matcher(MemberCallMatcher::rooted("navigator.sendBeacon"))
.matcher(MemberCallMatcher::heuristic("menu.addMenuItem"))
.matcher(MemberReadMatcher::module_member("obsidian", "Platform.isMobile"))
```

Specific changes:

- Remove the duplicate `Matcher::global_call`, `Matcher::call`,
  `Matcher::member_call`, and similar facade constructors.
  `RuleBuilder::matcher(impl Into<Matcher>)` already makes the wrapper
  unnecessary.
- Keep `Matcher` as the public sum enum, but let callers normally construct
  concrete matcher types.
- Rename constructors consistently: `rooted`, `heuristic`, `module_member`,
  and `module_export`.
- Replace `arg_string`, `arg_value`, `static_string_arg`, `arg_object_keys`,
  and `arg_rooted_exprs` with a single
  `.arg(index, ValueMatcher/ArgumentMatcher)` vocabulary.
- Make every matcher field private.
- Never silently ignore an invalid builder operation.
- Return path-aware errors such as
  `matcher[1].flow.source.call.argument[0]`, rather than a broad
  `InvalidMatcher(String)`.

## Separate declaration from execution

The public API should compile into a private matcher plan:

```text
provider declaration
    -> validation
    -> canonicalization
    -> private CompiledMatcherPlan
    -> indexes and execution
```

Analysis code currently imports public `FlowMatcher`, `FlowRequirement`, and
`FlowSinkArgs` directly. It should instead consume private compiled types such
as `CompiledObjectFlow`, with canonical or interned paths and precomputed
source, event, and sink keys.

This gives the public API freedom to become expressive without forcing the
execution engine to mirror its shape. It also makes "compile once at catalog
construction" a real boundary.

## Builder safety

Avoid a heavy typestate builder initially. Private fields, composed child
matchers, explicit condition and completion enums, and one fallible `build()`
boundary provide most of the safety while keeping dynamically assembled
provider rules practical.

The builder should still enforce these properties:

- A source constraint is attached directly to its source rather than to the
  most recently added source.
- Conditions use explicit `any_of` and `all_of` expressions rather than a
  global boolean modifier.
- Completion is exactly one explicit mode: configuration or one or more
  sinks.
- Empty alternatives, invalid paths, excessive expression size, and
  unsupported matcher combinations fail during rule construction.
- Invalid operations never become silent no-ops.

## Implementation order

1. Add behavioral characterization tests for every existing flow shape.
2. Introduce the private compiled matcher plan without changing behavior.
3. Add `ValueMatcher` and the unified argument API.
4. Replace `FlowMatcher` with `ObjectFlowMatcher`, explicit conditions, and
   explicit completion.
5. Migrate providers, core tests, and documentation in one clean break.
6. Delete the old types and convenience wrappers without a compatibility
   layer.
7. Add validation tests for invalid ordering, empty groups, excessive
   expression size, and ambiguous completion.
8. Run `make ci`.

The behavioral migration should preserve the existing architectural
invariants: strict provenance, failure-closed unknown semantics, bounded and
deterministic analysis, matcher-independent fact construction, and a single
parse and semantic-analysis pass per file.
