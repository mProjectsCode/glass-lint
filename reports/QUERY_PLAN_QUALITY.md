# Query plan quality baseline

This is the deterministic workload baseline for the phase-14 plan optimizer.
It uses the checked-in `tests/e2e` corpus, the Obsidian recommended profile,
one worker, one warm-up, and one repetition:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness --quiet -- \
  profile --path tests/e2e --provider obsidian --profile recommended \
  --repeat 1 --workers 1
```

The measured run covered 14 inputs and 24,316 bytes, produced 64 findings,
and completed without diagnostics. The report recorded 65 evidence items,
2 maximum live alternatives, 20 coalescing comparisons, and 3 fixed-point
iterations. The lint phase was 30.1 ms in this environment.

The optimizer currently applies only deterministic canonical root ordering and
exact deduplication. Roots with different evidence descriptors remain separate,
so evidence order and coverage cannot change. The compiler's reference oracle
and physical-plan tests compare the optimized roots with the unoptimized
semantic roots before runtime data is involved.
