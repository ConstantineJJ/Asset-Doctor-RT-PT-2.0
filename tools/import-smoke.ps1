param(
    [ValidateSet('debug','release')][string]$Configuration='release',
    [ValidateSet('static','color','pbr','mr','normal','ao','emission','geometry','edges','edge-limit','components')][string]$Scene='static',
    [switch]$Pbr,
    [switch]$NoMrMaps,
    [switch]$NoNormalMaps,
    [switch]$NoAoMaps,
    [switch]$NoEmission,
    [switch]$SynchronizationValidation
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $repo "target\$Configuration\AssetDoctorRTPT.exe"
if (!(Test-Path -LiteralPath $exe)) { throw 'Build the desktop executable first.' }
$good=Join-Path $repo $(switch($Scene) {'color' {'tests\fixtures\Color_Checker.glb'} 'pbr' {'tests\fixtures\Pbr_Factors.glb'} 'mr' {'tests\fixtures\MR_Checker.glb'} 'normal' {'tests\fixtures\Normal_Checker.glb'} 'ao' {'tests\fixtures\AO_Checker.glb'} 'emission' {'tests\fixtures\Emission_Checker.glb'} 'geometry' {'tests\fixtures\Geometry_Findings.glb'} 'edges' {'tests\fixtures\Edge_Topology.glb'} 'components' {'tests\fixtures\Components_Checker.glb'} 'edge-limit' {'artifacts\Edge_Limit.glb'} default {'tests\fixtures\Twin_Cubes.glb'}})
$bad=Join-Path $repo $(switch($Scene) {'color' {'tests\fixtures\Bad_Image.glb'} 'mr' {'tests\fixtures\Bad_MR_Image.glb'} 'normal' {'tests\fixtures\Bad_Normal_Image.glb'} 'ao' {'tests\fixtures\Bad_AO_Image.glb'} 'emission' {'tests\fixtures\Bad_Emission_Image.glb'} default {'tests\fixtures\Bad_Truncated.glb'}})
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
if ($Scene -in @('edges','edge-limit','components')) {$suffix='-'+$Scene+$suffix}
if ($Pbr) {$suffix='-pbr'+$suffix}
if ($NoMrMaps) {$suffix='-no-maps'+$suffix}
if ($NoNormalMaps) {$suffix='-no-normal'+$suffix}
if ($NoAoMaps) {$suffix='-no-ao'+$suffix}
if ($NoEmission) {$suffix='-no-emission'+$suffix}
$stdout=Join-Path $reports "$Configuration-import-smoke$suffix.stdout.log"
$stderr=Join-Path $reports "$Configuration-import-smoke$suffix.stderr.log"
# Start-Process joins arguments on Windows: quote each literal path containing spaces.
$arguments=@('--smoke-import',('"'+$good+'"'),'--smoke-bad',('"'+$bad+'"'))
if ($Pbr) {$arguments+='--pbr'}
if ($NoMrMaps) {$arguments+='--no-mr-maps'}
if ($NoNormalMaps) {$arguments+='--no-normal-maps'}
if ($NoAoMaps) {$arguments+='--no-ao-maps'}
if ($NoEmission) {$arguments+='--no-emission'}
$previousSync=$env:VK_LAYER_VALIDATE_SYNC
try {
    if ($SynchronizationValidation) { $env:VK_LAYER_VALIDATE_SYNC='1' }
    $process=Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
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
if ($Scene -eq 'edges' -and $log -notmatch 'analysis_complete document=1 revision=0 primitives=7 triangles=13 findings=17') {throw 'Unexpected known edge findings.'}
if ($Scene -eq 'components' -and $log -notmatch 'analysis_complete document=1 revision=0 primitives=3 triangles=7 findings=8') {throw 'Unexpected component findings.'}
if ($Scene -eq 'components' -and $log -notmatch 'analysis_coverage rule=G008 completed_primitives=3 omitted_primitives=0 excluded_triangles=1 reason=None retained_findings=1 retained_count=4') {throw 'Missing source component count/coverage evidence.'}
if ($Scene -eq 'edge-limit') {
    if ($log -notmatch 'analysis_partial document=1 revision=0 primitives=1 triangles=2000000 findings=3') {throw 'Missing preserved core findings after edge refusal.'}
    foreach($rule in @('G001','G002','G003','G004','G005','G008')) {
        if ($log -notmatch "analysis_coverage rule=$rule completed_primitives=1 omitted_primitives=0 excluded_triangles=0 reason=None") {throw "Core coverage lost: $rule"}
    }
    foreach($rule in @('G006','G007')) {
        if ($log -notmatch "analysis_coverage rule=$rule completed_primitives=0 omitted_primitives=1 excluded_triangles=0 reason=Some\(Scratch\)") {throw "Edge refusal not explicit: $rule"}
    }
}
if ($Scene -eq 'geometry' -and $log -notmatch 'analysis_complete document=1 revision=0 primitives=2 triangles=6 findings=9') {throw 'Unexpected known geometry findings.'}
$expectedBindings=if($Scene -in @('color','emission')){2}else{1}
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
if ($Scene -eq 'normal' -and $Pbr -and !$NoNormalMaps -and $log -notmatch 'normal_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing normal-map sampled frame evidence.'}
if (($NoNormalMaps -or !$Pbr -or $Scene -ne 'normal') -and $log -notmatch 'normal_frames=0\b') {throw 'Normal sampling unexpectedly active.'}
if ($Scene -eq 'mr' -and $Pbr -and !$NoMrMaps -and $log -notmatch 'mr_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing MR sampled frame evidence.'}
if (($NoMrMaps -or !$Pbr) -and $log -notmatch 'mr_frames=0\b') {throw 'MR sampling unexpectedly active.'}
if ($Pbr -and $log -notmatch 'lighting_smoke mode=DirectPbr pbr_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing PBR frame presentation evidence.'}
($log -split "`n" | Where-Object { $_ -match '^analysis_complete|^analysis_partial|^analysis_coverage|^gpu_material_images|^asset_smoke|^lighting_smoke|^SMOKE_PASS' }).Trim()
