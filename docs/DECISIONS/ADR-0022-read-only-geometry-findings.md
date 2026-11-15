# ADR-0022 — Bounded read-only source geometry diagnostics

Status: ACCEPTED for first static diagnostic slice, 2026-11-15.

New ad-analysis depends only on ad-document; no GPU/UI/window/file/network types.
Analyze each decoded source primitive once, including inactive primitives. Instance
and world/presentation transforms do not affect counts. MeshId is document-local;
Report carries document ID/revision and algorithm version. No mutation or repair.

First checks: exactly zero-area triangles (adaptive robust orient2d on all three projections of source f32 values),
same-index oriented duplicate triangles (cyclic rotations match, reversed winding
does not), unused source vertices, missing normals and exactly zero normals.
Missing normals is information because preview has geometric fallback; other
findings are warnings, not proof an asset is unusable. Counts are exact for these
rules; near-zero thresholds, welded topology, edges/components, UV/material/rig
checks and profiles are not included. Import-rejected malformed data is separate.

Findings retain MeshId, typed vertex/triangle index samples (lowest16), total
affected count, severity and stable code. Never retain a sample per defect.
Deterministic primitive/code order. No whole-document health score or auto-fix.

Work occurs in the existing bounded import worker over immutable decoded data.
Ticket/cancel guards cover result publication. Analysis failure/resource refusal
does not prevent an otherwise valid model from opening; UI explicitly reports
incomplete analysis, never a healthy result. On cancellation, discard whole import.
No independent rescan or editing invalidation until those lifecycle contracts exist.

Default limits:4million source triangles total,64MiB scratch per primitive,
4096findings. Scratch is referenced-byte array plus16bytes/triangle sort record;
prospective checked before allocation, fallible reserves. This is analysis working
storage, not a whole-process peak bound. Periodic cooperative cancellation plus
checks before/after bounded in-place unstable sort; sort itself is not interruptible.
Partial results never published as complete. Source bytes and revision unchanged.

Acceptance: known counts/indices for repeated/cyclic/reversed/collinear/unused
geometry, tiny/large scales, normal absence/zero, shared instances counted once,
cancellation/budget refusal/source preservation, source-tag mismatch hidden,
UI displays scope/incomplete status, native import/lifecycle validation.

Relevant audit: reference/SLICE_09_GEOMETRY_DIAGNOSTICS_AUDIT.md.
Primary format reference: [Khronos glTF 2.0](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html).

Numerical review reproduced plain f64 cross cancellation for a nonzero triangle.
Use pinned robust1.2.0 adaptive predicates (MIT OR Apache-2.0), no geometry engine.
Finite f32 inputs fit safe f64 product ranges. Replacement boundary is zero_area.
See [upstream predicate docs](https://docs.rs/robust/1.2.0/robust/fn.orient2d.html).
