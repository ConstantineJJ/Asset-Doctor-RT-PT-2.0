# ADR-0002 — Document and source fidelity

Status: ACCEPTED for static-import subset in ADR-0013 and restricted triangle
index changes/revisions in ADR-0026. General editing/skin/clip/history schema
remains proposed. Original source bytes are immutable; current authoring indices
can change only through the validated command boundary.

Use a renderer-independent document with stable IDs, monotonic revisions and
versioned schema. Preserve source bytes/provenance plus explicit supported decoded
data; unknown extensions/extras remain opaque and cannot be silently discarded.
Proposed basis: glTF right-handed, Y-up, metres; retain authored local transforms,
including negative scales. Camera fitting/normalization is presentation only.

Need acceptance fixtures for scene roots/instancing/skins/morphs/clips/materials,
opaque extension roundtrip and tolerances. IDs must survive Undo/Redo and cannot
depend on GPU handles, array order or UI selection. Do not implement a new mesh
graph by translating the old Three.js object hierarchy.
