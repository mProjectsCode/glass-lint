# Query migration baseline

This document records deterministic regression baselines for representative
query shapes. The exact executable assertions live in
`glass-lint-core/tests/query_baseline.rs`; this file explains the baseline
design, source inputs, environment, and expected output invariants.

## Regeneration

Run the baseline tests to verify:

```sh
cargo test -p glass-lint-core --test query_baseline -- --quiet
```

If operation counts or physical plan summaries change intentionally, update
the assertions in `query_baseline.rs` to match the new expected values.

## Baseline cases

### 1. Simple indexed query (global call)

**Source:** `fetch('/data')`

**Rule:** `QueryDecl::call_global("fetch")`

**Environment:** `fetch` registered as global

**Expected:**
- Physical plan: 1 root, 1 `IndexedScan`, 0 `ConstrainedScan`
- Findings: 1 (definite)
- Evidence: 1 trace per finding
- Plan requirements: needs occurrence indexes, no overlay, no flow

### 2. Constrained call

**Source:** `fetch('/api/data')`

**Rule:** `QueryDecl::call_global("fetch").with_arg(0, ValueMatcher::static_string().equals("/api/data"))`

**Environment:** `fetch` registered as global

**Expected:**
- Physical plan: 1 root, 0 `IndexedScan`, 1 `ConstrainedScan`
- Findings: 1 (definite, static value matches)
- Argument: index 0, equality predicate

### 3. Returned-object query

**Source:** `const el = document.createElement('script'); el.src = '...'; document.head.appendChild(el);`

**Rule:** `QueryDecl::member_call_returned("createElement", "appendChild")`

**Environment:** `document` registered as global object

**Expected:**
- Physical plan: 1 root, 1 `ReturnedSubject`
- Findings: 1 (definite, returned-object chain resolved)

### 4. Constructed-instance query

**Source:** `const client = new Client(); client.send(data)`

**Rule:** `QueryDecl::member_call_instance("pkg", "Client", "send")`

**Environment:** module resolution maps `pkg.Client` → export

**Expected:**
- Physical plan: 1 root, 1 `InstanceSubject`
- Findings: 1 (definite, instance correlation verified)

### 5. Local lifecycle

**Source:** Full script-injection fixture (createElement, configure src, appendChild)

**Rule:** Flow-based lifecycle rule

**Expected:**
- Physical plan: 1 root, 1 `Lifecycle`
- Findings: 1 (definite, source→condition→sink complete)
- Flow operations > 0

### 6. Cross-call lifecycle

**Source:** Multi-function script-injection (create in helper, configure in caller, append in caller)

**Rule:** Same flow rule

**Expected:**
- Findings: 1 (definite, cross-function flow tracked)
- Cross-call flow operations > 0

### 7. Project module identity

**Source:** `import { readFile } from 'fs'; readFile('/etc/passwd')`

**Rule:** `QueryDecl::call_module("fs", "readFile")`

**Expected:**
- Physical plan: project_overlay=yes
- Findings: 1 (definite, module identity resolved)

### 8. Ambiguous project alternative

**Source:** Module with ambiguous re-exports

**Rule:** `QueryDecl::call_module("pkg", "ambiguousExport")`

**Expected:**
- Findings: possible (unknown alternative does not erase complete witness)

### 9. Cross-file flow

**Source:** Multi-file script-injection (source in a.js, config in b.js, sink in a.js)

**Rule:** Same flow rule

**Expected:**
- Findings: 1 (cross-file flow)
- Cross-file flow operations > 0

## Baseline invariants

Baselines assert the following are stable:

1. **Finding count** per rule per source
2. **Certainty** (Definite / Possible) per finding
3. **Physical root count and type distribution**
4. **Plan requirement flags** (overlay, flow, cross-call)
5. **Operation counts** for flow projection
6. **Evidence trace count** per finding
7. **Finding order** (deterministic by source location)

These are not opaque snapshots. Each assertion targets a specific stable
field so that intentional changes require targeted updates.
