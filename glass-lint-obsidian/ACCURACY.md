# Obsidian rule accuracy policy

The default `recommended` profile is precision-first. A rule belongs in this
profile only when positive examples and adversarial negatives support the
matching mechanism it uses. Glass Lint does not make a numeric precision claim
until a representative, manually labeled bundle corpus exists.

## Accepted mechanisms

- Lexically resolved global calls with local shadowing excluded.
- ESM, CommonJS, namespace, bundled-wrapper, alias, and destructuring provenance.
- Rooted member chains with reassignment-aware alias flow.
- Exact static call-argument constraints.
- Connected AST flows whose source, transformations, and sink are all observed.
- Parsed literals rather than raw matches in comments or identifiers.

## Heuristic profile

The `heuristic` profile additionally includes rules based on broad literal
fragments, unconstrained syntactic member names, suffix member reads, or class
and method names without verified provenance. These rules remain useful for
discovery but are not suitable as high-confidence lint failures.

Every heuristic rule remains available under its stable `obsidian:<name>` ID.
Promotion to `recommended` requires focused negative coverage and the removal
or explicit justification of every unconstrained matcher. See the repository
[testing strategy](../TESTING.md) for required rule coverage.
