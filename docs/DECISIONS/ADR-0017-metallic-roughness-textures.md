# ADR-0017 — Metallic/roughness texture channel

Status: ACCEPTED for the bounded static GLB preview, 2026-11-15.

Preserve a material's optional metallicRoughnessTexture independently of base color,
including its sampler and declared UV set. Sample B as metal and G as roughness,
multiply by the corresponding source factors; ignore R/A. Data uses linear UNORM,
while base color continues SRGB RGB/linear alpha. No pixel rewriting or source edit.
Both retain the existing embedded 8-bit PNG/JPEG restrictions and resource refusal.
MR resources now participate in admission even in the default base-color view;
unsupported/malformed referenced MR data fails import and keeps the previous asset.

Rename the independent image/sampler wrapper to Texture. Encoded source texture
identity and decoded image identity stay independent of use. CPU pixels are decoded
once across roles. TextureAdmission accepts an explicit transfer encoding, retaining
owners and counting unique CPU images and distinct sampled image/encoding variants.
Prospective mip bytes include every variant because Vulkan uploads SRGB and UNORM
copies separately. Existing256MiB/256 resource caps remain; variants also count
toward256. White fallback resources add a small fixed renderer overhead.

Snapshot carries separate base-color and MR UV streams (56bytes/vertex+indices),
without a general material graph. GPU shares image uploads by (image, encoding)
and descriptor sets by (source texture, encoding). Two compatible descriptor sets
bind base color and MR independently. Push layout stays128 bytes. A session-only
MR sampling switch applies only to direct PBR; base-color sampling remains separate.

Acceptance: fixture with independent UV0/UV1, varying B/G and adversarial R/A,
same encoded image used in SRGB and linear roles, immutable source, sharing/caps,
missing/malformed UV/MR images, failed replacement, native default/PBR lifecycle
and0 validation errors. No calibrated pixel equivalence is claimed without readback.
Normals/tangents/occlusion/emission maps, BLEND, IBL/shadows and editing stay open.

Authority: [Khronos glTF material schema](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/schema/material.pbrMetallicRoughness.schema.json).
Behavior audit: reference/SLICE_05_MR_AUDIT.md.
