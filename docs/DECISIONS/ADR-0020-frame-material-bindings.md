# ADR-0020 — Frame uniform and deduplicated material bindings

Status: ACCEPTED, 2026-11-15. Internal GPU refactor before adding emission.

Replace the four per-channel bound sets with set0 frame uniform and set1 material
textures. Frame uniform std140 is96bytes: viewProjection64, eye/exposure16,
key/fill/reserved16. Host-visible coherent buffer is allocated once and overwritten
only after the single in-flight submission fence retires; no per-frame allocation
or scene/source ownership. Its pool/layout/buffer survives compatible recreation
and is retired before destruction. Pipeline owns an independently compatible layout.

Material set has a sampled-image/sampler pair per fixed channel. Source textures
and encoded image variants keep their existing sharing. Deduplicate complete channel
tuples so repeated instances reuse one material descriptor set. Limit unique map
tuples to4096 before any Vulkan image/pool allocation; rejection preserves current
CPU/GPU scene. This is a derived GPU admission limit, not a source material count:
different source factors may reuse a descriptor tuple. Two white image variants remain.

Per-draw push becomes64bytes: baseColor, material(metal/rough/cutoff/normalScale),
view(flags/AOstrength/reserved), emissive(reserved until next increment). No extra
descriptor-indexing or optional device feature. Four channels use8material bindings;
emission can extend to10 while staying under guaranteed sampler/image minima.
Only2bound sets/64byte push are needed. egui bindings remain independent.

Default/base/PBR/normal/AO output is unchanged in this refactor. Domain snapshots,
88byte vertex admission and source resource budgets unchanged. Native regression
must include all channels, resize/recreation and failure preservation with0errors.
Tests cover host/GLSL offsets, descriptor tuple sharing/cap refusal and short writes.

Sources: [Vulkan limits](https://docs.vulkan.org/spec/latest/chapters/limits.html),
[uniform buffers](https://docs.vulkan.org/guide/latest/descriptor_arrays.html).
