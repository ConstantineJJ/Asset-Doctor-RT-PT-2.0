# Slice12 — Source UV diagnostics behavior audit

2026-11-16. Read-only local Legacy source reference remains HEAD17ca3549.
NormalsAndUvAnalyzer.ts:70–170 inventories named UV attributes, reports missing
UV0, malformed/nonfinite UV0, UV0 outside0..1 and area using an epsilon. Legacy
evidence is source-observed, native behavior remains unverified. No old code,
epsilon, worker/controller structure or implementation is copied or executed.

New ADR-0025 deliberately defines a smaller, independent contract: importer
rejects malformed/nonfinite declared channels, including unused ones; analysis
works on all admitted source TEXCOORD_n arrays, not material-selected role UVs.
MissingUV0 is information, one count per source primitive. Exact zero UV area
uses robust orientation of decoded f32, excludes repeated-index triangles and
does not reject tiny nonzero maps. Outside0..1 is information because wrapping
can be intentional. Original coordinates/bytes remain unchanged.

Each primitive is checked once, including inactive data; visible instances do
not multiply findings. Channels keep explicit IDs and independently bounded
vertex/triangle samples. All channel findings for a primitive/rule publish
atomically; refusal retains earlier findings and marks that rule omitted. No
channels means not-applicable for area/range, distinct from completed zero-finding
checks. Cancellation discards the import candidate/result.

Authored UV_Checker contains unused UV0, valid UV2 with an outside unused vertex,
missingUV0 with UV2, a no-UV primitive and an instance. Bad_Unused_UV has NaN in
an untextured/unselected channel. Full/partial UI, typed source indices, absence
vs N/A, integer source channels, extreme/tiny/mirrored/collinear/repeated-index
cases, output budgets and recovery require verification. No overlaps/islands/
density, focus/repair/export or complete UV diagnostic parity is claimed.
