# ADR-0025 — Source UV inventory and bounded diagnostics

Status: ACCEPTED, 2026-11-16. Implement in data and analysis increments.

Preserve every declared TEXCOORD_n, including untextured/unselected channels,
as one sorted unique source channel per primitive. Limit8channels per primitive;
IDs remain explicit, never inferred from material roles. Decode matching finite
F32 VEC2 or normalized U8/U16 VEC2 through the existing reader. Sparse/global
format restrictions stay unchanged. Invalid unused UVs now refuse import rather
than being silently ignored. Preserve original bytes. No coordinate wrapping,
flipping, welding, normalization of F32 values, repair or export.

Material roles store selected channel IDs. Snapshots derive the same five UV
streams from source channels; GPU layout96bytes/vertex is unchanged. No duplicate
authoritative role arrays. Source admission conservatively adds8bytes×vertices×
channels to existing decoded estimate512MiB, before channel allocation, including
unreferenced channels. This is not a whole-process peak bound. All channel/count/
binding validation occurs at the document boundary as well as the importer.

Algorithm5 adds G009 missing source UV0 (information, count1per primitive), G010
exact zero-area UV triangles (warning per source channel) and G011 source UV
vertices outside the closed0..1square (information per channel). F32 decoded UV
orientation uses existing robust adaptive predicate, no inherited tiny-area epsilon.
G010 examines triangles with3distinct indices and records excluded repeated-index
faces once per primitive. G011 examines all source UV vertices, including unused
ones. Outside0..1 is allowed and can be intentional with wrapping samplers.

Evidence identifies source channel and typed vertex/triangle samples (lowest16).
UV0 absence is explicit, never a guess from role UVs. UV rules scan every source
primitive once, including inactive data; instances do not multiply counts. Per-rule
output for all channels of one primitive is atomic and bounded to8findings. Result
or allocation refusal marks that primitive/rule omitted, retaining prior findings.
Cancellation discards the report. No UV channels means G010/G011 not applicable,
not a checked zero-finding result. Coverage adds explicit not-applicable counts;
full coverage requires checked+not-applicable scope equals source primitive count.

Acceptance: unused UV0 and independent UV2/role IDs, normalized integer UVs,
missing UV0, repeated/collapsed/collinear/mirrored/nonzero tiny-area UVs, allowed
outside values, invalid unselected attributes, exact channel/byte budgets, bound
samples, atomic multichannel refusal, cancellation and source/instance preservation,
native material regressions/full/partial UI and lifecycle. Overlaps, UV islands,
density, tangent baking, rescan/edit/export and repairs remain outside this slice.

Primary sources: [Khronos primitive schema](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/schema/mesh.primitive.schema.json),
[texture coordinate binding](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/schema/textureInfo.schema.json).
Read-only behavior reference: Legacy NormalsAndUvAnalyzer.ts:70-170; no code or
epsilon/architecture is copied or executed.
