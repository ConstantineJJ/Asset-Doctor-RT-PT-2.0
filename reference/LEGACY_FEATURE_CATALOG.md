# Legacy feature catalog
Legacy HEAD `17ca3549767aac78bf4ad51fa1c254c90dd15113`, branch `work/import-limit-20261115`, dirty `clean`.

Source observation only; no Legacy code/tests/GUI executed. All native scenarios UNVERIFIED.
The per-row new-product labels below are the initial inventory snapshot; current
implementation progress is maintained in docs/FEATURE_PARITY_MATRIX.csv and reports.

## IMP-01 — Open self-contained GLB/glTF

- Entry: Open / drop
- Parameters: 192 MiB source admission; two pending files
- Mutation / Undo / cancellation: Replace only accepted asset; load sequence; errors retain old document
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/loaders/GLBLoaderService.ts:107`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Open valid, truncated and oversized files; old model survives failure

## IMP-02 — Reject external resources

- Entry: All file import paths
- Parameters: external bin/images/http/file references
- Mutation / Undo / cancellation: No sibling/network grants; reject before loader
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/loaders/GLBLoaderService.ts:143`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Reject external URI even in renamed GLB

## IMP-03 — Draco decode

- Entry: Open compressed asset
- Parameters: bundled decoder path
- Mutation / Undo / cancellation: Loading only; export certification separate
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/loaders/GLBLoaderService.ts:56`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Open licensed Draco fixture offline and compare decoded counts

## IMP-04 — Meshopt decode

- Entry: Open compressed asset
- Parameters: bundled MeshoptDecoder
- Mutation / Undo / cancellation: Source preserved; decoder resource lifecycle
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/loaders/GLBLoaderService.ts:45`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Decode Meshopt fixture offline and repeat import

## IMP-05 — KTX2/BasisU decode

- Entry: Open textured asset
- Parameters: GPU support detection required
- Mutation / Undo / cancellation: Temporary probe disposed; errors explicit
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/loaders/GLBLoaderService.ts:83`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Open KTX2 fixture and test unsupported renderer

## IMP-06 — Built-in test patient

- Entry: Samples menu
- Parameters: one named sample entry
- Mutation / Undo / cancellation: Switch subject to unsaved guard
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/TopToolbar.tsx:209`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Load patient and identify intentionally broken geometry

## UI-01 — Application menu/settings/exit

- Entry: Main menu
- Parameters: language/helpers/keyboard bindings
- Mutation / Undo / cancellation: Native close guard; browser Exit disabled
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/ApplicationMenu.tsx:5`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Keyboard menu traversal and cancel unsaved close

## UI-02 — Scene tree collapse

- Entry: Toolbar tree toggle
- Parameters: default false (expanded)
- Mutation / Undo / cancellation: Presentation state; no document Undo
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/App.tsx:213`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Start application and toggle tree without losing selection

## UI-03 — Inspect tabs

- Entry: Right inspector
- Parameters: health/summary/materials/skeleton/performance
- Mutation / Undo / cancellation: Presentation only
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/InspectorPanel.tsx:383`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Switch tabs with model and report intact

## TREE-01 — Select/focus/visibility/isolate/show all

- Entry: Scene tree / viewport
- Parameters: selected UUID
- Mutation / Undo / cancellation: Presentation toggles; no persistent document history established
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/App.tsx:875`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Select nested mesh, isolate, focus, restore all

## VIEW-01 — Render channels

- Entry: Toolbar selector
- Parameters: 13 PBR/debug modes
- Mutation / Undo / cancellation: GPU presentation; original materials retained
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/TopToolbar.tsx:237`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Cycle all modes then export original authored material values

## VIEW-02 — Shading modes

- Entry: Shading control
- Parameters: hybrid/smooth/flat; default hybrid
- Mutation / Undo / cancellation: Stored preference; bounded exact smoothing
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/viewer/ShadingMode.ts:3`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Switch mode and verify source geometry unchanged

## VIEW-03 — Lighting presets

- Entry: Toolbar lighting
- Parameters: 8 presets
- Mutation / Undo / cancellation: Presentation; independent Compare setting exists
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/TopToolbar.tsx:261`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Cycle presets and check material values remain authored

## VIEW-04 — Edit key/fill/rim/ambient/exposure

- Entry: Inspector lighting
- Parameters: exposure 1; ambient .4; key 1.6; fill .6; rim 1
- Mutation / Undo / cancellation: Presentation config; no shared command Undo
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/App.tsx:195`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Change each supported parameter; inspect illumination and export

