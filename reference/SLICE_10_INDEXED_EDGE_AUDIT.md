# Indexed edge diagnostics audit — 2026-11-16

Read-only Legacy TopologyAnalyzer.ts:270-307 observes boundary/non-manifold
incidence counts and bounded source edge endpoints. It is a behavior reference,
not a new architecture or implementation. No Legacy runtime/code was copied or
executed. Catalog DIAG-03 remains partial; full topology parity is unproven.

Independent contract ADR-0023 defines source-primitive/index scope, distinct source
triangle incidence, repeated-index exclusion, intentional boundaries and typed
vertex-pair evidence. Coincident positions, instances and material primitives
are not welded. Degree>2 is an edge observation, not complete manifoldness proof.
Source normal/zero-area rules are unchanged. Per-rule coverage explicitly separates
completed evidence from resource refusal. No automatic repair or health score.
