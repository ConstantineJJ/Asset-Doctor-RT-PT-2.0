param(
    [ValidateSet('debug','release')][string]$Configuration='release',
    [ValidateSet('static','color','pbr')][string]$Scene='static',
    [switch]$Pbr,
    [switch]$SynchronizationValidation
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $repo "target\$Configuration\AssetDoctorRTPT.exe"
if (!(Test-Path -LiteralPath $exe)) { throw 'Build the desktop executable first.' }
$good=Join-Path $repo $(switch($Scene) {'color' {'tests\fixtures\Color_Checker.glb'} 'pbr' {'tests\fixtures\Pbr_Factors.glb'} default {'tests\fixtures\Twin_Cubes.glb'}})
$bad=Join-Path $repo $(if($Scene -eq 'color') {'tests\fixtures\Bad_Image.glb'} else {'tests\fixtures\Bad_Truncated.glb'})
$reports=Join-Path $repo 'artifacts'
New-Item -ItemType Directory -Force -Path $reports | Out-Null
$suffix=if ($SynchronizationValidation) {'-sync'} else {''}
if ($Scene -eq 'color') {$suffix='-color'+$suffix}
if ($Scene -eq 'pbr') {$suffix='-factors'+$suffix}
if ($Pbr) {$suffix='-pbr'+$suffix}
$stdout=Join-Path $reports "$Configuration-import-smoke$suffix.stdout.log"
$stderr=Join-Path $reports "$Configuration-import-smoke$suffix.stderr.log"
# Start-Process joins arguments on Windows: quote each literal path containing spaces.
$arguments=@('--smoke-import',('"'+$good+'"'),'--smoke-bad',('"'+$bad+'"'))
if ($Pbr) {$arguments+='--pbr'}
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
if ($Scene -eq 'color' -and $log -notmatch 'gpu_color_images=1 color_textures=2 mip_bytes=20') {
    throw 'Missing deduplicated color image upload evidence.'
}
if ($Pbr -and $log -notmatch 'lighting_smoke mode=DirectPbr pbr_frames=(1[5-9]|[2-9]\d|\d{3,})\b') {throw 'Missing PBR frame presentation evidence.'}
($log -split "`n" | Where-Object { $_ -match '^asset_smoke|^lighting_smoke|^SMOKE_PASS' }).Trim()