## VIEW-05 — Light bulb manipulation

- Entry: Viewport bulb
- Parameters: key light position; shadow toggle
- Mutation / Undo / cancellation: Legacy helper graph; must not be reused as new scene architecture
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/viewer/LightingManager.ts:15`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Drag near/far and verify predictable world position

## VIEW-06 — Floor themes

- Entry: Floor selector
- Parameters: grid + 11 images + 6 procedural + none
- Mutation / Undo / cancellation: Presentation texture pool owns resources
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/viewer/SurfaceCatalog.ts:14`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Cycle themes twice; no duplicate choices or growing retained textures

## VIEW-07 — Helpers and overlays

- Entry: Toolbar/settings
- Parameters: grid=true axes=true bbox=false skeleton=false origin=true
- Mutation / Undo / cancellation: Presentation; shared skeleton visibility
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/App.tsx:214`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Toggle each overlay in Doctor and Compare

## VIEW-08 — Exploded presentation

- Entry: Toolbar slider
- Parameters: 0..100 default 0
- Mutation / Undo / cancellation: Non-destructive displacement; source matrix contract
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/TopToolbar.tsx:312`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Explode and export; source transforms unchanged

## VIEW-09 — Manual turntable/autorotate

- Entry: Viewport controls
- Parameters: rotation 0; autorotate false; speed 1
- Mutation / Undo / cancellation: Legacy presentation; new owner requests removal
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/App.tsx:212`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Document as Legacy only; new app must never autorotate

## CAM-01 — Orbit/pan/zoom

- Entry: Viewport mouse
- Parameters: damping .08; distance .05..50000
- Mutation / Undo / cancellation: Presentation; key/mouse ownership must be redesigned
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/viewer/CameraController.ts:14`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Orbit/pan/zoom without document transform changes

## CAM-02 — Frame all/focus/raw bounds/presets

- Entry: Toolbar and F/Shift+F
- Parameters: perspective/front/top/right API; raw debug fit
- Mutation / Undo / cancellation: No document edits
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/TopToolbar.tsx:44`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Fit skinned/translated model without clipping

## CMP-01 — Manual Compare opening

- Entry: Viewport Compare button
- Parameters: separate modal/view state
- Mutation / Undo / cancellation: Doctor active suspended; explicit transition
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/Viewport.tsx:67`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Add two assets and switch back with state retained

## CMP-02 — Add/remove/select/solo models

- Entry: Compare slots
- Parameters: A..H; max 8; source total 8x192 MiB
- Mutation / Undo / cancellation: Separate asset records/resource disposal; no shared document Undo
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/CompareViewport.tsx:55`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Add eight, reject ninth, remove/re-add without leaks

## CMP-03 — Layout and scale matching

- Entry: Compare controls
- Parameters: grid/lineup; real/manual matching
- Mutation / Undo / cancellation: Presentation scale separate from source
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/CompareViewport.tsx:85`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Switch layout and compare known dimensions

## CMP-04 — Synchronized/independent cameras

- Entry: Compare controls
- Parameters: default true
- Mutation / Undo / cancellation: Presentation camera state
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/CompareViewport.tsx:88`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Unsync orbit one slot; sync again predictably

## CMP-05 — Compare animation sync/timeline

- Entry: Compare timeline
- Parameters: sync=true playing=false loop=true speed=1
- Mutation / Undo / cancellation: Preview only
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/CompareViewport.tsx:89`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Play/seek differing-duration clips and change sync

## CMP-06 — Open compared asset in Doctor

- Entry: Slot action
- Parameters: sourceBuffer required
- Mutation / Undo / cancellation: Rebuilds selected File then closes Compare
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/Viewport.tsx:126`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Transfer skinned asset and verify source-backed export

## DIAG-01 — Integrity of buffers/skeleton references

- Entry: Import / Rescan
- Parameters: finite data; valid indices and attribute pairs
- Mutation / Undo / cancellation: Read-only; sampled locations capped at 16
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/analysis/IntegrityAnalyzer.ts:46`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Malformed index/NaN/skin pair yields concrete evidence

## DIAG-02 — Geometry summary/tree/bounds

