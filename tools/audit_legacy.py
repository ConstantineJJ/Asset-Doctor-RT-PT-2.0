"""Read-only evidence collector. Writes only to this repository's reference/docs.

Lexical source indexes are discovery aids, never proof of GUI execution or parity.
No Legacy modules are imported or executed. Source implementation is not vendored.
"""
import csv
import hashlib
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEGACY = Path(r"E:\MyCreations\Asset-Doctor")
REF = ROOT / "reference"
DOCS = ROOT / "docs"


def git(*args):
    return subprocess.check_output(["git", "-C", str(LEGACY), *args], text=True).strip()


def read(path):
    return (LEGACY / path).read_text(encoding="utf-8-sig")


def evidence(path, needle):
    lines = read(path).splitlines()
    found = [i for i, line in enumerate(lines, 1) if needle in line]
    if not found:
        raise ValueError(f"Missing evidence: {path}: {needle}")
    return f"{path}:{found[0]}"


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text.rstrip() + "\n", encoding="utf-8")


def table(path, rows):
    with path.open("w", encoding="utf-8-sig", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


# Curated scenarios discovered from current source, not from old architecture.
# id | task | source | symbol | entry | parameters | mutation/history/cancel | acceptance
FEATURES = """
IMP-01|Open self-contained GLB/glTF|src/loaders/GLBLoaderService.ts|loadFromFile(|Open / drop|192 MiB source admission; two pending files|Replace only accepted asset; load sequence; errors retain old document|Open valid, truncated and oversized files; old model survives failure
IMP-02|Reject external resources|src/loaders/GLBLoaderService.ts|externalUris.length|All file import paths|external bin/images/http/file references|No sibling/network grants; reject before loader|Reject external URI even in renamed GLB
IMP-03|Draco decode|src/loaders/GLBLoaderService.ts|setDRACOLoader|Open compressed asset|bundled decoder path|Loading only; export certification separate|Open licensed Draco fixture offline and compare decoded counts
IMP-04|Meshopt decode|src/loaders/GLBLoaderService.ts|setMeshoptDecoder|Open compressed asset|bundled MeshoptDecoder|Source preserved; decoder resource lifecycle|Decode Meshopt fixture offline and repeat import
IMP-05|KTX2/BasisU decode|src/loaders/GLBLoaderService.ts|ensureKtx2Support|Open textured asset|GPU support detection required|Temporary probe disposed; errors explicit|Open KTX2 fixture and test unsupported renderer
IMP-06|Built-in test patient|src/components/TopToolbar.tsx|test-patient|Samples menu|one named sample entry|Switch subject to unsaved guard|Load patient and identify intentionally broken geometry
UI-01|Application menu/settings/exit|src/components/ApplicationMenu.tsx|requestDesktopClose|Main menu|language/helpers/keyboard bindings|Native close guard; browser Exit disabled|Keyboard menu traversal and cancel unsaved close
UI-02|Scene tree collapse|src/App.tsx|isTreeCollapsed|Toolbar tree toggle|default false (expanded)|Presentation state; no document Undo|Start application and toggle tree without losing selection
UI-03|Inspect tabs|src/components/InspectorPanel.tsx|activeTab === 'health'|Right inspector|health/summary/materials/skeleton/performance|Presentation only|Switch tabs with model and report intact
TREE-01|Select/focus/visibility/isolate/show all|src/App.tsx|handleToggleVisibility|Scene tree / viewport|selected UUID|Presentation toggles; no persistent document history established|Select nested mesh, isolate, focus, restore all
VIEW-01|Render channels|src/components/TopToolbar.tsx|select-render-mode|Toolbar selector|13 PBR/debug modes|GPU presentation; original materials retained|Cycle all modes then export original authored material values
VIEW-02|Shading modes|src/viewer/ShadingMode.ts|type ShadingMode|Shading control|hybrid/smooth/flat; default hybrid|Stored preference; bounded exact smoothing|Switch mode and verify source geometry unchanged
VIEW-03|Lighting presets|src/components/TopToolbar.tsx|select-lighting-preset|Toolbar lighting|8 presets|Presentation; independent Compare setting exists|Cycle presets and check material values remain authored
VIEW-04|Edit key/fill/rim/ambient/exposure|src/App.tsx|const [lightingConfig|Inspector lighting|exposure 1; ambient .4; key 1.6; fill .6; rim 1|Presentation config; no shared command Undo|Change each supported parameter; inspect illumination and export
VIEW-05|Light bulb manipulation|src/viewer/LightingManager.ts|Interactive Light Bulb|Viewport bulb|key light position; shadow toggle|Legacy helper graph; must not be reused as new scene architecture|Drag near/far and verify predictable world position
VIEW-06|Floor themes|src/viewer/SurfaceCatalog.ts|surfaceChoices|Floor selector|grid + 11 images + 6 procedural + none|Presentation texture pool owns resources|Cycle themes twice; no duplicate choices or growing retained textures
VIEW-07|Helpers and overlays|src/App.tsx|const [toggles|Toolbar/settings|grid=true axes=true bbox=false skeleton=false origin=true|Presentation; shared skeleton visibility|Toggle each overlay in Doctor and Compare
VIEW-08|Exploded presentation|src/components/TopToolbar.tsx|slider-explode|Toolbar slider|0..100 default 0|Non-destructive displacement; source matrix contract|Explode and export; source transforms unchanged
VIEW-09|Manual turntable/autorotate|src/App.tsx|autoRotateSpeed|Viewport controls|rotation 0; autorotate false; speed 1|Legacy presentation; new owner requests removal|Document as Legacy only; new app must never autorotate
CAM-01|Orbit/pan/zoom|src/viewer/CameraController.ts|new OrbitControls|Viewport mouse|damping .08; distance .05..50000|Presentation; key/mouse ownership must be redesigned|Orbit/pan/zoom without document transform changes
CAM-02|Frame all/focus/raw bounds/presets|src/components/TopToolbar.tsx|onFrameRawBounds|Toolbar and F/Shift+F|perspective/front/top/right API; raw debug fit|No document edits|Fit skinned/translated model without clipping
CMP-01|Manual Compare opening|src/components/Viewport.tsx|compareOpen|Viewport Compare button|separate modal/view state|Doctor active suspended; explicit transition|Add two assets and switch back with state retained
CMP-02|Add/remove/select/solo models|src/components/CompareViewport.tsx|SLOT_NAMES|Compare slots|A..H; max 8; source total 8x192 MiB|Separate asset records/resource disposal; no shared document Undo|Add eight, reject ninth, remove/re-add without leaks
CMP-03|Layout and scale matching|src/components/CompareViewport.tsx|scaleMode|Compare controls|grid/lineup; real/manual matching|Presentation scale separate from source|Switch layout and compare known dimensions
CMP-04|Synchronized/independent cameras|src/components/CompareViewport.tsx|syncCameras|Compare controls|default true|Presentation camera state|Unsync orbit one slot; sync again predictably
CMP-05|Compare animation sync/timeline|src/components/CompareViewport.tsx|syncAnimations|Compare timeline|sync=true playing=false loop=true speed=1|Preview only|Play/seek differing-duration clips and change sync
CMP-06|Open compared asset in Doctor|src/components/Viewport.tsx|openCompareAssetInDoctor|Slot action|sourceBuffer required|Rebuilds selected File then closes Compare|Transfer skinned asset and verify source-backed export
DIAG-01|Integrity of buffers/skeleton references|src/analysis/IntegrityAnalyzer.ts|analyzeIntegrity|Import / Rescan|finite data; valid indices and attribute pairs|Read-only; sampled locations capped at 16|Malformed index/NaN/skin pair yields concrete evidence
DIAG-02|Geometry summary/tree/bounds|src/analysis/GeometryAnalyzer.ts|buildSceneTree|Summary/tree|counts and node classification|Read-only; authored vs presentation distinction|Nested/skinned fixture counts and bounds agree with expected
DIAG-03|Topology|src/analysis/TopologyAnalyzer.ts|export|Progressive analysis|edges/components/duplicate/degenerate checks|Worker path; cancellation/stale guards require native exercise|Synthetic defect fixture gives expected issue IDs and locations
DIAG-04|Normals and UV|src/analysis/NormalsAndUvAnalyzer.ts|export|Health panel|missing/zero normals and UV validity|Read-only|Known absent/zero data produces correct severity
DIAG-05|Materials/textures|src/analysis/MaterialAnalyzer.ts|export|Health/materials|authored material values; texture dimensions/channels|Read-only|PBR/alpha/missing maps fixture preserves values and reports evidence
DIAG-06|Skeleton/weights|src/analysis/SkeletonAnalyzer.ts|export|Rig inspector|bones/bind/skin influence metrics|Read-only|Invalid sums/indices and unused bones are distinguished
DIAG-07|Animation diagnostics|src/analysis/AnimationDiagnostics.ts|export|Health/animation list|tracks/durations/root motion|Read-only|Bad track and valid rest pose distinguish integrity vs profile advice
DIAG-08|Transforms/performance|src/analysis/PerformanceAnalyzer.ts|export|Summary/performance|draw calls/memory estimates/profile thresholds|Estimates are not measured process RAM/VRAM|Known mesh/material counts and timing stage evidence
DIAG-09|Diagnostic profiles|src/health/DiagnosticProfiles.ts|DIAGNOSTIC_PROFILES|Profile selector|7 target profiles; general default|Severity interpretation only; no asset mutation|Same geometry gets target-specific fitness findings
DIAG-10|Finding focus/filter/export report|src/components/InspectorPanel.tsx|onLogIssue|Health cards and F9|category/severity/technical evidence|Reports do not clear unsaved model state|Save single/all reports; cancel dialog retains model state
HEAL-01|Preview/apply/verify/undo|src/heal/SurgicalHealEngine.ts|applyPendingAsync|Finding action/report|READY plan for current mesh/revision|Transactional geometry; verify before success; preview cancellation|Preview no mutation; apply clears target; undo restores exact arrays
HEAL-02|Safe repair queue|src/heal/SafeRepairQueueRunner.ts|runSafeRepairQueue|Health queue controls|max 250 passes; re-read candidates after each apply|User confirmation; stop/cancel/stale/regression codes|Cancel queued repair; regression stops further apply
HEAL-03|Persisted repair report/history|src/heal/HealReportStorage.ts|export|Health history|historical vs active verification distinct|Storage failure explicit; history not model state|Reopen stored report without treating it as current verified repair
RIG-01|Inspect/edit/pose modes|src/rig/RigEditSession.ts|type RigMode|Rig toolbar|inspect/edit/pose; initial inspect|Authored rest/clips separate from transient pose|Pose then return to edit without corrupting rest
RIG-02|Joint gizmo tools and numeric edits|src/components/RigEditorPanel.tsx|Rig tools|Joint toolbar/fields|select/translate/rotate/twist; local/world; joint/end|Session transaction/gesture; child follow/hold|Drag cancelled/committed once; undo restores rest and inverse binds
RIG-03|Add/rename joint and edit lengths|src/components/RigEditorPanel.tsx|Apply Length|Rig panel/context menu|length > .000001; numeric xyz step .01|Rig history; terminal guides separate from deform bones|Add child/end, rename, length change and undo/redo/export
RIG-04|Delete joint/chain/skeleton|src/rig/RigRemoval.ts|export|Rig/context removal dialog|scope and skinning consequences|Guard invalid removal; snapshots retain resources|Delete with influences, undo exact bind/animation restoration
RIG-05|Fit rig to surface proposals|src/components/RigFittingPanel.tsx|fit-analyze|Rig fitting panel|select suggested; shift threshold .35 in fitting core|Worker preview/cancel; apply selected revision guarded|Analyze, cancel; reject stale/uncertain proposals; undo applied fit
RIG-06|Undo/redo/reset original|src/rig/RigEditSession.ts|get canUndo|Rig panel/Ctrl+Z|128 MiB history budget; retained baseline|Separate Legacy history from Heal; no unified new commands yet|Undo and redo after branching; restore original; budget failure no mutation
ANIM-01|Autoplay imported first clip|src/App.tsx|setIsPlayingAnimation(true)|Successful import|clips.length > 0|Current source starts playback; native unverified|Import animated/static files then switch models
ANIM-02|Clip search/selection|src/components/AnimationClipList.tsx|toLocaleLowerCase|Animation list|case-insensitive substring; busy disables selection|Edit mode selects without motion|Search/select long named clip in edit and inspect modes
ANIM-03|Play/pause/stop/step/seek/loop/speed|src/components/AnimationTimeline.tsx|const speeds|Bottom timeline|.25/.5/1/2x; seek 0..1 step .001; loop=true|Preview only; stop resets|Boundary seek/step/loop on short and root-motion clips
ANIM-04|Rename and retime clip|src/components/RigEditorPanel.tsx|Retime Selected Clip|Clip editor|name max 256; duration > .000001|Rig command history; preview speed not exported duration|Rename/retime export and reimport keys/duration
ANIM-05|Root-motion measurement/overlay|src/components/AnimationTimeline.tsx|rootMotionDetected|Timeline Path toggle|translation/rotation vector|Presentation only|Known root-motion clip matches measured displacement
EXP-01|Repaired GLB export and re-open verification|src/export/RepairedExportService.ts|exportAndVerify(request|Toolbar / Heal report|verified active repairs; one scene; reachable nodes|Source-preserving patch; cancellation; verified reimport|Apply repair export/reimport; compare hierarchy/materials/skin/clips
EXP-02|Rig-edited/combined GLB export|src/rig/RigExportService.ts|async export|Rig verify/save|source patch with authored rest and clips|Revision/geometry/bind guards; worker cancellation|Edit rig + Heal, export/reimport or explicitly block unsupported combination
EXP-03|Save cancellation/failure/close protection|src/platform/FileIO.ts|saveBinaryFile|Native Save and application close|selected output path; GLB only|Only completed write clears protection; browser download not proof|Cancel Save, inject write failure, then cancel unsaved close
SYS-01|Keyboard shortcuts/conflicts/reset|src/platform/Shortcuts.ts|shortcutActions|Settings|12 actions; reserved chords; no mouse bindings|Stored adapter validates defaults/conflicts|Assign conflict/reserved; reset; ignore text/modal typing
SYS-02|Language|src/i18n/index.tsx|readStoredLanguage|Settings/toolbar|English fallback; en/ru|localStorage reads/writes need fault validation|Throw storage reads/writes; UI remains available in new app
SYS-03|System log capture/save/clear|src/platform/SystemLog.ts|preferences=|F8/log control|enabled true; verbose false; 600 entries; 2 MiB|Sanitized structured context; native disk rotation separate|Toggle verbose, clear and save; no secret/path disclosure
SYS-04|Startup trace and native shell|src-tauri/src/startup_trace.rs|pub fn record|Native bootstrap|48 events per trace|Stage only; historical report flags unbounded file count|Fail before UI; inspect bounded trace and retention behavior
SYS-05|Resource budgets and worker cancellation|src/performance/MemoryBudget.ts|memoryLimits|Import/analysis/Undo|packet32/worker64/history128 MiB|Independent binary budgets not whole RAM/VRAM|Reject overbudget input without replacing current document
""".strip()

REQUESTS = [
    ("REQ-IMP-001", "Increase import admission; decoded RAM/VRAM checks", "src/performance/MemoryBudget.ts", "modelFile=192", "Source cap already 192 MiB; decoded admission not complete", "Boundary source+decoded budgets and cancel keep old document"),
    ("REQ-LIGHT-001", "Light is editable scene entity with attached gizmo", "src/viewer/LightingManager.ts", "lightBulbGroup", "Legacy helper exists; requested authoritative entity behavior unverified", "Drag light in local/world without jumps or camera motion"),
    ("REQ-LIGHT-002", "Light context menu/color/temperature/enable", "src/viewer/LightingManager.ts", "currentConfig", "Partial lighting controls; full requested menu/temperature not established", "All displayed controls work, serialize and undo"),
    ("REQ-INPUT-001", "Rebind left/right/side mouse buttons", "src/platform/Shortcuts.ts", "validChord", "Keyboard-only binding schema found", "Mouse namespace conflicts/reset and no Back/Forward"),
    ("REQ-SESSION-001", "Automatic single at 1, Compare at 2+ with shared settings", "src/components/Viewport.tsx", "compareOpen", "Manual separate Compare found", "1→2→1 keeps edits/selection/jobs/export correct"),
    ("REQ-GIZMO-001", "Model translate/scale gizmo; remove autorotation", "src/App.tsx", "autoRotateSpeed", "Legacy turntable exists; requested model edit contract not established", "Move/scale/undo/export; no automatic rotation"),
    ("REQ-TREE-001", "Start scene tree collapsed", "src/App.tsx", "isTreeCollapsed", "Current source false (expanded)", "Fresh start collapsed, expand on request"),
    ("REQ-ANIM-001", "Autoplay imported animations", "src/App.tsx", "setIsPlayingAnimation(true)", "Autoplay branch found in current source; native unverified", "Animated clip starts; static model remains safe; edit selection controlled"),
    ("REQ-FLOOR-001", "Distinct themes and no floor duplicates", "src/viewer/SurfaceCatalog.ts", "surfaceChoices", "Shared catalog exists; visual distinctness/native unverified", "Cycle themes twice; one floor, unique labels, persist choice"),
    ("REQ-ENV-001", "Environment toggle independent of lamps", "src/viewer/LightingManager.ts", "lightsGroup", "Lighting graph found; independent environment contract not established", "Toggle environment without changing manual lamp states"),
]

# Explicitly reviewed user controls, separate from transient useState discovery.
# name | default | allowed UI values | evidence path | evidence token | storage/history
SETTINGS = """
renderMode|pbr|13 toolbar options|src/components/TopToolbar.tsx|select-render-mode|session presentation; no document Undo
lightingPreset|neutral-studio|8 preset options|src/components/TopToolbar.tsx|select-lighting-preset|session presentation; Compare has own state
shading|hybrid|hybrid/smooth/flat|src/viewer/ShadingMode.ts|STORAGE_KEY|localStorage; no document Undo
treeCollapsed|false|true/false|src/App.tsx|isTreeCollapsed|session presentation; requested new default true
floor|grid|19 shared catalog choices|src/viewer/SurfaceCatalog.ts|surfaceChoices|session presentation; no document Undo
grid|true|true/false|src/App.tsx|grid: true|session presentation
axes|true|true/false|src/App.tsx|axes: true|session presentation
bbox|false|true/false|src/App.tsx|bbox: false|session presentation
skeleton|false|true/false|src/App.tsx|skeleton: false|session presentation; shared Compare overlay
origin|true|true/false|src/App.tsx|origin: true|session presentation
skeletonXray|false|true/false|src/App.tsx|skeletonXray|session presentation
exposure|1.0|0.2..3 step .05|src/components/InspectorPanel.tsx|value={lightingConfig.exposure}|session presentation
keyIntensity|1.6|0..4 step .05|src/components/InspectorPanel.tsx|value={lightingConfig.keyIntensity}|session presentation
fillIntensity|.6|0..2.5 step .05|src/components/InspectorPanel.tsx|value={lightingConfig.fillIntensity}|session presentation
rimIntensity|1.0|0..3.5 step .05|src/components/InspectorPanel.tsx|value={lightingConfig.rimIntensity}|session presentation
environmentIntensity|.5 UI fallback|0..2.5 step .05|src/components/InspectorPanel.tsx|value={lightingConfig.environmentIntensity|UI environmentIntensity separate from App ambientIntensity .4; reconcile before parity
keyPosition|[5,8,5]|X/Z -20..20; Y .5..25 step .5|src/components/InspectorPanel.tsx|keyPosition?.[0]|session presentation; bulb drag range differs from sliders
keyColor|#fffbf5|HTML color + 5 named color presets|src/components/InspectorPanel.tsx|value={lightingConfig.keyColor|session presentation; preset Kelvin names are colors not full temperature model
fillColor|#dce5ef|HTML color|src/components/InspectorPanel.tsx|value={lightingConfig.fillColor|session presentation
rimColor|#ffffff|HTML color|src/components/InspectorPanel.tsx|value={lightingConfig.rimColor|session presentation
keyShadows|true|true/false|src/components/InspectorPanel.tsx|castShadows ?? true|session presentation
lightBulbVisible|true|true/false|src/App.tsx|showLightBulb: true|session presentation
explodedAmount|0|0..100|src/components/TopToolbar.tsx|slider-explode|session presentation; authored transform unchanged
modelRotation|0|manual turntable|src/App.tsx|modelRotation,|session presentation; new product removes turntable
autoRotate|false|true/false|src/App.tsx|autoRotate,|session presentation; new product removes action
autoRotateSpeed|1|range still requires source/UI reconciliation|src/App.tsx|autoRotateSpeed,|session presentation; new product removes action
diagnosticProfile|general|general/desktop-game-character/mobile-game-character/static-prop/animated-character/mechanical-asset/visualization|src/health/DiagnosticProfiles.ts|DIAGNOSTIC_PROFILES|session interpretation; no model mutation
animationSpeed|1|.25/.5/1/2|src/components/AnimationTimeline.tsx|const speeds|preview only; not exported duration
animationLoop|true|true/false|src/App.tsx|isLoopingAnimation|preview only
rootMotionPath|false|true/false|src/App.tsx|showRootMotion,|preview only
animationSeek|0|normalized 0..1 step .001|src/components/AnimationTimeline.tsx|timeline-scrubber|preview only
rigMode|inspect|inspect/edit/pose|src/rig/RigEditSession.ts|mode:RigMode=|session mode; changes pose/playback handling
rigTool|translate|select/translate/rotate/twist with mode constraints|src/components/RigEditorPanel.tsx|Rig tools|presentation tool; committed gesture uses Rig history
rigSpace|local|local/world; twist forces local|src/components/RigEditorPanel.tsx|Rig coordinates|presentation tool
rigHandle|joint|joint/end|src/components/RigEditorPanel.tsx|Rig handle|presentation tool; rotate end blocked
rigChildPolicy|follow|follow/hold|src/components/RigEditorPanel.tsx|Rig children policy|parameter of committed rig command
jointPosition|authored rest|XYZ finite; UI step .01|src/components/RigEditorPanel.tsx|Joint position|Rig transaction + Undo
jointRotation|authored rest|XYZ degrees UI step 1|src/components/RigEditorPanel.tsx|Joint rotation|Rig transaction or transient pose preview
boneLength|selected bone|positive; UI min .000001 step .01|src/components/RigEditorPanel.tsx|aria-label="Bone length"|Rig transaction + Undo
clipName|selected clip|nonempty; max 256|src/components/RigEditorPanel.tsx|maxLength={256}|Rig history; exported GLB
clipDuration|selected duration|positive; UI min .000001 step .1|src/components/RigEditorPanel.tsx|Export clip duration|Rig history; exported keys retimed
compareLayout|grid|grid/lineup|src/components/CompareViewport.tsx|viewMode,|session presentation
compareScale|real|real/normalize-height + per-model manual scale|src/components/CompareViewport.tsx|scaleMode,|presentation; source unaffected
compareCameraSync|true|true/false|src/components/CompareViewport.tsx|syncCameras,|session presentation
compareAnimationSync|true|true/false|src/components/CompareViewport.tsx|syncAnimations,|session presentation
language|en|en/ru|src/i18n/index.tsx|STORAGE_KEY|localStorage; fault behavior requires native check
systemLogEnabled|true|true/false|src/platform/SystemLog.ts|enabled:true|localStorage preferences + native writer
systemLogVerbose|false|true/false|src/platform/SystemLog.ts|verbose:false|localStorage preferences
""".strip()


def main():
    REF.mkdir(exist_ok=True)
    DOCS.mkdir(exist_ok=True)
    before = (git("rev-parse", "HEAD"), git("status", "--porcelain=v1"))
    tracked = git("ls-files").splitlines()
    files = [p for p in tracked if (LEGACY / p).is_file()]
    manifest = [{"path": p, "bytes": (LEGACY / p).stat().st_size,
                 "sha256": hashlib.sha256((LEGACY / p).read_bytes()).hexdigest()} for p in files]
    controls, settings, diagnostics, tests = [], [], [], []
    for path in files:
        if Path(path).suffix not in {".ts", ".tsx", ".rs"}:
            continue
        source = read(path)
        lines = source.splitlines()
        if path.endswith(".tsx"):
            for i, line in enumerate(lines, 1):
                for match in re.finditer(r"<(button|input|select|dialog)\b", line):
                    context = " ".join(lines[i - 1:i + 6])
                    callbacks = re.findall(r"\b(onClick|onChange|onInput|onClose)=\{([^\n]{0,180})", context)
                    identity = re.search(r'(?:data-testid|id|aria-label)="([^"]+)"', context)
                    controls.append({"id": f"UI-{len(controls)+1:04d}", "source": f"{path}:{i}", "kind": match[1],
                                     "label_hint": identity[1] if identity else "dynamic/unlabelled; review required",
                                     "callback_hint": " | ".join(a + ":" + b[:110] for a, b in callbacks[:2]),
                                     "mapping_status": "UNREVIEWED_SOURCE_SITE", "runtime_status": "UNVERIFIED"})
        for i, line in enumerate(lines, 1):
            state = re.search(r"const \[(\w+),.*?useState(?:<[^;]+?>)?\((.*)", line)
            if state:
                settings.append({"name": state[1], "source": f"{path}:{i}", "initial_expression": state[2][:180],
                                 "classification": "STATE_CANDIDATE_NOT_ALL_ARE_USER_SETTINGS", "persistence": "REVIEW_REQUIRED"})
            if path.startswith("src/analysis/") and "Tests" not in path:
                for issue in re.finditer(r"\bid:\s*['\"]([^'\"]+)['\"]", line):
                    diagnostics.append({"issue_id": issue[1], "source": f"{path}:{i}", "status": "SOURCE_OBSERVED_RUNTIME_UNVERIFIED"})
        if "Tests" in path or "test" in Path(path).name.lower():
            tests.append({"path": path, "status": "DISCOVERED_NOT_EXECUTED", "sha256": hashlib.sha256((LEGACY / path).read_bytes()).hexdigest()})
    features = []
    for spec in FEATURES.splitlines():
        fid, title, path, needle, entry, params, behavior, acceptance = spec.split("|")
        features.append(dict(id=fid, title=title, evidence=evidence(path, needle), entry=entry, parameters=params,
                             mutation_undo_cancel=behavior, errors="See acceptance and referenced implementation; native fault exercise pending",
                             legacy_status="UNVERIFIED", source_status="OBSERVED", new_status="NOT_STARTED", origin="LEGACY_OBSERVED",
                             priority="P0" if fid.startswith(("IMP", "HEAL", "RIG", "EXP")) else "P1", acceptance_test=acceptance))
    for n, kind in enumerate(re.findall(r"kind: '([^']+)'", read("src/heal/framework/RepairRegistry.ts")), 1):
        features.append(dict(id=f"REPAIR-{n:02d}", title=kind, evidence=evidence("src/heal/framework/RepairRegistry.ts", f"kind: '{kind}'"),
                             entry="Finding repair / queue", parameters="Current mesh; CONDITIONAL risk; READY preview required",
                             mutation_undo_cancel="Preview/apply/undo/verify; index-only or geometry export patch per registry",
                             errors="BLOCKED unsupported geometry; rollback/regression must be exercised",
                             legacy_status="UNVERIFIED", source_status="OBSERVED", new_status="NOT_STARTED", origin="LEGACY_OBSERVED",
                             priority="P0", acceptance_test=f"Synthetic target for {kind}: preview no mutation; apply removes defect; undo exact arrays; export/reimport"))
    requests = []
    for fid, title, path, needle, observation, acceptance in REQUESTS:
        requests.append(dict(id=fid, requested_state=title, origin="OWNER_REQUESTED", requirement_status="CONFIRMED",
                             legacy_status="UNVERIFIED", source_observation=observation, evidence=evidence(path, needle),
                             design_status="REQUESTED", priority="P0", acceptance_test=acceptance))
    table(REF / "LEGACY_UI_SOURCE_SITES.csv", controls)
    table(REF / "LEGACY_STATE_CANDIDATES.csv", settings)
    reviewed_settings = []
    for row in SETTINGS.splitlines():
        name, default, allowed, path, needle, persistence = row.split("|")
        reviewed_settings.append(dict(setting=name, default=default, allowed=allowed, evidence=evidence(path, needle),
                                      persistence_undo=persistence, legacy_status="UNVERIFIED", source_status="OBSERVED",
                                      acceptance=f"Exercise {name} default, both boundaries/alternatives and reset; verify stated persistence/history"))
    for action, key in re.findall(r"id:'([^']+)',en:'[^']+',ru:'[^']+',key:'([^']+)'", read("src/platform/Shortcuts.ts")):
        reviewed_settings.append(dict(setting=f"shortcut.{action}", default=key, allowed="valid nonreserved keyboard chord or null; no conflicts",
                                      evidence=evidence("src/platform/Shortcuts.ts", f"id:'{action}'"), persistence_undo="localStorage with conflict/reserved/storage refusal; reset defaults",
                                      legacy_status="UNVERIFIED", source_status="OBSERVED", acceptance=f"Rebind/reset {action}; duplicate and reserved binding rejected"))
    table(REF / "LEGACY_SETTINGS_MATRIX.csv", reviewed_settings)
    if diagnostics:
        table(REF / "LEGACY_DIAGNOSTIC_IDS.csv", diagnostics)
    table(REF / "LEGACY_TEST_INDEX.csv", tests)
    table(DOCS / "FEATURE_PARITY_MATRIX.csv", features)
    table(REF / "OWNER_REQUESTS_BACKLOG.csv", requests)
    header = f"Legacy HEAD `{before[0]}`, branch `{git('branch', '--show-current')}`, dirty `{before[1] or 'clean'}`.\n\nSource observation only; no Legacy code/tests/GUI executed. All native scenarios UNVERIFIED.\n"
    catalog = ["# Legacy feature catalog", header]
    for feature in features:
        catalog.append(f"## {feature['id']} — {feature['title']}\n\n- Entry: {feature['entry']}\n- Parameters: {feature['parameters']}\n- Mutation / Undo / cancellation: {feature['mutation_undo_cancel']}\n- Errors: {feature['errors']}\n- Evidence: `{feature['evidence']}`\n- Source: OBSERVED; native: UNVERIFIED; new product: NOT_STARTED.\n- Acceptance: {feature['acceptance_test']}\n")
    write(REF / "LEGACY_FEATURE_CATALOG.md", "\n".join(catalog))
    write(REF / "OWNER_REQUESTS_BACKLOG.md", "# Owner requests\n\nThese ten requests come from Get Starting §2.4, not proof of Legacy implementation. Current user clarification: carry functionality only; old architecture is rejected.\n\n" + "\n".join(f"## {r['id']} — {r['requested_state']}\n\nREQUESTED / P0. {r['source_observation']}. Evidence `{r['evidence']}`. Native UNVERIFIED.\n\nAcceptance: {r['acceptance_test']}\n" for r in requests))
    parity = [dict(id=r["id"], title=r["requested_state"], evidence=r["evidence"], entry="Owner Get Starting §2.4", parameters=r["source_observation"],
                   mutation_undo_cancel="Design contract pending", errors="Fault acceptance pending", legacy_status="UNVERIFIED", source_status="OWNER_REQUEST", new_status="NOT_STARTED", origin="OWNER_REQUESTED", priority="P0", acceptance_test=r["acceptance_test"]) for r in requests]
    table(DOCS / "FEATURE_PARITY_MATRIX.csv", features + parity)
    write(REF / "LEGACY_UI_ATLAS.md", f"# Legacy UI atlas\n\n{header}\n## Source layout\n\nToolbar → scene tree / Doctor viewport / inspector → animation timeline → status bar. Application menu opens settings/shortcuts. Inspector: Health, Summary, Materials, Rig, Performance. Compare is a separate manually opened view with its own slots, camera/animation controls and settings. Rig removal, fitting, Heal preview/report and topology tests have separate UI surfaces.\n\n{len(controls)} lexical button/input/select/dialog source sites are indexed in LEGACY_UI_SOURCE_SITES.csv, including dynamic template sites. This is NOT a count of visible controls. Label/callback hints are approximate and require semantic mapping. All sites remain UNREVIEWED_SOURCE_SITE until mapped to a catalog ID or excluded with reason.\n\n{len(reviewed_settings)} reviewed settings/control rows are in LEGACY_SETTINGS_MATRIX.csv. {len(settings)} additional state candidates are in LEGACY_STATE_CANDIDATES.csv; transient state is not automatically a user setting. Complete ranges/persistence and native behavior still require review.\n\nFull UI audit gate is OPEN. No claim that every UI action has been reviewed. Native interaction, DPI, focus, mouse ownership and screenshot atlas remain pending.")
    write(REF / "LEGACY_WORKFLOWS.md", "# Legacy workflows\n\n" + header + "\n" + "\n".join(f"{i}. **{f['id']} {f['title']}** — {f['entry']}; {f['acceptance_test']}. Evidence `{f['evidence']}`. UNVERIFIED native.\n" for i, f in enumerate(features, 1)))
    write(REF / "LEGACY_FORMAT_COMPATIBILITY.md", """# Format compatibility

Source evidence: GLBLoaderService, GltfSourceAudit, RepairedExportService, RigGlbPatch, FileIO (indexed in catalog).

| Data | Current source observation | New implementation |
|---|---|---|
| GLB 2 / self-contained JSON glTF | Accepted through loader; original bytes retained | NOT_STARTED |
| External buffers/images/URLs | Single-file importer rejects | Contract pending: never silently fetch |
| Draco | Bundled view decoder; modified rig/skinning combinations can block export | NOT_STARTED, do not claim parity |
| Meshopt | View decoder; buffer view patch rules exist | NOT_STARTED |
| KTX2/BasisU | Capability detection, bundled transcoder | NOT_STARTED |
| Skins, clips, morphs | Source metadata and export verification have dedicated rules | NOT_STARTED |
| Unknown extras/extensions | Guarded source patching, not universal certification | Preserve opaque provenance; reject unsupported edit explicitly |
| Repaired export | GLB only; source single scene/reachable-node restrictions | NOT_STARTED |
| Rig + Heal export | Separate source patch path; Draco/static baking/index/accessor extension restrictions | NOT_STARTED |
| FBX/OBJ/USD | No product loader found in inspected paths | NOT_FOUND; no implied support |

All entries are source observations, not a fresh file roundtrip/native PASS. Export support must be tested per extension × operation, not inferred from viewing support.
""")
    fixture_paths = [p for p in files if Path(p).suffix.lower() in {".glb", ".gltf", ".ktx2", ".bin"}]
    write(REF / "LEGACY_TEST_FIXTURES.md", f"# Test/fixture discovery\n\n{len(tests)} test-named source files discovered; none executed or copied. See LEGACY_TEST_INDEX.csv.\n\nTracked candidate asset paths (licensing not established; DO NOT vendor):\n\n" + "\n".join(f"- `{p}`" for p in fixture_paths) + "\n\nNew synthetic fixtures must cover empty/truncated/oversized GLB, embedded glTF, nested and negative transforms, PBR/UV/alpha, all seven repairs, skeleton/rest/pose/weights/morphs, clips, compressed extensions and 1→8→1 models. Each requires origin/license/hash/decoded sizes/expected output. Historical 387/387 + 5/5 is not current execution evidence.")
    write(REF / "LEGACY_GAP_REGISTER.md", """# Open audit / parity gaps

P0: Semantic mapping of every source UI site to feature IDs is incomplete. The lexical index is a search aid. Full review gate remains OPEN.
P0: Installed Legacy EXE has not been exercised; native dialogs, Save cancel/failure, unsaved close, repair/rig/export roundtrips remain UNVERIFIED.
P0: Per-operation × extension × source-fidelity fixture matrix is not certified. Do not build Heal/Rig until contracts and fixtures close these unknowns.
P0: State-candidate list needs user-setting classification, exact UI constraints and persistence/Undo/cancel review.
P0: Original owner screenshot/checkpoint was referenced by supplied document but not independently supplied in this request. Use the ten explicit §2.4 requirements, without inventing screenshot evidence.
P1: GUI atlas screenshots and high-DPI/keyboard/side-button interaction remain pending.
P1: Resource peaks, latency, cancellation responsiveness and growing retained resources have not been measured in native Legacy.

Arc-XRay findings are historical supplemental leads: orchestration concentration, synchronous heavy passes, decoded-memory coverage, storage failures, native test gap, startup retention, duplicated tree projections. The reported type-only cycle is not evidence of a runtime cycle. No finding was repaired in Legacy.

This is a substantive first source inventory, not a completed exhaustive functional audit or a native parity claim.
""")
    sources = [Path(r"D:\Desktop\logs\arc-xray-findings-selected.md"), Path(r"D:\Desktop\index.html"),
               Path(r"D:\Desktop\My Labarotory\Asset Doctor RT-PT 2.0\Asset Doctor RT-PT 2.0 Get Starting.md")]
    supplements = [{"path": str(p), "sha256": hashlib.sha256(p.read_bytes()).hexdigest(), "bytes": p.stat().st_size} for p in sources]
    atlas = sources[1].read_text(encoding="utf-8-sig")
    scripts = re.findall(r'<script[^>]*type=["\']application/json["\'][^>]*>(.*?)</script>', atlas, re.S)
    atlas_info = []
    for script in scripts:
        try:
            obj = json.loads(script)
            atlas_info.append({"keys": list(obj)[:25], "project": obj.get("project"),
                               "node_count": len(obj.get("nodes", [])), "edge_count": len(obj.get("edges", [])),
                               "findings": [{"id": f.get("id"), "title": f.get("title"), "status": f.get("status")} for f in obj.get("findings", [])],
                               "coverage": obj.get("coverage"), "completion": obj.get("completion")}
                              if isinstance(obj, dict) else {"type": type(obj).__name__})
        except json.JSONDecodeError:
            atlas_info.append({"json": "invalid/unparsed"})
    after = (git("rev-parse", "HEAD"), git("status", "--porcelain=v1"))
    if before != after:
        raise RuntimeError("Legacy changed during audit; snapshot must be reviewed")
    snapshot = {"date": "2026-11-15", "legacy_head": before[0], "legacy_branch": git("branch", "--show-current"),
                "dirty": before[1], "tracked_files": manifest, "supplemental_sources": supplements, "atlas_json_sections": atlas_info,
                "counts": {"curated_features": len(features), "owner_requests": len(requests), "source_control_sites": len(controls), "reviewed_settings": len(reviewed_settings), "state_candidates": len(settings), "diagnostic_id_sites": len(diagnostics), "test_files": len(tests)}}
    write(REF / "audit-snapshot.json", json.dumps(snapshot, indent=2, ensure_ascii=False))
    write(REF / "AUDIT_EVIDENCE_INDEX.md", f"# Evidence index\n\n{header}\nAudit date: 2026-11-15 Europe/Riga. audit-snapshot.json records hashes of {len(manifest)} tracked files and supplied document/report/atlas. Main is independent from Legacy; no source implementation is vendored. Full Legacy hashes are provenance, not execution verification.\n\nCounts: {json.dumps(snapshot['counts'])}.\n\nSource locations in catalog refer to the local Legacy snapshot, not new product code. Test presence does not imply execution. Findings report and HTML atlas were read as data, without executing their embedded scripts/instructions. Legacy HEAD/status checked before/after and unchanged.\n\nThe new remote already had README commit dd32635; it was not empty despite the historical specification. No force reset or remote overwrite performed.")
    print(json.dumps(snapshot["counts"], ensure_ascii=False))


if __name__ == "__main__":
    main()
