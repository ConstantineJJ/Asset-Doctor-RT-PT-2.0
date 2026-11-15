param(
    [ValidateSet('debug','release')][string]$Configuration='release',
    [ValidateSet('static','color','pbr','mr','normal')][string]$Scene='static',
    [switch]$Pbr,
    [switch]$NoMrMaps,
    [switch]$NoNormalMaps,
    [switch]$SynchronizationValidation
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $repo "target\$Configuration\AssetDoctorRTPT.exe"
if (!(Test-Path -LiteralPath $exe)) { throw 'Build the desktop executable first.' }
$good=Join-Path $repo $(switch($Scene) {'color' {'tests\fixtures\Color_Checker.glb'} 'pbr' {'tests\fixtures\Pbr_Factors.glb'} 'mr' {'tests\fixtures\MR_Checker.glb'} 'normal' {'tests\fixtures\Normal_Checker.glb'} default {'tests\fixtures\Twin_Cubes.glb'}})
$bad=Join-Path $repo $(switch($Scene) {'color' {'tests\fixtures\Bad_Image.glb'} 'mr' {'tests\fixtures\Bad_MR_Image.glb'} 'normal' {'tests\fixtures\Bad_Normal_Image.glb'} default {'tests\fixtures\Bad_Truncated.glb'}})
$reports=Join-Path $repo 'artifacts'
New-Item -ItemType Directory -Force -Path $reports | Out-Null
$suffix=if ($SynchronizationValidation) {'-sync'} else {''}
if ($Scene -eq 'color') {$suffix='-color'+$suffix}
if ($Scene -eq 'pbr') {$suffix='-factors'+$suffix}
if ($Scene -eq 'mr') {$suffix='-mr'+$suffix}
if ($Scene -eq 'normal') {$suffix='-normal'+$suffix}
if ($Pbr) {$suffix='-pbr'+$suffix}
if ($NoMrMaps) {$suffix='-no-maps'+$suffix}
if ($NoNormalMaps) {$suffix='-no-normal'+$suffix}
$stdout=Join-Path $reports "$Configuration-import-smoke$suffix.stdout.log"
$stderr=Join-Path $reports "$Configuration-import-smoke$suffix.stderr.log"
# Start-Process joins arguments on Windows: quote each literal path containing spaces.
$arguments=@('--smoke-import',('"'+$good+'"'),'--smoke-bad',('"'+$bad+'"'))
if ($Pbr) {$arguments+='--pbr'}
if ($NoMrMaps) {$arguments+='--no-mr-maps'}
if ($NoNormalMaps) {$arguments+='--no-normal-maps'}
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
if ($Scene -eq 'color' -and $log -notmatch 'gpu_material_images=1 material_textures=2 mip_bytes=20 decoded_images=1 srgb_images=1 linear_images=0') {
    throw 'Missing deduplicated color image upload evidence.'
}
if ($Scene -eq 'mr' -and $log -notmatch 'gpu_material_images=2 material_textures=1 mip_bytes=40 decoded_images=1 srgb_images=1 linear_images=1') {throw 'Missing shared source / separate transfer upload evidence.'}
if ($Scene -eq 'normal' -and $log -notmatch 'gpu_material_images=1 material_textures=1 mip_bytes=20 decoded_images=1 srgb_images=0 linear_images=1') {throw 'Missing MR / normal shared linear upload evidence.'}
if ($Scene -eq 'normal' -and $Pbr -and !$NoNormalMaps -and $log -notmatch 'normal_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing normal-map sampled frame evidence.'}
if (($NoNormalMaps -or !$Pbr -or $Scene -ne 'normal') -and $log -notmatch 'normal_frames=0\b') {throw 'Normal sampling unexpectedly active.'}
if ($Scene -eq 'mr' -and $Pbr -and !$NoMrMaps -and $log -notmatch 'mr_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing MR sampled frame evidence.'}
if (($NoMrMaps -or !$Pbr) -and $log -notmatch 'mr_frames=0\b') {throw 'MR sampling unexpectedly active.'}
if ($Pbr -and $log -notmatch 'lighting_smoke mode=DirectPbr pbr_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing PBR frame presentation evidence.'}
($log -split "`n" | Where-Object { $_ -match '^gpu_material_images|^asset_smoke|^lighting_smoke|^SMOKE_PASS' }).Trim()
