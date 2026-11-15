# Source occlusion preview — 2026-11-15

After completing normal maps, development continued under the owner's autonomous
development/self-test instruction. ADR-0019/SLICE_07 contract AO as a material-map
contribution to the existing approximate fill, without adding an owner review gate.

## Behavior and boundaries

- Preserve embedded PNG/JPEG occlusionTexture, declared UV and source strength,
  default1/range[0,1]. Linear R is blended as1-strength+strength*R; G/B/A ignored.
  Only approximate fill is multiplied. Direct diffuse/specular, base-color sampling,
  source factors, coverage and default mode retain their previous behavior.
- **Окклюзия · AO** toggles sampling independently of color/MR/normal switches;
  illumination reset does not alter it. No source/material/geometry edit or saved
  setting. This is source AO, not ray-traced occlusion or physically calibrated IBL.
- UV stream8bytes adds88bytes/vertex+indices overall. Linear MR/AO/normal can share
  source pixels/upload. Existing sampled-variant/mip/side/count caps apply even in
  base mode; malformed AO now explicitly rejects replacement. Four descriptor sets
  reach the guaranteed Vulkan minimum; push stays128bytes with strength inlighting.w.
  Emission requires a binding redesign before any fifth set/additional push data.

## Verification

| Check | Result |
|---|---|
| Workspace |58PASS: desktop13/document13/formats22/renderer10 |
| fmt/clippy all-target denied warnings/Windows release | PASS; desktop7,727,104bytes |
| cargo-deny advisories/bans/licenses/sources | PASS; no package/policy addition |
| AO_Checker PBR on/off/default |15mesh presents each,AO15/0/0; source image shared color/MR/AO,2format uploads40mipbytes; failed AO replacement preserved; lifecycle0errors |
| Normal, color/MASK and empty UI regression | PASS; normal toggle path remains active where applicable;0errors |
| Final heavy Vanguard |852516vertices/1559826triangles;normal15/AO0(no source AO); failed AO replacement preserved; lifecycle0errors; originalSHA256 unchanged |
| Native interactive AO |key0/fill1 modulation visible;strength0 unchanged; key8/fill0 looked unchanged under AO toggle; normal close,0validation errors |

Native import checks resized, minimized, restored and closed through the OS. Child
VK_LAYER_VALIDATE_SYNC=1 requested; effective setting not independently queried.
Interactive capture/input was explicitly authorized; window captures displayed by
the tool, none committed. All windows closed. Logs remain Git-ignored under
artifacts/release-import-smoke*ao*.*, ao-ui-visual.*, vanguard-ao-regression.*.

Malformed AO-only image regression failed when the map was ignored, then passed
after strict admission. Source tests include strength0/0.5/1/default1, out-of-range/
nonfinite values, independent UV2, missing UV, CPU/linear-upload sharing and source
preservation. Actual UI checkbox/reset tests keep channels independent. Source-only
CPU/GPU review found no remaining blocker. Native counters and qualitative visual
comparison are not quantitative pixel/LOD/BRDF validation or a performance/memory
benchmark. Device-loss/allocation injection remains open.

## Reproduce

Open tests/fixtures/AO_Checker.glb in PBR. Four spheres have source strength1/0.5/0/
default1; UV2 differs from MR UV1 and base UV0. One numeric PNG is shared across
color/MR/AO roles. Set key0/fill1 to isolate fill, then key8/fill0 for direct-only
comparison. Bad_AO_Image references malformed PNG only through AO.

`tools/import-smoke.ps1 -Scene ao -Pbr -SynchronizationValidation`; add `-NoAoMaps`
for disabled sampling or omit `-Pbr` for default mode. Developer CLI also supports
`--no-ao-maps`. Provenance/hashes in tools/make_ao_fixtures.py and fixture manifest.
No legacy implementation/report changes, external asset copies or remote publication.
