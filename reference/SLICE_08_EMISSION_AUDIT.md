# Emission preview review — 2026-11-15

Read-only legacy MaterialAnalyzer.ts:20,28,45 records emissive color/map;
GeometryAnalyzer.ts:165 and TextureAnalyzer.ts:36 count its resource. Those
diagnostics are distinct from this preview slice; no legacy source/architecture
or runtime is copied, executed or modified.

New independent contract follows core glTF: linear RGB factor multiplied by SRGB
texture, alpha ignored, default factor0 and white missing texture. Add emission
before existing display exposure/mapping without AO/direct-light attenuation.
It is surface emission only, without bloom/GI or material extension parity.
ADR-0020 provides portable material/frame capacity before the channel is added.
