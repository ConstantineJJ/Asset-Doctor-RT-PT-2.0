# Legacy workflows

Legacy HEAD `17ca3549767aac78bf4ad51fa1c254c90dd15113`, branch `work/import-limit-20261115`, dirty `clean`.

Source observation only; no Legacy code/tests/GUI executed. All native scenarios UNVERIFIED.

1. **IMP-01 Open self-contained GLB/glTF** — Open / drop; Open valid, truncated and oversized files; old model survives failure. Evidence `src/loaders/GLBLoaderService.ts:107`. UNVERIFIED native.

2. **IMP-02 Reject external resources** — All file import paths; Reject external URI even in renamed GLB. Evidence `src/loaders/GLBLoaderService.ts:143`. UNVERIFIED native.

3. **IMP-03 Draco decode** — Open compressed asset; Open licensed Draco fixture offline and compare decoded counts. Evidence `src/loaders/GLBLoaderService.ts:56`. UNVERIFIED native.

4. **IMP-04 Meshopt decode** — Open compressed asset; Decode Meshopt fixture offline and repeat import. Evidence `src/loaders/GLBLoaderService.ts:45`. UNVERIFIED native.

5. **IMP-05 KTX2/BasisU decode** — Open textured asset; Open KTX2 fixture and test unsupported renderer. Evidence `src/loaders/GLBLoaderService.ts:83`. UNVERIFIED native.

6. **IMP-06 Built-in test patient** — Samples menu; Load patient and identify intentionally broken geometry. Evidence `src/components/TopToolbar.tsx:209`. UNVERIFIED native.

7. **UI-01 Application menu/settings/exit** — Main menu; Keyboard menu traversal and cancel unsaved close. Evidence `src/components/ApplicationMenu.tsx:5`. UNVERIFIED native.

8. **UI-02 Scene tree collapse** — Toolbar tree toggle; Start application and toggle tree without losing selection. Evidence `src/App.tsx:213`. UNVERIFIED native.

9. **UI-03 Inspect tabs** — Right inspector; Switch tabs with model and report intact. Evidence `src/components/InspectorPanel.tsx:383`. UNVERIFIED native.

10. **TREE-01 Select/focus/visibility/isolate/show all** — Scene tree / viewport; Select nested mesh, isolate, focus, restore all. Evidence `src/App.tsx:875`. UNVERIFIED native.

11. **VIEW-01 Render channels** — Toolbar selector; Cycle all modes then export original authored material values. Evidence `src/components/TopToolbar.tsx:237`. UNVERIFIED native.

12. **VIEW-02 Shading modes** — Shading control; Switch mode and verify source geometry unchanged. Evidence `src/viewer/ShadingMode.ts:3`. UNVERIFIED native.

13. **VIEW-03 Lighting presets** — Toolbar lighting; Cycle presets and check material values remain authored. Evidence `src/components/TopToolbar.tsx:261`. UNVERIFIED native.

14. **VIEW-04 Edit key/fill/rim/ambient/exposure** — Inspector lighting; Change each supported parameter; inspect illumination and export. Evidence `src/App.tsx:195`. UNVERIFIED native.

15. **VIEW-05 Light bulb manipulation** — Viewport bulb; Drag near/far and verify predictable world position. Evidence `src/viewer/LightingManager.ts:15`. UNVERIFIED native.

16. **VIEW-06 Floor themes** — Floor selector; Cycle themes twice; no duplicate choices or growing retained textures. Evidence `src/viewer/SurfaceCatalog.ts:14`. UNVERIFIED native.

17. **VIEW-07 Helpers and overlays** — Toolbar/settings; Toggle each overlay in Doctor and Compare. Evidence `src/App.tsx:214`. UNVERIFIED native.

18. **VIEW-08 Exploded presentation** — Toolbar slider; Explode and export; source transforms unchanged. Evidence `src/components/TopToolbar.tsx:312`. UNVERIFIED native.

19. **VIEW-09 Manual turntable/autorotate** — Viewport controls; Document as Legacy only; new app must never autorotate. Evidence `src/App.tsx:212`. UNVERIFIED native.

20. **CAM-01 Orbit/pan/zoom** — Viewport mouse; Orbit/pan/zoom without document transform changes. Evidence `src/viewer/CameraController.ts:14`. UNVERIFIED native.

21. **CAM-02 Frame all/focus/raw bounds/presets** — Toolbar and F/Shift+F; Fit skinned/translated model without clipping. Evidence `src/components/TopToolbar.tsx:44`. UNVERIFIED native.

22. **CMP-01 Manual Compare opening** — Viewport Compare button; Add two assets and switch back with state retained. Evidence `src/components/Viewport.tsx:67`. UNVERIFIED native.

23. **CMP-02 Add/remove/select/solo models** — Compare slots; Add eight, reject ninth, remove/re-add without leaks. Evidence `src/components/CompareViewport.tsx:55`. UNVERIFIED native.

