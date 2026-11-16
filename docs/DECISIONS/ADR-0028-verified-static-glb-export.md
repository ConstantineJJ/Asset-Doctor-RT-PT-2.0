# ADR-0028 — Verified static GLB export

Status: ACCEPTED, 2026-11-16, CPU buffer export only. Extends ADR-0027's format
subset; native edit publication and full Heal still require later increments.

Reuse the existing bounded source parser/preflight, not a new decoder. Source
MeshId order is the original meshes→primitives flatten order, retained across
index commands. Derive a source binding at export, including inactive primitives;
do not map by visible draws, name or node instance. Reject missing/extra mappings.

Only current triangle indices may differ: they must be a nonempty whole-triangle
subsequence of original indices, preserving order/winding and vertex attributes.
U8/U16/U32 and implicit source indices are supported. Changed primitives get new
U32 index accessors/views in appended BIN storage. Preserve original BIN bytes
including padding, original views/accessors/encoded images and all other metadata
semantics; never rewrite vertex streams/materials or bake world transforms.

Unchanged current indices use the original GLB bytes, including opaque metadata/
unknown chunks. Still independently reimport and compare the entire current
document; revision alone is not proof of fidelity. Undo restoring original data
can therefore export an exact original copy at its new revision.

Changed output is deliberately core-only: reject nonempty extensions/extension
lists, non-null extras and unknown schema members. Vertex attribute semantic
names may remain custom because their unchanged source buffers/references are
preserved. Reject unknown GLB chunks rather than dropping them. No skin/animation/
morph/sparse/compressed/BLEND export support is inferred from a viewer. Unknown
metadata is preserved by exact copy or explicitly blocks changed export.

Parse changed metadata with bounded duplicate-key/depth/item checks; serialize
through a bounded writer. Accessor min/max must be numeric arrays of the declared
dimensions/component range; viewing can accept opaque bounds that changed export
must refuse. Metadata key order/whitespace may change, semantics must
not. Before issuing an opaque VerifiedGlb, independently reimport candidate bytes
and compare nodes/roots/transforms, all source primitives/indices/attributes/UV
sets/bindings, core material factors, samplers/images and texture statistics.
Also compare serialized metadata with the planned metadata; original BIN prefix
is retained byte-for-byte. No tolerance/lossy repair or health-score shortcut.
Large array/image comparisons are cancellable and image pairs are checked once.

VerifiedGlb binds a private document-origin stamp plus ID/revision, including
foreign documents with the same public tag. A stale proof cannot be used as a
current saved result. Original bytes/document/history remain untouched. Verified
memory bytes do not imply a file was saved.

Prototype export caps:192MiB output,16MiB JSON,200000metadata values,depth64,
4096changed primitives; also respect import limits and GLB u32 lengths. These
bound encoded data/parse items, not whole-process peaks; readback decoding holds
another document/images alongside the current document. No new registry package:
promote pinned serde_json/serde to explicit production use inside ad-formats.

Acceptance: unchanged/exact-Undo copy; index edits/instances/inactive/multiscene;
all material roles/UV/colors/tangents; source index types/implicit/shared accessor;
unknown metadata/chunks refusal vs exact copy; duplicate/deep/oversize metadata;
budgets/cancellation/stale/foreign proof; deliberately mismatched document readback.

Primary references: [Khronos glTF/GLB specification](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/Specification.adoc)
for chunk order/alignment, BIN references and little-endian U32 indices. No Legacy
implementation or separate Heal/Rig export architectures are copied.
