param([ValidateSet('debug','release')][string]$Configuration='release')
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $repo "target\$Configuration\AssetDoctorRTPT.exe"
if (!(Test-Path -LiteralPath $exe)) { throw 'Build the desktop executable first.' }
$reports=Join-Path $repo 'artifacts'
New-Item -ItemType Directory -Force -Path $reports | Out-Null
$stdout=Join-Path $reports "$Configuration-smoke.stdout.log"
$stderr=Join-Path $reports "$Configuration-smoke.stderr.log"
$process=Start-Process -FilePath $exe -ArgumentList '--smoke' -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
# Windows PowerShell 5.1 reports ExitCode only if the handle is cached before exit.
$null=$process.Handle
if (!$process.WaitForExit(30000)) {
    Stop-Process -Id $process.Id
    throw 'Native smoke timed out; see artifacts logs.'
}
$log=Get-Content -LiteralPath $stderr -Raw
if ($process.ExitCode -ne 0) { throw "Native smoke failed (exit $($process.ExitCode)). See $stderr" }
$pass=$log -split "`n" | Where-Object { $_ -match '^SMOKE_PASS .*validation_enabled=true validation_errors=0' }
if (!$pass) { throw 'Missing validated native smoke completion evidence.' }
$pass.Trim()