24. **CMP-03 Layout and scale matching** — Compare controls; Switch layout and compare known dimensions. Evidence `src/components/CompareViewport.tsx:85`. UNVERIFIED native.

25. **CMP-04 Synchronized/independent cameras** — Compare controls; Unsync orbit one slot; sync again predictably. Evidence `src/components/CompareViewport.tsx:88`. UNVERIFIED native.

26. **CMP-05 Compare animation sync/timeline** — Compare timeline; Play/seek differing-duration clips and change sync. Evidence `src/components/CompareViewport.tsx:89`. UNVERIFIED native.

27. **CMP-06 Open compared asset in Doctor** — Slot action; Transfer skinned asset and verify source-backed export. Evidence `src/components/Viewport.tsx:126`. UNVERIFIED native.

28. **DIAG-01 Integrity of buffers/skeleton references** — Import / Rescan; Malformed index/NaN/skin pair yields concrete evidence. Evidence `src/analysis/IntegrityAnalyzer.ts:46`. UNVERIFIED native.

29. **DIAG-02 Geometry summary/tree/bounds** — Summary/tree; Nested/skinned fixture counts and bounds agree with expected. Evidence `src/analysis/GeometryAnalyzer.ts:32`. UNVERIFIED native.

30. **DIAG-03 Topology** — Progressive analysis; Synthetic defect fixture gives expected issue IDs and locations. Evidence `src/analysis/TopologyAnalyzer.ts:3`. UNVERIFIED native.

31. **DIAG-04 Normals and UV** — Health panel; Known absent/zero data produces correct severity. Evidence `src/analysis/NormalsAndUvAnalyzer.ts:12`. UNVERIFIED native.

32. **DIAG-05 Materials/textures** — Health/materials; PBR/alpha/missing maps fixture preserves values and reports evidence. Evidence `src/analysis/MaterialAnalyzer.ts:5`. UNVERIFIED native.

33. **DIAG-06 Skeleton/weights** — Rig inspector; Invalid sums/indices and unused bones are distinguished. Evidence `src/analysis/SkeletonAnalyzer.ts:6`. UNVERIFIED native.

34. **DIAG-07 Animation diagnostics** — Health/animation list; Bad track and valid rest pose distinguish integrity vs profile advice. Evidence `src/analysis/AnimationDiagnostics.ts:63`. UNVERIFIED native.

35. **DIAG-08 Transforms/performance** — Summary/performance; Known mesh/material counts and timing stage evidence. Evidence `src/analysis/PerformanceAnalyzer.ts:4`. UNVERIFIED native.

36. **DIAG-09 Diagnostic profiles** — Profile selector; Same geometry gets target-specific fitness findings. Evidence `src/health/DiagnosticProfiles.ts:3`. UNVERIFIED native.

37. **DIAG-10 Finding focus/filter/export report** — Health cards and F9; Save single/all reports; cancel dialog retains model state. Evidence `src/components/InspectorPanel.tsx:85`. UNVERIFIED native.

38. **HEAL-01 Preview/apply/verify/undo** — Finding action/report; Preview no mutation; apply clears target; undo restores exact arrays. Evidence `src/heal/SurgicalHealEngine.ts:978`. UNVERIFIED native.

39. **HEAL-02 Safe repair queue** — Health queue controls; Cancel queued repair; regression stops further apply. Evidence `src/heal/SafeRepairQueueRunner.ts:64`. UNVERIFIED native.

40. **HEAL-03 Persisted repair report/history** — Health history; Reopen stored report without treating it as current verified repair. Evidence `src/heal/HealReportStorage.ts:7`. UNVERIFIED native.

41. **RIG-01 Inspect/edit/pose modes** — Rig toolbar; Pose then return to edit without corrupting rest. Evidence `src/rig/RigEditSession.ts:12`. UNVERIFIED native.

42. **RIG-02 Joint gizmo tools and numeric edits** — Joint toolbar/fields; Drag cancelled/committed once; undo restores rest and inverse binds. Evidence `src/components/RigEditorPanel.tsx:46`. UNVERIFIED native.

43. **RIG-03 Add/rename joint and edit lengths** — Rig panel/context menu; Add child/end, rename, length change and undo/redo/export. Evidence `src/components/RigEditorPanel.tsx:63`. UNVERIFIED native.

44. **RIG-04 Delete joint/chain/skeleton** — Rig/context removal dialog; Delete with influences, undo exact bind/animation restoration. Evidence `src/rig/RigRemoval.ts:5`. UNVERIFIED native.

45. **RIG-05 Fit rig to surface proposals** — Rig fitting panel; Analyze, cancel; reject stale/uncertain proposals; undo applied fit. Evidence `src/components/RigFittingPanel.tsx:14`. UNVERIFIED native.

46. **RIG-06 Undo/redo/reset original** — Rig panel/Ctrl+Z; Undo and redo after branching; restore original; budget failure no mutation. Evidence `src/rig/RigEditSession.ts:81`. UNVERIFIED native.

