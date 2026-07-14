# glass-lint-harness-cli

This package owns the `glass-lint-harness` executable. Reusable case parsing,
adapters, reports, and profiling remain in [`glass-lint-harness`](../glass-lint-harness/).

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- verify tests/e2e
```
