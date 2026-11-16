# ADR-0024 — Vertex-index connected components

Status: ACCEPTED for read-only source diagnostics, 2026-11-16.

G008 reports more than one vertex-connected component in a source primitive as
information. Connectivity is through shared vertex indices in eligible triangles,
including point-only joins; this is not edge-connected surface or physical part
counting. Positions, source primitives, materials and instances are not welded.
Each decoded source primitive, including inactive data, is checked once.

Eligibility matches ADR-0023: three distinct indices. Distinct-index collinear
triangles participate. Repeated-index triangles are excluded and counted. Unused
vertices and vertices referenced only by excluded triangles are not components;
G003 retains its independent unused-source-vertex meaning. Zero eligible faces
completes with zero components; one component produces no G008 finding. Neither
case establishes general asset health. Duplicate/reversed eligible source faces
count as separate source triangles inside their component.

Algorithm4 extends fixed coverage to8rules. A G008 finding count is the number
of components, with up to16typed component samples ordered by smallest source
vertex index. Each sample retains that representative, unique vertex count and
eligible source triangle count. It is not a location for every vertex or repair.

An independent disjoint-set forest uses parent, member-count and triangle-count
u32 arrays:12bytes/source vertex, checked prospective against existing64MiB
analysis scratch, with fallible allocation. Earlier core/edge buffers are retired.
Union by member count plus iterative compression bounds traversal; no adjacency
map, recursion, spatial hashing or unbounded component list. During final ascending
vertex scan, consuming the forest allows member counts to mark already summarized
roots. First encounter is the component's smallest source index, independent of
union root selection. Cancellation checks cover initialization, faces and summary.

Scratch/allocation/finding refusal marks only G008 coverage incomplete and retains
all earlier findings. G008 is still attempted after an edge-phase refusal. Whole-
job cancellation returns Cancelled without publishing partial results. Upfront
core admission remains unchanged. Scratch limits are not whole-process peaks.

Acceptance: shared-vertex versus disjoint triangles, coincident split indices,
unused/excluded-only vertices, collinear/reversed/duplicate triangles, independent
primitives/inactive data/instances, deterministic minimum representatives and
counts,16sample bound, exact scratch threshold, result-cap refusal, mid-phase
cancellation and native full/partial lifecycle/UI evidence. No automatic merge,
tiny-component heuristic, topology repair, full manifoldness or watertightness.

Relevant read-only behavior audit: reference/SLICE_11_COMPONENTS_AUDIT.md.
