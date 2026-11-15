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

## Source review conclusions for implementation planning

- Include geometrically zero-area triangles when their three source indices are
  distinct. Exclude repeated-index triangles entirely from edge incidence and
  record their excluded count. Dropping only self edges would let collapsed
  triangles create misleading degree observations.
- Count distinct eligible source triangle ordinals for canonical(min,max) edges
  within one source primitive. Cyclic/reversed duplicate faces remain distinct
  incidents. This degree test is sufficient for edge incidence, not manifoldness.
- Boundary means unique degree1edges; high incidence means unique degree>2edges.
  Keep lowest16lexicographic source vertex pairs, with typed edge evidence rather
  than triangle ordinals. Coincident positions do not merge indices or primitives.
- Complete existing cheap rules across the whole document before starting edge
  phases. Edge allocation/scratch refusal must preserve those findings. Whole-job
  cancellation still discards publication. Findings-cap refusal also requires
  explicit incomplete coverage, without evicting completed cheap findings.
- Per-rule coverage must report Completed only for its full eligible scope;
  ResourceLimited/Skipped includes reason, aggregate examined/omitted/excluded
  counts and bounded primitive samples. No unbounded per-primitive coverage list.
- Heavy case exact estimate:1559826triangles×3×12=56153736bytes(53.55MiB).
  Keeping852516referenced bytes raises this to54.37MiB. Release both earlier face
  sort and referenced buffers where possible; verify actual record size and all
  prospective arithmetic. Never overlap face-sort storage with edge records.

These decisions narrow the proposal; the report coverage API and tests remain
to be implemented and reviewed. Suggested first count oracle: one triangle has
3boundary edges; two triangles sharing one edge have4; a tetrahedron has0; three
triangles sharing one edge have6boundary edges and1high-incidence edge. Duplicate
faces can hide boundaries or create high incidence, so report G002 independently.
