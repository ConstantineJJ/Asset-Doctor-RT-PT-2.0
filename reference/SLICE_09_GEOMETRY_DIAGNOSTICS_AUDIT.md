# Initial geometry findings audit — 2026-11-15

Read-only Legacy TopologyAnalyzer.ts:1-295 exposes degeneracy, oriented duplicate
faces, unused vertices, edge/component and thin-triangle observations with capped
locations. NormalsAndUvAnalyzer.ts:12-90 distinguishes missing and invalid normals,
plus UV checks. Catalog DIAG-01..04 records source observations; native parity is
not assumed. Legacy source/runtime/architecture is not copied or executed.

Independent first slice intentionally covers exact zero area, oriented index
duplicates, unused vertices and missing/zero normals only. No scale heuristic or
welding policy is inherited. Results reference immutable source primitives rather
than expanded viewport instances, are bounded and tagged, with explicit omissions.
Repairs/Undo/export, edge/component and UV/material rules require further contracts.
See ADR-0022 for public semantics and worker/resource/publication boundaries.
