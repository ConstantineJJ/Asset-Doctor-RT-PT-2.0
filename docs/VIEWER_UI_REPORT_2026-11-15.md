# Native viewer shell — 2026-11-15

The engineering prototype now has visible Open/Fit/tree/summary controls, a
loading/cancel status bar and Russian UI labels. Vulkan draws geometry into the
remaining central rectangle and egui paint data into the same render pass. No
WebView, eframe or second GPU backend was added. Domain/formats remain independent
of UI/GPU types. Bounded contracts/evidence: ADR-0014 and SLICE_02_VIEWER_UI_AUDIT.

## Behavior delivered

- Open uses the same native dialog/worker as Ctrl+O and drop. Buttons emit actions
  after UI layout; native dialogs open only after the acquired image is submitted.
- Fit reframes the model. Tree is hidden at startup (REQ-TREE-001); Summary is
  initially visible. In compact windows optional side panels hide and compact
  controls retain Open/Fit/cancel; H still opens Help.
- The active scene hierarchy is flattened iteratively once per accepted document,
  with virtualized fixed-height rows and bounded display names. Select a row to
  inspect its name, child/primitive-reference counts and original local matrix.
  Selection belongs to a document; successful replacement clears it, failed
  replacement leaves it unchanged. This is inspection, without geometry highlight,
  visibility/isolate, viewport picking or editing.
- Summary labels source size/document nodes separately from expanded display
  vertex/triangle/primitive counts and indexed world bounds in metres. It never
  presents those counts as diagnostics or a whole-process memory measurement.
- Toolbar/panel input cannot initiate camera dragging; a viewport drag retains
  ownership while crossing panels. Focus loss releases drag state. Hit-testing
  uses full egui pixels-per-point including native DPI and UI zoom; pan uses scene
  height and camera aspect uses the physical scene rectangle. Open/Fit/H remain
  available after button focus; text-edit focus suppresses ordinary shortcuts.
- Empty viewport shows an opening hint. Loading has spinner/cancel. Native errors
  remain available, with a bounded one-line status error and tooltip. Cancel and
  failed imports preserve the previous asset. Sources are never rewritten.
- H/Help contains controls and embedded copyright/license notices for unchanged
  bundled fonts. Product license is still the owner's decision.

The static import limitations from STATIC_VIEWER_REPORT remain: opaque triangle
GLB/BIN only, base-color face lighting/depth, no textures/PBR/animation/skins/export.

## Verification

| Layer | Result |
|---|---|
| Final fmt, all-target clippy with denied warnings, Windows release build | PASS |
| Final workspace tests | PASS: 30 (9 desktop, 5 document, 9 formats, 7 presentation/UI-rectangle) |
| Dependency policy | advisories/bans/licenses/sources PASS; exact existing miniz/syn exceptions retained |
| Native empty UI lifecycle | PASS: 15 presents, resize/minimize/restore/real WM_CLOSE, validation enabled, 0 errors |
| Final release import/UI + failed replacement + lifecycle | PASS: authored Twin_Cubes, 15 mesh presents, previous CPU/GPU tags retained on malformed file, 0 validation errors |
| Synchronization setting | Import run requested child VK_LAYER_VALIDATE_SYNC=1; effective layer setting not separately queried |
| Visual polish, physical panel clicks/keyboard traversal, screen reader, multi-monitor DPI, compact high-zoom UI | Not certified by the automatic GPU run; owner review remains open |

The owner explicitly authorized a 20–30-second automatic window check after the
earlier PC handback. Only self-closing app smoke windows were used; no computer
input or screen capture was used. No yellow capture border was introduced. All
test windows are closed. Empty UI smoke preceded the last bounded status-label
change; final import/UI/lifecycle smoke ran after it. Logs are local Git-ignored
artifacts/viewer-ui-*.log. No public CI/push/release was performed.

Headless checks cover compact/normal layout, pointer ownership, full UI-zoom
hit-testing, actual Open-button press/release action, document-scoped hierarchy
selection, a 10,000-level nonrecursive graph and physical scene clipping. Pointer
ownership and zoom regressions were observed failing before their fixes. The
review also caught mismatched virtual row height; fixed-height truncated rows now
match the scroll stride. A second source review found no remaining concrete
blocker in its inspected scope. This is not a large-scene UI performance, fuzzing,
peak-memory or native accessibility certification.

## Prior owner acceptance, kept separate

Before this shell change, the owner reported all then-current viewer controls and
error messages working. User-supplied screenshots show successful static viewing:

| Asset | Display triangles |
|---|---:|
| Yellow Car T 360K tris | 350,216 |
| Tank_cherry_black | 1,158,996 |
| Violet_Vanguard2 | 1,559,826 |
| Sunfire_APC_soft | 1,568,489 |

The three latter screenshots show 142 FPS/~7 ms at capture time. This is owner
evidence of those displayed cases, not an uncapped benchmark or per-process VRAM
measurement. Earlier refused files displayed a skin/animation compatibility
error; a file-size limit was not established as their cause. The new shell has not
yet been manually accepted by the owner.

## Remaining decisions

ADR-0004 remains open for the permanent UI backend, including a native Win32
comparison, accessibility/text/input acceptance and real dependency footprint.
Current release EXE is 7,024,128 bytes; the previous static-only prototype was
1,679,872 bytes. Whole-process RAM/VRAM, complex text/large-tree responsiveness,
allocation-failure cleanup and device-loss behavior remain unprofiled/uninjected.
Compatible swapchain recreation retains UI textures; a surface-format change
currently fails explicitly with a restart request. HDR/general color-mode
recreation remains open. UI choices are session-only and are not persisted yet.

Next: owner review of the shell and relevant material/normal/texture contracts
before the PBR increment. Full Legacy parity, diagnostics/repair, rig/animation,
Compare, undo/export and RT/PT remain future work.