- Entry: Summary/tree
- Parameters: counts and node classification
- Mutation / Undo / cancellation: Read-only; authored vs presentation distinction
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/analysis/GeometryAnalyzer.ts:32`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Nested/skinned fixture counts and bounds agree with expected

## DIAG-03 — Topology

- Entry: Progressive analysis
- Parameters: edges/components/duplicate/degenerate checks
- Mutation / Undo / cancellation: Worker path; cancellation/stale guards require native exercise
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/analysis/TopologyAnalyzer.ts:3`
- Source: OBSERVED; native: UNVERIFIED; new product: PARTIAL_INDEXED_TOPOLOGY.
- Acceptance: Synthetic defect fixture gives expected issue IDs and locations

## DIAG-04 — Normals and UV

- Entry: Health panel
- Parameters: missing/zero normals and UV validity
- Mutation / Undo / cancellation: Read-only
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/analysis/NormalsAndUvAnalyzer.ts:12`
- Source: OBSERVED; native: UNVERIFIED; new product: PARTIAL_SOURCE_NORMALS.
- Acceptance: Known absent/zero data produces correct severity

## DIAG-05 — Materials/textures

- Entry: Health/materials
- Parameters: authored material values; texture dimensions/channels
- Mutation / Undo / cancellation: Read-only
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/analysis/MaterialAnalyzer.ts:5`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: PBR/alpha/missing maps fixture preserves values and reports evidence

## DIAG-06 — Skeleton/weights

- Entry: Rig inspector
- Parameters: bones/bind/skin influence metrics
- Mutation / Undo / cancellation: Read-only
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/analysis/SkeletonAnalyzer.ts:6`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Invalid sums/indices and unused bones are distinguished

## DIAG-07 — Animation diagnostics

- Entry: Health/animation list
- Parameters: tracks/durations/root motion
- Mutation / Undo / cancellation: Read-only
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/analysis/AnimationDiagnostics.ts:63`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Bad track and valid rest pose distinguish integrity vs profile advice

## DIAG-08 — Transforms/performance

- Entry: Summary/performance
- Parameters: draw calls/memory estimates/profile thresholds
- Mutation / Undo / cancellation: Estimates are not measured process RAM/VRAM
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/analysis/PerformanceAnalyzer.ts:4`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Known mesh/material counts and timing stage evidence

## DIAG-09 — Diagnostic profiles

- Entry: Profile selector
- Parameters: 7 target profiles; general default
- Mutation / Undo / cancellation: Severity interpretation only; no asset mutation
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/health/DiagnosticProfiles.ts:3`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Same geometry gets target-specific fitness findings

## DIAG-10 — Finding focus/filter/export report

- Entry: Health cards and F9
- Parameters: category/severity/technical evidence
- Mutation / Undo / cancellation: Reports do not clear unsaved model state
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/InspectorPanel.tsx:85`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Save single/all reports; cancel dialog retains model state

## HEAL-01 — Preview/apply/verify/undo

- Entry: Finding action/report
- Parameters: READY plan for current mesh/revision
- Mutation / Undo / cancellation: Transactional geometry; verify before success; preview cancellation
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/heal/SurgicalHealEngine.ts:978`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Preview no mutation; apply clears target; undo restores exact arrays

## HEAL-02 — Safe repair queue

- Entry: Health queue controls
- Parameters: max 250 passes; re-read candidates after each apply
- Mutation / Undo / cancellation: User confirmation; stop/cancel/stale/regression codes
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/heal/SafeRepairQueueRunner.ts:64`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Cancel queued repair; regression stops further apply

## HEAL-03 — Persisted repair report/history

- Entry: Health history
- Parameters: historical vs active verification distinct
- Mutation / Undo / cancellation: Storage failure explicit; history not model state
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/heal/HealReportStorage.ts:7`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Reopen stored report without treating it as current verified repair

## RIG-01 — Inspect/edit/pose modes

- Entry: Rig toolbar
- Parameters: inspect/edit/pose; initial inspect
- Mutation / Undo / cancellation: Authored rest/clips separate from transient pose
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/rig/RigEditSession.ts:12`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Pose then return to edit without corrupting rest

## RIG-02 — Joint gizmo tools and numeric edits

- Entry: Joint toolbar/fields
- Parameters: select/translate/rotate/twist; local/world; joint/end
- Mutation / Undo / cancellation: Session transaction/gesture; child follow/hold
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/RigEditorPanel.tsx:46`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Drag cancelled/committed once; undo restores rest and inverse binds

## RIG-03 — Add/rename joint and edit lengths

