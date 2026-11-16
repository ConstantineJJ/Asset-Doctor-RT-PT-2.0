# Connected-component behavior audit — 2026-11-16

Read-only Legacy TopologyAnalyzer.ts:324-387 observes referenced-vertex connected
groups through adjacency traversal, skips unused indexed vertices and applies a
tiny-component heuristic. These are behavior observations, not new runtime parity.
Legacy code/graph/storage/runtime are not copied, executed or modified.

Independent ADR-0024 uses a bounded disjoint-set forest over eligible source index
triangles. It intentionally excludes repeated-index faces, distinguishes vertex
connectivity from edge connectivity, retains typed bounded summaries and per-rule
coverage, and makes no tiny-component or physical-part/repair inference. No spatial
duplicate-position hashing or scale heuristic is included.
