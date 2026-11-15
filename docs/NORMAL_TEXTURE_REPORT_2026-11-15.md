# Tangent-space normal preview — 2026-11-15

Owner authorized autonomous development and computer-based self-verification without
per-increment manual acceptance. ADR-0018/SLICE_06 define the bounded normal-map
contract; full Mikk/material fidelity is explicitly outside the current preview.

## Implemented

- Preserve finite normalTexture.scale, selected UVs, linear embedded PNG/JPEG and
  optional matching F32VEC4 source tangents with ±1W. Snapshot derives tangents with
  worldlinear transform/Gram-Schmidt against inverse-transpose normals, flipping W
  for negative determinant. Source bytes/attributes/material values remain unchanged.
- Missing/unusable authored frames use a zero sentinel. Shader derives a frame from
  both position and selected UV derivatives, preserving mirrored UV orientation.
  Zero/near-parallel/nonfinite/degenerate frames retain the base normal. This is a
  derivative approximation, not MikkTSpace; baked seams may differ. Missing source
  normals retain the flat/view-facing geometric frame of the two-sided preview.
- Normal RGB is linear, reconstructed to[-1,1], XY multiplied by source scale then
  normalized; alpha ignored. Derivatives and implicit texture sampling occur before
  alpha discard. The independent normal switch only affects PBR and draws with a
  real normal map; the shared linear white fallback is never used as a sampled
  normal. Source normal scale occupies material.w in the unchanged128-byte push.
- Third compatible descriptor set; normal/MR source sharing reuses the same linear
  GPU upload. Seven vertex streams: position12+normal12+tangent16+threeUV24+color16
  =80bytes/vertex+indices. Existing image/variant/mip/node/source/draw caps remain.
  Mip values are admitted RGBA estimates, not whole-process or measured VRAM peaks.

Occlusion/emission/environment/shadows, BLEND, culling parity, Mikk generation,
editing, skin/animation and export remain open. Normal resources now undergo strict
admission even in base mode; unsupported/bad maps fail without replacing the asset.

## Checks

| Check | Evidence |
|---|---|
| Workspace |54PASS: desktop12/document12/formats20/renderer10 |
| fmt/clippy all-target warnings denied/release build | PASS; EXE7,718,400bytes |
| cargo-deny advisories/bans/licenses/sources | PASS; no dependency/policy addition |
| Normal_Checker PBR on/off/default |15mesh presents each,normal15/0/0;1linear image shared with MR,20mipbytes; bad normal replacement preserved; lifecycle0errors |
| Prior MR, color/MASK, factor and plain regressions |15mesh presents each; failed replacement retained; lifecycle0errors |
| RedCat normal native |31471vertices/46731triangles/3images;268435452mipbytes;15normal presents; failure+lifecycle0errors; originalSHA256 unchanged |
| Mini-bus6x normal native |26327vertices/37128triangles/3images;268435452mipbytes;15normal presents; failure+lifecycle0errors; originalSHA256 unchanged |
| Violet_Vanguard2 normal native |852516vertices/1559826triangles/3images;201326588mipbytes;15normal presents; failure+lifecycle0errors; originalSHA256 unchanged |
| Native visual/input fixture and RedCat | Visible on/off surface response, scale0 case unchanged, orbit/maximize/native close; both sessions0validation errors |

Import smokes requested child VK_LAYER_VALIDATE_SYNC=1; effective setting was not
independently queried. All native checks used actual validation-enabled app windows.
Authorized interactive checks displayed window captures through the tool; no user
asset or screenshots were copied into the repository. All test windows closed.

The OS picker opened via Ctrl+O and cancelled via Escape while preserving the scene.
AX/screenshot filename focus/set did not succeed through automation. No automated
file-commit pass or application defect is inferred from that helper limitation.
Physical Open workflow remains separate from successful CLI imports.

Tangent mirror/nonuniform regression was RED then GREEN; source/layout/W/scale/UV/
budget tests include no-normal/singular/parallel cases. Actual UI checkbox/reset
tests keep normal/MR/color independent. Source-only reviews found no remaining
blocker. Native counters are execution evidence, and visual inspection is qualitative:
no quantitative pixel/seam/LOD/Mikk comparison, allocator/device-loss injection,
memory peak or performance benchmark has been completed.

## Reproduce

Open tests/fixtures/Normal_Checker.glb in PBR. Four cases: authored tangent; mirrored
nonuniform derivative frame; degenerate UV with scale0; no authored normal with
negative scale. UV2 differs from MR UV1, one numeric linear PNG serves both roles.
Bad_Normal_Image references malformed PNG only through normal maps. Provenance
and hashes: tools/make_normal_fixtures.py and fixture manifest.

`tools/import-smoke.ps1 -Scene normal -Pbr -SynchronizationValidation`; add
`-NoNormalMaps` for disabled sampling or omit `-Pbr` for default mode. Developer
launch accepts `--no-normal-maps`. Native logs remain Git-ignored under artifacts/:
release-import-smoke*normal*, redcat/minibus/vanguard-normal-smoke, normal-ui-visual
and redcat-normal-visual. Local development only; no remote publication.
