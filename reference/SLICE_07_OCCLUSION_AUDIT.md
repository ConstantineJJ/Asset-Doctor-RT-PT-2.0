# Occlusion preview review — 2026-11-15

Read-only legacy material/texture analyzers count aoMap alongside other material
resources. Existing shading/debug-view inventory is distinct from this material
sampling slice; no diagnostic, repair or legacy rendering implementation is ported.

Khronos glTF independently defines a linear R occlusion channel and strength blend,
affecting indirect/ambient lighting rather than direct illumination. The new preview
has only approximate fill, so apply occlusion solely there. Shared ORM data must not
be re-decoded through SRGB or consume a duplicate linear upload. ADR-0019 defines
its independent UV/presentation/admission boundary; no general lighting parity claim.
