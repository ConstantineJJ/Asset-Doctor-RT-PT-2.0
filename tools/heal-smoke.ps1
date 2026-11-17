param([switch]$SynchronizationValidation)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $repo 'target/release/AssetDoctorRTPT.exe'
if (!(Test-Path -LiteralPath $exe)) {throw 'Build the release workspace first.'}
$artifacts=Join-Path $repo 'artifacts'
New-Item -ItemType Directory -Force -Path $artifacts | Out-Null
$copy=Join-Path $artifacts ('Duplicate_Heal_Output_'+[Guid]::NewGuid().ToString('N')+'.glb')
$probeLog=Join-Path $artifacts 'duplicate-heal-probe.log'
$probeOutput=& (Join-Path $PSScriptRoot 'dev.ps1') -Command @('run','-p','ad-heal','--example','duplicate_heal_probe','--locked','--',$copy) 2>&1
$probeExit=$LASTEXITCODE
$probeOutput | Set-Content -LiteralPath $probeLog
if ($probeExit -ne 0 -or ($probeOutput -join "`n") -notmatch 'DUPLICATE_HEAL_PASS before=992 after=960 .*removed=32 revision=3 undo_redo=true source_preserved=true') {throw "CPU Heal probe failed; see $probeLog"}
$copy | Set-Content -LiteralPath (Join-Path $artifacts 'duplicate-heal-probe-path.txt')
$bad=Join-Path $repo 'tests/fixtures/Bad_Normal_Image.glb'
$stdout=Join-Path $artifacts 'duplicate-heal-native.stdout.log'
$stderr=Join-Path $artifacts 'duplicate-heal-native.stderr.log'
$arguments=@('--pbr','--smoke-import',('"'+$copy+'"'),'--smoke-bad',('"'+$bad+'"'))
$previousSync=$env:VK_LAYER_VALIDATE_SYNC
try {
    if ($SynchronizationValidation) {$env:VK_LAYER_VALIDATE_SYNC='1'}
    $process=Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
} finally {$env:VK_LAYER_VALIDATE_SYNC=$previousSync}
if (!$process.WaitForExit(30000)) {Stop-Process -Id $process.Id;throw 'Native Heal readback timed out.'}
$log=Get-Content -LiteralPath $stderr -Raw
if ($process.ExitCode -ne 0 -or $log -notmatch 'SMOKE_PASS .*validation_enabled=true validation_errors=0' -or $log -notmatch 'asset_smoke imported=true mesh_frames=15 rejected_replacement_preserved=true' -or $log -notmatch 'lighting_smoke mode=DirectPbr pbr_frames=15 mr_frames=15 normal_frames=15' -or $log -notmatch 'analysis_complete document=1 revision=0 primitives=4 triangles=3840') {throw "Native Heal readback failed; see $stderr"}
$probeOutput | Where-Object {$_ -match 'DUPLICATE_HEAL_PASS'}
$log -split "`n" | Where-Object {$_ -match 'analysis_complete|lighting_smoke|SMOKE_PASS'}
