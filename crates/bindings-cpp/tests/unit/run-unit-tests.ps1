[CmdletBinding()]
param(
    [switch]$Detailed
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$buildDir = Join-Path $scriptDir 'build'
$launcherPath = Join-Path $buildDir 'bindings_cpp_unit_tests.cjs'

$emcmake = Get-Command emcmake.bat -ErrorAction SilentlyContinue
if ($null -eq $emcmake) {
    $emcmake = Get-Command emcmake -ErrorAction SilentlyContinue
}
if ($null -eq $emcmake) {
    throw 'Unable to locate emcmake or emcmake.bat'
}

$node = Get-Command node -ErrorAction SilentlyContinue
if ($null -eq $node) {
    throw 'Unable to locate node'
}

Write-Host ''
Write-Host '==> Configuring unit tests' -ForegroundColor Cyan
& $emcmake.Source cmake -S $scriptDir -B $buildDir
if ($LASTEXITCODE -ne 0) {
    throw "cmake configure failed with exit code $LASTEXITCODE"
}

Write-Host ''
Write-Host '==> Building unit tests' -ForegroundColor Cyan
cmake --build $buildDir --target bindings_cpp_unit_tests
if ($LASTEXITCODE -ne 0) {
    throw "cmake build failed with exit code $LASTEXITCODE"
}

Write-Host ''
Write-Host '==> Running unit tests' -ForegroundColor Cyan
if (-not (Test-Path $launcherPath)) {
    throw "Could not find built bindings_cpp_unit_tests.cjs launcher at $launcherPath"
}
if ($Detailed) {
    & $node.Source $launcherPath -v
} else {
    & $node.Source $launcherPath
}
if ($LASTEXITCODE -ne 0) {
    throw "unit tests failed with exit code $LASTEXITCODE"
}
