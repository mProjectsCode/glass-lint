# Obsidian rule accuracy policy

The default `Recommended` profile is precision-first. A rule belongs in this profile only when its behavior is supported by positive examples and adversarial negatives covering the matching mechanism it uses. There is deliberately no numeric precision claim until a representative, manually labelled bundle corpus exists.

## Mechanisms accepted for recommended rules

- Lexically resolved global calls with local shadowing excluded.
- ESM, CommonJS, namespace, bundled-wrapper, alias, and destructuring provenance.
- Rooted member chains with reassignment-aware alias flow.
- Exact static call-argument constraints.
- Connected AST flows whose source, transformations, and sink are all observed.
- Parsed literals, never raw matches in comments or identifiers.

## Heuristic profile

The `Heuristic` profile additionally includes rules based on broad literal fragments, unconstrained syntactic member names, suffix member reads, or class/method names without verified provenance. These rules remain useful for discovery but are not suitable as high-confidence lint failures.

Every heuristic rule must stay available under its stable `obsidian:<name>` ID. Promotion to `Recommended` requires a focused negative corpus and removal or justification of every unconstrained matcher.
