param([string[]]$Command)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$localTools=Join-Path (Split-Path $repo -Parent) '.toolchain'
if (Test-Path (Join-Path $localTools 'cargo\bin\cargo.exe')) {
    $env:CARGO_HOME=Join-Path $localTools 'cargo'
    $env:RUSTUP_HOME=Join-Path $localTools 'rustup'
    $env:PATH="$env:CARGO_HOME\bin;$env:PATH"
    # The portable GNU compiler is an experiment fallback. CI uses MSVC.
    $env:RUSTUP_TOOLCHAIN='stable-x86_64-pc-windows-gnu'
    $gnuBin=Join-Path $localTools 'rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained'
    $env:PATH="$gnuBin;$env:PATH"
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=Join-Path $gnuBin 'x86_64-w64-mingw32-gcc.exe'
}
Push-Location $repo
try { & cargo @Command; exit $LASTEXITCODE } finally { Pop-Location }