47. **ANIM-01 Autoplay imported first clip** — Successful import; Import animated/static files then switch models. Evidence `src/App.tsx:600`. UNVERIFIED native.

48. **ANIM-02 Clip search/selection** — Animation list; Search/select long named clip in edit and inspect modes. Evidence `src/components/AnimationClipList.tsx:7`. UNVERIFIED native.

49. **ANIM-03 Play/pause/stop/step/seek/loop/speed** — Bottom timeline; Boundary seek/step/loop on short and root-motion clips. Evidence `src/components/AnimationTimeline.tsx:53`. UNVERIFIED native.

50. **ANIM-04 Rename and retime clip** — Clip editor; Rename/retime export and reimport keys/duration. Evidence `src/components/RigEditorPanel.tsx:70`. UNVERIFIED native.

51. **ANIM-05 Root-motion measurement/overlay** — Timeline Path toggle; Known root-motion clip matches measured displacement. Evidence `src/components/AnimationTimeline.tsx:76`. UNVERIFIED native.

52. **EXP-01 Repaired GLB export and re-open verification** — Toolbar / Heal report; Apply repair export/reimport; compare hierarchy/materials/skin/clips. Evidence `src/export/RepairedExportService.ts:127`. UNVERIFIED native.

53. **EXP-02 Rig-edited/combined GLB export** — Rig verify/save; Edit rig + Heal, export/reimport or explicitly block unsupported combination. Evidence `src/rig/RigExportService.ts:16`. UNVERIFIED native.

54. **EXP-03 Save cancellation/failure/close protection** — Native Save and application close; Cancel Save, inject write failure, then cancel unsaved close. Evidence `src/platform/FileIO.ts:92`. UNVERIFIED native.

55. **SYS-01 Keyboard shortcuts/conflicts/reset** — Settings; Assign conflict/reserved; reset; ignore text/modal typing. Evidence `src/platform/Shortcuts.ts:3`. UNVERIFIED native.

56. **SYS-02 Language** — Settings/toolbar; Throw storage reads/writes; UI remains available in new app. Evidence `src/i18n/index.tsx:18`. UNVERIFIED native.

57. **SYS-03 System log capture/save/clear** — F8/log control; Toggle verbose, clear and save; no secret/path disclosure. Evidence `src/platform/SystemLog.ts:6`. UNVERIFIED native.

58. **SYS-04 Startup trace and native shell** — Native bootstrap; Fail before UI; inspect bounded trace and retention behavior. Evidence `src-tauri/src/startup_trace.rs:56`. UNVERIFIED native.

59. **SYS-05 Resource budgets and worker cancellation** — Import/analysis/Undo; Reject overbudget input without replacing current document. Evidence `src/performance/MemoryBudget.ts:9`. UNVERIFIED native.

60. **REPAIR-01 remove-degenerate-triangles** — Finding repair / queue; Synthetic target for remove-degenerate-triangles: preview no mutation; apply removes defect; undo exact arrays; export/reimport. Evidence `src/heal/framework/RepairRegistry.ts:8`. UNVERIFIED native.

61. **REPAIR-02 remove-unreferenced-vertices** — Finding repair / queue; Synthetic target for remove-unreferenced-vertices: preview no mutation; apply removes defect; undo exact arrays; export/reimport. Evidence `src/heal/framework/RepairRegistry.ts:27`. UNVERIFIED native.

62. **REPAIR-03 recalculate-normals** — Finding repair / queue; Synthetic target for recalculate-normals: preview no mutation; apply removes defect; undo exact arrays; export/reimport. Evidence `src/heal/framework/RepairRegistry.ts:46`. UNVERIFIED native.

63. **REPAIR-04 merge-exact-duplicate-vertices** — Finding repair / queue; Synthetic target for merge-exact-duplicate-vertices: preview no mutation; apply removes defect; undo exact arrays; export/reimport. Evidence `src/heal/framework/RepairRegistry.ts:66`. UNVERIFIED native.

64. **REPAIR-05 normalize-skin-weights** — Finding repair / queue; Synthetic target for normalize-skin-weights: preview no mutation; apply removes defect; undo exact arrays; export/reimport. Evidence `src/heal/framework/RepairRegistry.ts:85`. UNVERIFIED native.

65. **REPAIR-06 remove-exact-duplicate-triangles** — Finding repair / queue; Synthetic target for remove-exact-duplicate-triangles: preview no mutation; apply removes defect; undo exact arrays; export/reimport. Evidence `src/heal/framework/RepairRegistry.ts:104`. UNVERIFIED native.

66. **REPAIR-07 consolidate-duplicate-skin-influences** — Finding repair / queue; Synthetic target for consolidate-duplicate-skin-influences: preview no mutation; apply removes defect; undo exact arrays; export/reimport. Evidence `src/heal/framework/RepairRegistry.ts:123`. UNVERIFIED native.
