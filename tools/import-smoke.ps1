param(
    [ValidateSet('debug','release')][string]$Configuration='release',
    [ValidateSet('static','color','pbr','mr','normal','ao','emission','geometry','edges','edge-limit','components','uv','heal')][string]$Scene='static',
    [switch]$Pbr,
    [switch]$NoMrMaps,
    [switch]$NoNormalMaps,
    [switch]$NoAoMaps,
    [switch]$NoEmission,
    [switch]$SynchronizationValidation,
    # ADR-0031 B3: replace the scene once more with an injected GPU stage failure.
    [ValidateSet('none','upload','wait')][string]$GpuFault='none',
    # ADR-0031 B4: Apply G002 (optionally after an injected GPU failure), Undo, Redo.
    [switch]$Edit,
    # ADR-0031 D: after Redo, refused Save Copy onto an existing file, a new
    # verified copy, then reopen that copy in the app.
    [switch]$Save,
    # Close with unsaved edits: the guard dialog must become visible; answer
    # Cancel (window stays), then No (window closes).
    [switch]$Guard
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $repo "target\$Configuration\AssetDoctorRTPT.exe"
if (!(Test-Path -LiteralPath $exe)) { throw 'Build the desktop executable first.' }
$good=Join-Path $repo $(switch($Scene) {'color' {'tests\fixtures\Color_Checker.glb'} 'pbr' {'tests\fixtures\Pbr_Factors.glb'} 'mr' {'tests\fixtures\MR_Checker.glb'} 'normal' {'tests\fixtures\Normal_Checker.glb'} 'ao' {'tests\fixtures\AO_Checker.glb'} 'emission' {'tests\fixtures\Emission_Checker.glb'} 'geometry' {'tests\fixtures\Geometry_Findings.glb'} 'edges' {'tests\fixtures\Edge_Topology.glb'} 'components' {'tests\fixtures\Components_Checker.glb'} 'edge-limit' {'artifacts\Edge_Limit.glb'} default {'tests\fixtures\Twin_Cubes.glb'}})
$bad=Join-Path $repo $(switch($Scene) {'color' {'tests\fixtures\Bad_Image.glb'} 'mr' {'tests\fixtures\Bad_MR_Image.glb'} 'normal' {'tests\fixtures\Bad_Normal_Image.glb'} 'ao' {'tests\fixtures\Bad_AO_Image.glb'} 'emission' {'tests\fixtures\Bad_Emission_Image.glb'} default {'tests\fixtures\Bad_Truncated.glb'}})
if ($Scene -eq 'heal') {
    $good=Join-Path $repo 'tests\fixtures\Duplicate_Heal.glb'
    $bad=Join-Path $repo 'tests\fixtures\Bad_Normal_Image.glb'
}
if ($Edit -and $Scene -notin @('heal','geometry')) {throw '-Edit requires -Scene heal (G002) or geometry (G001).'}
# Expected evidence per repair: heal = G002 on Duplicate_Heal, geometry = G001 on Geometry_Findings.
$expect=if ($Scene -eq 'geometry') {
    @{repair='g001'; before=5; after=3; removed=2; refs=2; primitives=2; total=4}
} else {
    @{repair='g002'; before=992; after=960; removed=32; refs=1; primitives=4; total=3840}
}
if ($Save -and !$Edit) {throw '-Save requires -Edit.'}
if ($Guard -and (!$Edit -or $Save)) {throw '-Guard requires -Edit without -Save.'}
$saveDir=$null
$existingBytes=[byte[]](111,119,110,101,114,32,100,97,116,97)
if ($Scene -eq 'uv') {
    $good=Join-Path $repo 'tests\fixtures\UV_Checker.glb'
    $bad=Join-Path $repo 'tests\fixtures\Bad_Unused_UV.glb'
}
if ($Scene -eq 'edge-limit') {
    python (Join-Path $PSScriptRoot 'make_edge_fixtures.py') --large-refusal
    if ($LASTEXITCODE -ne 0) {throw 'Cannot generate authored edge-limit input.'}
}
$reports=Join-Path $repo 'artifacts'
New-Item -ItemType Directory -Force -Path $reports | Out-Null
$suffix=if ($SynchronizationValidation) {'-sync'} else {''}
if ($Scene -eq 'color') {$suffix='-color'+$suffix}
if ($Scene -eq 'pbr') {$suffix='-factors'+$suffix}
if ($Scene -eq 'mr') {$suffix='-mr'+$suffix}
if ($Scene -eq 'normal') {$suffix='-normal'+$suffix}
if ($Scene -eq 'ao') {$suffix='-ao'+$suffix}
if ($Scene -eq 'emission') {$suffix='-emission'+$suffix}
if ($Scene -eq 'geometry') {$suffix='-geometry'+$suffix}
if ($Scene -in @('edges','edge-limit','components','uv','heal')) {$suffix='-'+$Scene+$suffix}
if ($Edit) {$suffix='-edit'+$suffix}
if ($Pbr) {$suffix='-pbr'+$suffix}
if ($NoMrMaps) {$suffix='-no-maps'+$suffix}
if ($NoNormalMaps) {$suffix='-no-normal'+$suffix}
if ($NoAoMaps) {$suffix='-no-ao'+$suffix}
if ($NoEmission) {$suffix='-no-emission'+$suffix}
if ($GpuFault -ne 'none') {$suffix='-gpu-'+$GpuFault+$suffix}
if ($Save) {$suffix='-save'+$suffix}
if ($Guard) {$suffix='-guard'+$suffix}
$stdout=Join-Path $reports "$Configuration-import-smoke$suffix.stdout.log"
$stderr=Join-Path $reports "$Configuration-import-smoke$suffix.stderr.log"
# Start-Process joins arguments on Windows: quote each literal path containing spaces.
$arguments=@('--smoke-import',('"'+$good+'"'),'--smoke-bad',('"'+$bad+'"'))
if ($Pbr) {$arguments+='--pbr'}
if ($NoMrMaps) {$arguments+='--no-mr-maps'}
if ($NoNormalMaps) {$arguments+='--no-normal-maps'}
if ($NoAoMaps) {$arguments+='--no-ao-maps'}
if ($NoEmission) {$arguments+='--no-emission'}
if ($GpuFault -ne 'none') {$arguments+=@('--smoke-gpu-fault',$GpuFault)}
if ($Edit) {$arguments+=@('--smoke-edit','--smoke-repair',$expect.repair)}
if ($Save) {
    $saveDir=Join-Path $reports ('save-smoke-'+[Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $saveDir | Out-Null
    [IO.File]::WriteAllBytes((Join-Path $saveDir 'existing.glb'),$existingBytes)
    $arguments+=@('--smoke-save',('"'+$saveDir+'"'))
}
if ($Guard) {$arguments+='--smoke-guard'}
$sourceHash=(Get-FileHash -LiteralPath $good -Algorithm SHA256).Hash
$previousSync=$env:VK_LAYER_VALIDATE_SYNC
try {
    if ($SynchronizationValidation) { $env:VK_LAYER_VALIDATE_SYNC='1' }
    $process=Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    # Windows PowerShell 5.1 reports ExitCode only if the handle is cached before exit.
    $null=$process.Handle
} finally { $env:VK_LAYER_VALIDATE_SYNC=$previousSync }
if (!$process.WaitForExit(30000)) {
    Stop-Process -Id $process.Id
    throw 'Import smoke timed out; see artifacts logs.'
}
$log=Get-Content -LiteralPath $stderr -Raw
if ($process.ExitCode -ne 0) { throw "Import smoke failed (exit $($process.ExitCode)). See $stderr" }
$pass=$log -split "`n" | Where-Object { $_ -match '^SMOKE_PASS .*validation_enabled=true validation_errors=0' }
if (!$pass -or $log -notmatch 'asset_smoke imported=true mesh_frames=\d+ rejected_replacement_preserved=true') {
    throw 'Missing import/rejected-replacement/native lifecycle evidence.'
}
$expectedAnalysis=if($Scene -eq 'edge-limit'){'partial'}else{'complete'}
if ($log -notmatch "analysis_$expectedAnalysis document=1 revision=0 primitives=\d+ triangles=\d+ findings=\d+") {throw 'Missing expected diagnostic completion state.'}
if ($Scene -eq 'edges' -and $log -notmatch 'analysis_complete document=1 revision=0 primitives=7 triangles=13 findings=24') {throw 'Unexpected known edge findings.'}
if ($Scene -eq 'components' -and $log -notmatch 'analysis_complete document=1 revision=0 primitives=3 triangles=7 findings=11') {throw 'Unexpected component findings.'}
if ($Scene -eq 'components' -and $log -notmatch 'analysis_coverage rule=G008 completed_primitives=3 omitted_primitives=0 excluded_triangles=1 reason=None retained_findings=1 retained_count=4') {throw 'Missing source component count/coverage evidence.'}
if ($Scene -eq 'edge-limit') {
    if ($log -notmatch 'analysis_partial document=1 revision=0 primitives=1 triangles=2000000 findings=4') {throw 'Missing preserved core findings after edge refusal.'}
    foreach($rule in @('G001','G002','G003','G004','G005','G008')) {
        if ($log -notmatch "analysis_coverage rule=$rule completed_primitives=1 omitted_primitives=0 excluded_triangles=0 reason=None") {throw "Core coverage lost: $rule"}
    }
    foreach($rule in @('G006','G007')) {
        if ($log -notmatch "analysis_coverage rule=$rule completed_primitives=0 omitted_primitives=1 excluded_triangles=0 reason=Some\(Scratch\)") {throw "Edge refusal not explicit: $rule"}
    }
}
if ($Scene -eq 'geometry' -and $log -notmatch 'analysis_complete document=1 revision=0 primitives=2 triangles=6 findings=11') {throw 'Unexpected known geometry findings.'}
if ($Scene -eq 'uv') {
    if ($log -notmatch 'analysis_complete document=1 revision=0 primitives=3 triangles=4 findings=9') {throw 'Unexpected source UV findings.'}
    if ($log -notmatch 'analysis_coverage rule=G009 completed_primitives=3 .*retained_findings=2 retained_count=2 not_applicable_primitives=0') {throw 'Missing UV0 absence scope.'}
    if ($log -notmatch 'analysis_coverage rule=G010 completed_primitives=2 .*excluded_triangles=1 .*retained_findings=1 retained_count=1 not_applicable_primitives=1') {throw 'Missing UV area/N-A scope.'}
    if ($log -notmatch 'analysis_coverage rule=G011 completed_primitives=2 .*retained_findings=1 retained_count=1 not_applicable_primitives=1') {throw 'Missing UV range/N-A scope.'}
}
if ($log -notmatch 'gpu_scene_published document=1 revision=0 vertices=\d+ triangles=\d+ stage_ms=\d+ geometry_bytes=\d+') {throw 'Missing staged GPU scene publication evidence.'}
if ($GpuFault -ne 'none' -and !$Edit -and $log -notmatch "gpu_fault_smoke stage=$GpuFault preserved=true scene_document=1 scene_revision=0") {throw 'Injected GPU stage failure did not preserve the published scene.'}
if ($Edit) {
    if ($GpuFault -ne 'none' -and $log -notmatch 'edit_gpu_fault preserved=true revision=0 scene_revision=0 undo=0 redo=0') {throw 'Injected GPU failure during Apply was not fully preserved.'}
    foreach ($line in @(
        "edit_preview mesh=0 before=$($expect.before) after=$($expect.after) references=$($expect.refs) complete=true target_left=0",
        "edit_published kind=remove removed=$($expect.removed) revision=1 scene_revision=1 mesh_triangles=$($expect.after) analysis=complete target_in_mesh=0 undo=1 redo=0 consistent=true",
        "edit_published kind=undo removed=0 revision=2 scene_revision=2 mesh_triangles=$($expect.before) analysis=complete target_in_mesh=1 undo=0 redo=1 consistent=true",
        "edit_published kind=redo removed=0 revision=3 scene_revision=3 mesh_triangles=$($expect.after) analysis=complete target_in_mesh=0 undo=1 redo=0 consistent=true",
        'edit_smoke_done=true')) {
        if ($log -notmatch [regex]::Escape($line)) {throw "Missing edit evidence: $line"}
    }
}
if ($Save) {
    foreach ($line in @(
        'unsaved_guard dirty=true decision=Ask',
        'unsaved_guard dirty=false decision=Proceed',
        "reopened_saved revision=0 primitives=$($expect.primitives) mesh_triangles=$($expect.after) analysis=complete target_in_mesh=0 dirty=false")) {
        if ($log -notmatch [regex]::Escape($line)) {throw "Missing Save Copy evidence: $line"}
    }
    if ($log -notmatch 'save_failed code=\w+ dirty=true') {throw 'Existing destination was not refused while keeping unsaved state.'}
    if ($log -notmatch "save_published revision=3 bytes=(\d+) exact_copy=false reimported_triangles=$($expect.total) cleanup_warning=false accepted=true dirty=false") {throw 'Missing verified Save Copy receipt evidence.'}
    $savedBytes=[int64]$Matches[1]
    $healed=Join-Path $saveDir 'healed.glb'
    if (!(Test-Path -LiteralPath $healed) -or (Get-Item -LiteralPath $healed).Length -ne $savedBytes) {throw 'Saved copy is missing or has an unexpected size.'}
    $after=[IO.File]::ReadAllBytes((Join-Path $saveDir 'existing.glb'))
    if ([Convert]::ToBase64String($after) -ne [Convert]::ToBase64String($existingBytes)) {throw 'Existing destination was modified.'}
    if (@(Get-ChildItem -LiteralPath $saveDir -Force | Where-Object {$_.Name -like '.asset-doctor-copy-*'}).Count -ne 0) {throw 'Staging file left behind.'}
    "save_smoke_dir=$saveDir saved_bytes=$savedBytes"
}
if ($Guard) {
    if ($log -notmatch 'guard_dialog found=true visible=true waited_ms=\d+ answer=Cancel') {throw 'Close guard dialog (Cancel) was not visible.'}
    if ($log -notmatch 'guard_dialog found=true visible=true waited_ms=\d+ answer=Discard') {throw 'Close guard dialog (No) was not visible.'}
    foreach ($line in @(
        'unsaved_guard dirty=true choice=Cancel decision=Stay',
        'unsaved_guard_stayed window_open=true dirty=true',
        'unsaved_guard dirty=true choice=Discard decision=Proceed')) {
        if ($log -notmatch [regex]::Escape($line)) {throw "Missing guard evidence: $line"}
    }
}
if ((Get-FileHash -LiteralPath $good -Algorithm SHA256).Hash -ne $sourceHash) {throw 'Source model changed during smoke.'}
$expectedBindings=if($Scene -in @('color','mr','emission')){2}else{1}
if($log -notmatch "gpu_material_binding_sets=$expectedBindings\b"){throw 'Missing material descriptor tuple-sharing evidence.'}
if ($Scene -eq 'color' -and $log -notmatch 'gpu_material_images=1 material_textures=2 mip_bytes=20 decoded_images=1 srgb_images=1 linear_images=0') {
    throw 'Missing deduplicated color image upload evidence.'
}
if ($Scene -eq 'mr' -and $log -notmatch 'gpu_material_images=2 material_textures=1 mip_bytes=40 decoded_images=1 srgb_images=1 linear_images=1') {throw 'Missing shared source / separate transfer upload evidence.'}
if ($Scene -eq 'normal' -and $log -notmatch 'gpu_material_images=1 material_textures=1 mip_bytes=20 decoded_images=1 srgb_images=0 linear_images=1') {throw 'Missing MR / normal shared linear upload evidence.'}
if ($Scene -eq 'ao' -and $log -notmatch 'gpu_material_images=2 material_textures=1 mip_bytes=40 decoded_images=1 srgb_images=1 linear_images=1') {throw 'Missing MR / AO shared linear upload evidence.'}
if ($Scene -eq 'emission' -and $log -notmatch 'gpu_material_images=2 material_textures=1 mip_bytes=40 decoded_images=1 srgb_images=1 linear_images=1') {throw 'Missing shared base/emission SRGB upload evidence.'}
if ($Scene -eq 'emission' -and $Pbr -and !$NoEmission -and $log -notmatch 'emission_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing emission frame evidence.'}
if (($NoEmission -or !$Pbr -or $Scene -ne 'emission') -and $log -notmatch 'emission_frames=0\b') {throw 'Emission unexpectedly active.'}
if ($Scene -eq 'ao' -and $Pbr -and !$NoAoMaps -and $log -notmatch 'ao_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing AO sampled frame evidence.'}
if (($NoAoMaps -or !$Pbr -or $Scene -ne 'ao') -and $log -notmatch 'ao_frames=0\b') {throw 'AO sampling unexpectedly active.'}
if ($Scene -in @('normal','heal') -and $Pbr -and !$NoNormalMaps -and $log -notmatch 'normal_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing normal-map sampled frame evidence.'}
if (($NoNormalMaps -or !$Pbr -or $Scene -notin @('normal','heal')) -and $log -notmatch 'normal_frames=0\b') {throw 'Normal sampling unexpectedly active.'}
if ($Scene -eq 'mr' -and $Pbr -and !$NoMrMaps -and $log -notmatch 'mr_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing MR sampled frame evidence.'}
if (($NoMrMaps -or !$Pbr) -and $log -notmatch 'mr_frames=0\b') {throw 'MR sampling unexpectedly active.'}
if ($Pbr -and $log -notmatch 'lighting_smoke mode=DirectPbr pbr_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing PBR frame presentation evidence.'}
($log -split "`n" | Where-Object { $_ -match '^guard_dialog|^unsaved_guard|^save_|^reopened_saved|^edit_preview|^edit_gpu_fault|^edit_published|^edit_smoke_done|^gpu_fault_smoke|^gpu_scene_published|^analysis_complete|^analysis_partial|^analysis_coverage|^gpu_material_images|^asset_smoke|^lighting_smoke|^SMOKE_PASS' }).Trim()
