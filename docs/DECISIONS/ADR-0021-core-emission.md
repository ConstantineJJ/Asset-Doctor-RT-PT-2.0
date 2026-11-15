# ADR-0021 — Core emissive material preview

Status: ACCEPTED for static glTF core materials, 2026-11-15.

Preserve emissiveFactor RGB linear[0,1], default0, and optional embedded PNG/JPEG
emissiveTexture with declared independent UV. Emissive RGB samples SRGB; alpha is
ignored. Missing image samples white. Share source image/texture and SRGB upload
with base color where identities agree. Referenced emission images undergo strict
admission even when source factor0 or default mode; failures preserve current model.

Direct PBR adds factor*sampledRGB before exposure/Reinhard, independently of key,
fill, AO, base-color sampling and source base alpha. MASK coverage still comes only
from the base material. Session emission switch toggles the whole contribution;
lighting reset preserves it. No bloom, emission-strength extensions, GI or light
cast onto other objects. Required extensions remain rejected; optional strength
extension use receives an explicit unsupported note. No material/source edit.

ADR-0020 remains2sets/96byte frame/64byte push. Add fifth material image/sampler
pair(bindings8/9), source emissionRGB in reserved push vec4, flag64 for PBR+switch+
nonzero source factor. Material tuple cap4096 stays. New independent UV stream
makes96bytes/vertex+indices; image/encoding caps unchanged, including shared use.

Acceptance: factor-only/default0, textured SRGB/UV2, shared base/emissive identity,
alpha ignored, malformed emission-only image, factor range/UV/cap/source guards,
independent switch/reset, native lights-off emissive visual, on/off/default and
prior material/lifecycle/failure checks with0validation errors. Quantitative pixel,
bloom/HDR/GI and full material-extension fidelity remain open.

Source: [Khronos core material schema](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/schema/material.schema.json).
Legacy behavior reference: reference/SLICE_08_EMISSION_AUDIT.md.