- Entry: Rig panel/context menu
- Parameters: length > .000001; numeric xyz step .01
- Mutation / Undo / cancellation: Rig history; terminal guides separate from deform bones
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/RigEditorPanel.tsx:63`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Add child/end, rename, length change and undo/redo/export

## RIG-04 — Delete joint/chain/skeleton

- Entry: Rig/context removal dialog
- Parameters: scope and skinning consequences
- Mutation / Undo / cancellation: Guard invalid removal; snapshots retain resources
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/rig/RigRemoval.ts:5`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Delete with influences, undo exact bind/animation restoration

## RIG-05 — Fit rig to surface proposals

- Entry: Rig fitting panel
- Parameters: select suggested; shift threshold .35 in fitting core
- Mutation / Undo / cancellation: Worker preview/cancel; apply selected revision guarded
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/RigFittingPanel.tsx:14`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Analyze, cancel; reject stale/uncertain proposals; undo applied fit

## RIG-06 — Undo/redo/reset original

- Entry: Rig panel/Ctrl+Z
- Parameters: 128 MiB history budget; retained baseline
- Mutation / Undo / cancellation: Separate Legacy history from Heal; no unified new commands yet
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/rig/RigEditSession.ts:81`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Undo and redo after branching; restore original; budget failure no mutation

## ANIM-01 — Autoplay imported first clip

- Entry: Successful import
- Parameters: clips.length > 0
- Mutation / Undo / cancellation: Current source starts playback; native unverified
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/App.tsx:600`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Import animated/static files then switch models

## ANIM-02 — Clip search/selection

- Entry: Animation list
- Parameters: case-insensitive substring; busy disables selection
- Mutation / Undo / cancellation: Edit mode selects without motion
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/AnimationClipList.tsx:7`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Search/select long named clip in edit and inspect modes

## ANIM-03 — Play/pause/stop/step/seek/loop/speed

- Entry: Bottom timeline
- Parameters: .25/.5/1/2x; seek 0..1 step .001; loop=true
- Mutation / Undo / cancellation: Preview only; stop resets
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/AnimationTimeline.tsx:53`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Boundary seek/step/loop on short and root-motion clips

## ANIM-04 — Rename and retime clip

- Entry: Clip editor
- Parameters: name max 256; duration > .000001
- Mutation / Undo / cancellation: Rig command history; preview speed not exported duration
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/RigEditorPanel.tsx:70`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Rename/retime export and reimport keys/duration

## ANIM-05 — Root-motion measurement/overlay

- Entry: Timeline Path toggle
- Parameters: translation/rotation vector
- Mutation / Undo / cancellation: Presentation only
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/components/AnimationTimeline.tsx:76`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Known root-motion clip matches measured displacement

## EXP-01 — Repaired GLB export and re-open verification

- Entry: Toolbar / Heal report
- Parameters: verified active repairs; one scene; reachable nodes
- Mutation / Undo / cancellation: Source-preserving patch; cancellation; verified reimport
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/export/RepairedExportService.ts:127`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Apply repair export/reimport; compare hierarchy/materials/skin/clips

## EXP-02 — Rig-edited/combined GLB export

- Entry: Rig verify/save
- Parameters: source patch with authored rest and clips
- Mutation / Undo / cancellation: Revision/geometry/bind guards; worker cancellation
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/rig/RigExportService.ts:16`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Edit rig + Heal, export/reimport or explicitly block unsupported combination

## EXP-03 — Save cancellation/failure/close protection

- Entry: Native Save and application close
- Parameters: selected output path; GLB only
- Mutation / Undo / cancellation: Only completed write clears protection; browser download not proof
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/platform/FileIO.ts:92`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Cancel Save, inject write failure, then cancel unsaved close

## SYS-01 — Keyboard shortcuts/conflicts/reset

- Entry: Settings
- Parameters: 12 actions; reserved chords; no mouse bindings
- Mutation / Undo / cancellation: Stored adapter validates defaults/conflicts
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/platform/Shortcuts.ts:3`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Assign conflict/reserved; reset; ignore text/modal typing

## SYS-02 — Language

- Entry: Settings/toolbar
- Parameters: English fallback; en/ru
- Mutation / Undo / cancellation: localStorage reads/writes need fault validation
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/i18n/index.tsx:18`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Throw storage reads/writes; UI remains available in new app

## SYS-03 — System log capture/save/clear

- Entry: F8/log control
- Parameters: enabled true; verbose false; 600 entries; 2 MiB
- Mutation / Undo / cancellation: Sanitized structured context; native disk rotation separate
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/platform/SystemLog.ts:6`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Toggle verbose, clear and save; no secret/path disclosure

## SYS-04 — Startup trace and native shell

- Entry: Native bootstrap
- Parameters: 48 events per trace
- Mutation / Undo / cancellation: Stage only; historical report flags unbounded file count
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src-tauri/src/startup_trace.rs:56`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Fail before UI; inspect bounded trace and retention behavior

