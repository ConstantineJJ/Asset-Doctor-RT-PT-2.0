# Metallic/roughness behavior review — 2026-11-15

Read-only source reference: legacy MaterialAnalyzer.ts:25-26,40-41 records material
metal/roughness values and map slots. TextureAnalyzer.ts:33-34 and
GeometryAnalyzer.ts:162-163 count both maps; TopToolbar.tsx:248 identifies the
roughness G-channel inspection mode. No legacy implementation is reused/executed.
This slice adds material sampling only, not those diagnostic/debug-view operations.

Khronos glTF material schema independently defines linear B/G sampling multiplied
by material factors, with unit G/B fallback. R/A are irrelevant to this channel.
Base color has a different transfer function, so sharing one source image must
not silently force data maps through SRGB decoding. ADR-0017 defines independent
CPU identity, image encoding admission and UV selection. No normal-map or complete
material fidelity acceptance is implied by the owner's latest PBR screenshot.
