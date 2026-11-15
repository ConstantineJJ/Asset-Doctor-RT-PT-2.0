# ADR-0002 — Document and source fidelity

Status: PROPOSED; gate before import/edit implementation.

Use a renderer-independent document with stable IDs, monotonic revisions and
versioned schema. Preserve source bytes/provenance plus explicit supported decoded
data; unknown extensions/extras remain opaque and cannot be silently discarded.
Proposed basis: glTF right-handed, Y-up, metres; retain authored local transforms,
including negative scales. Camera fitting/normalization is presentation only.

Need acceptance fixtures for scene roots/instancing/skins/morphs/clips/materials,
opaque extension roundtrip and tolerances. IDs must survive Undo/Redo and cannot
depend on GPU handles, array order or UI selection. Do not implement a new mesh
graph by translating the old Three.js object hierarchy.