## SYS-05 — Resource budgets and worker cancellation

- Entry: Import/analysis/Undo
- Parameters: packet32/worker64/history128 MiB
- Mutation / Undo / cancellation: Independent binary budgets not whole RAM/VRAM
- Errors: See acceptance and referenced implementation; native fault exercise pending
- Evidence: `src/performance/MemoryBudget.ts:9`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Reject overbudget input without replacing current document

## REPAIR-01 — remove-degenerate-triangles

- Entry: Finding repair / queue
- Parameters: Current mesh; CONDITIONAL risk; READY preview required
- Mutation / Undo / cancellation: Preview/apply/undo/verify; index-only or geometry export patch per registry
- Errors: BLOCKED unsupported geometry; rollback/regression must be exercised
- Evidence: `src/heal/framework/RepairRegistry.ts:8`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Synthetic target for remove-degenerate-triangles: preview no mutation; apply removes defect; undo exact arrays; export/reimport

## REPAIR-02 — remove-unreferenced-vertices

- Entry: Finding repair / queue
- Parameters: Current mesh; CONDITIONAL risk; READY preview required
- Mutation / Undo / cancellation: Preview/apply/undo/verify; index-only or geometry export patch per registry
- Errors: BLOCKED unsupported geometry; rollback/regression must be exercised
- Evidence: `src/heal/framework/RepairRegistry.ts:27`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Synthetic target for remove-unreferenced-vertices: preview no mutation; apply removes defect; undo exact arrays; export/reimport

## REPAIR-03 — recalculate-normals

- Entry: Finding repair / queue
- Parameters: Current mesh; CONDITIONAL risk; READY preview required
- Mutation / Undo / cancellation: Preview/apply/undo/verify; index-only or geometry export patch per registry
- Errors: BLOCKED unsupported geometry; rollback/regression must be exercised
- Evidence: `src/heal/framework/RepairRegistry.ts:46`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Synthetic target for recalculate-normals: preview no mutation; apply removes defect; undo exact arrays; export/reimport

## REPAIR-04 — merge-exact-duplicate-vertices

- Entry: Finding repair / queue
- Parameters: Current mesh; CONDITIONAL risk; READY preview required
- Mutation / Undo / cancellation: Preview/apply/undo/verify; index-only or geometry export patch per registry
- Errors: BLOCKED unsupported geometry; rollback/regression must be exercised
- Evidence: `src/heal/framework/RepairRegistry.ts:66`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Synthetic target for merge-exact-duplicate-vertices: preview no mutation; apply removes defect; undo exact arrays; export/reimport

## REPAIR-05 — normalize-skin-weights

- Entry: Finding repair / queue
- Parameters: Current mesh; CONDITIONAL risk; READY preview required
- Mutation / Undo / cancellation: Preview/apply/undo/verify; index-only or geometry export patch per registry
- Errors: BLOCKED unsupported geometry; rollback/regression must be exercised
- Evidence: `src/heal/framework/RepairRegistry.ts:85`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Synthetic target for normalize-skin-weights: preview no mutation; apply removes defect; undo exact arrays; export/reimport

## REPAIR-06 — remove-exact-duplicate-triangles

- Entry: Finding repair / queue
- Parameters: Current mesh; CONDITIONAL risk; READY preview required
- Mutation / Undo / cancellation: Preview/apply/undo/verify; index-only or geometry export patch per registry
- Errors: BLOCKED unsupported geometry; rollback/regression must be exercised
- Evidence: `src/heal/framework/RepairRegistry.ts:104`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Synthetic target for remove-exact-duplicate-triangles: preview no mutation; apply removes defect; undo exact arrays; export/reimport

## REPAIR-07 — consolidate-duplicate-skin-influences

- Entry: Finding repair / queue
- Parameters: Current mesh; CONDITIONAL risk; READY preview required
- Mutation / Undo / cancellation: Preview/apply/undo/verify; index-only or geometry export patch per registry
- Errors: BLOCKED unsupported geometry; rollback/regression must be exercised
- Evidence: `src/heal/framework/RepairRegistry.ts:123`
- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.
- Acceptance: Synthetic target for consolidate-duplicate-skin-influences: preview no mutation; apply removes defect; undo exact arrays; export/reimport
