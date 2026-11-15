# ADR-0023 — Source edge incidence and per-check coverage

Status: PROPOSED; implement only after resolving the cases below and tests.

Next bounded diagnostic increment is indexed edge incidence, independently of
near-zero geometry thresholds or spatial welding. Do not infer an open edge is
automatically a defect: seams, intentionally open sheets and separate material
primitives can be valid. Preserve source primitive boundaries and report this scope.

Candidate rules: an undirected edge referenced by one distinct triangle is a
boundary observation; more than two distinct incident triangles is a non-manifold
observation. The latter is not equivalent to full manifoldness verification.
Reversed duplicate triangles remain distinct source triangles, even where the
existing oriented-duplicate rule does not flag them. Counts never imply repair.

Resolve first: self edges and repeated edge references inside an index-degenerate
triangle; whether zero-area triangles with three distinct indices participate;
how intentional split vertices/material boundaries are explained. Cross-primitive
welding and connected components are outside this rule until separately specified.

Resource design candidate: sort records containing undirected index pair and
source triangle ID, deduplicate repeated triangle-edge incidences, then run-length
count. A12byte record ×3edges ×source triangles is roughly56MiB for the heavy
1.56million triangle case. Do not overlap this with the existing16byte face sort
scratch if sharing the64MiB phase cap. Include retained referenced data and exact
size_of/overflow checks; no unchecked HashMap growth. Sorting remains a bounded
noninterruptible phase surrounded by cancellation checks.

Before adding expensive rules, decide report coverage schema: each check must be
Completed, Skipped/Unsupported or ResourceLimited, with scope/version and reason.
Completed zero findings must never stand in for an unrun check. A heavy edge-rule
refusal should preserve completed cheaper checks, while whole-job cancellation
still discards publication. This changes ADR-0022 all-or-nothing coverage and needs
explicit updated tests; do not retrofit a success flag around partial results.

Required fixtures: single triangle, two triangles sharing one edge, tetrahedron,
three distinct incident triangles, cyclic/reversed duplicates, repeated index,
split-but-coincident vertices, separated material primitives, repeated instances,
phase budget boundaries and stale/cancel before publication. Use typed edge samples
(two source vertex indices) with the existing16sample bound. Do not repurpose a
triangle sample index as an edge index or silently change G001..G005 semantics.

No implementation or runtime acceptance is claimed by this proposal.
