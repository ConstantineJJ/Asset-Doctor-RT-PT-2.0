param([string[]]$Command)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$names=@('PATH','CARGO_HOME','RUSTUP_HOME','RUSTUP_TOOLCHAIN')
$previous=@{}
foreach ($name in $names) { $previous[$name]=[Environment]::GetEnvironmentVariable($name,'Process') }
$cargoExitCode=1
Push-Location $repo
try {
    # Use the installed MSVC toolchain and preserve the caller's environment.
    Remove-Item Env:RUSTUP_TOOLCHAIN,Env:CARGO_HOME,Env:RUSTUP_HOME -ErrorAction SilentlyContinue
    $cargoBin=Join-Path $env:USERPROFILE '.cargo\bin'
    $env:PATH="$cargoBin;"+[Environment]::GetEnvironmentVariable('Path','Machine')+';'+[Environment]::GetEnvironmentVariable('Path','User')+';'+$previous['PATH']
    & (Join-Path $cargoBin 'cargo.exe') @Command
    $cargoExitCode=$LASTEXITCODE
} finally {
    Pop-Location
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name,$previous[$name],'Process') }
}
exit $cargoExitCode
