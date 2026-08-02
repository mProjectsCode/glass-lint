# Output architecture

`glass-lint-output` is the reusable presentation layer above core reports.

```text
core AnalysisReport + analyzed source text
  -> deterministic pretty report values
  -> terminal-oriented rendering
```

The crate owns pretty file, diagnostic, summary, and source-snippet rendering,
including terminal-safe text handling. It does not analyze source, construct
reports, load projects, select providers, or define CLI exit behavior. CLI
format dispatch and JSON serialization remain in `glass-lint-cli`, while this
crate consumes only validated core report data and the source text supplied by
its caller.
