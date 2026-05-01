[CmdletBinding()]
param(
    [ValidateSet("http-handlers")]
    [string]$Suite = "http-handlers"
)

$ErrorActionPreference = "Stop"

function Find-Emcmake {
    $candidates = @(
        (Get-Command emcmake.bat -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1),
        (Get-Command emcmake -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1)
    ) | Where-Object { $_ }

    if ($candidates.Count -eq 0) {
        throw "Unable to locate emcmake or emcmake.bat."
    }

    return $candidates[0]
}

function Invoke-LoggedCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,
        [Parameter(Mandatory = $true)]
        [string]$LogPath,
        [string]$WorkingDirectory
    )

    if ($WorkingDirectory) {
        Push-Location $WorkingDirectory
    }

    try {
        & $FilePath @Arguments *> $LogPath
        return $LASTEXITCODE
    } finally {
        if ($WorkingDirectory) {
            Pop-Location
        }
    }
}

function New-CompileCase {
    param(
        [string]$Name,
        [string]$RelativePath,
        [ValidateSet("success", "failure")]
        [string]$Expectation,
        [string]$Marker = ""
    )

    return [pscustomobject]@{
        Name = $Name
        RelativePath = $RelativePath
        Expectation = $Expectation
        Marker = $Marker
    }
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$bindingsRoot = Split-Path -Parent (Split-Path -Parent $scriptDir)
$repoRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $bindingsRoot))
$includeDir = Join-Path $bindingsRoot "include"
$buildRoot = Join-Path $scriptDir "build"
$libraryBuildDir = Join-Path $buildRoot "library"
$libraryLogDir = Join-Path $buildRoot "logs"
$templatePath = Join-Path $scriptDir "CMakeLists.module.txt"
$emcmake = Find-Emcmake

$cases = switch ($Suite) {
    "http-handlers" {
        @(
            (New-CompileCase "ok_http_handlers_basic" "cases/http-handlers/ok_http_handlers_basic.cpp" "success")
            (New-CompileCase "error_http_handler_no_args" "cases/http-handlers/error_http_handler_no_args.cpp" "failure" "too few arguments provided to function-like macro invocation")
            (New-CompileCase "error_http_handler_immutable_ctx" "cases/http-handlers/error_http_handler_immutable_ctx.cpp" "failure" "First parameter of HTTP handler must be HandlerContext")
            (New-CompileCase "error_http_handler_wrong_ctx" "cases/http-handlers/error_http_handler_wrong_ctx.cpp" "failure" "First parameter of HTTP handler must be HandlerContext")
            (New-CompileCase "error_http_handler_no_request_arg" "cases/http-handlers/error_http_handler_no_request_arg.cpp" "failure" "too few arguments provided to function-like macro invocation")
            (New-CompileCase "error_http_handler_wrong_request_arg_type" "cases/http-handlers/error_http_handler_wrong_request_arg_type.cpp" "failure" "Second parameter of HTTP handler must be HttpRequest")
            (New-CompileCase "error_http_handler_no_return_type" "cases/http-handlers/error_http_handler_no_return_type.cpp" "failure" "non-void function does not return a value")
            (New-CompileCase "error_http_handler_wrong_return_type" "cases/http-handlers/error_http_handler_wrong_return_type.cpp" "failure" "no viable conversion from returned value of type 'unsigned int' to function return type 'SpacetimeDB::HttpResponse'")
            (New-CompileCase "error_http_handler_no_sender" "cases/http-handlers/error_http_handler_no_sender.cpp" "failure" "no member named 'sender' in 'SpacetimeDB::HandlerContext'")
            (New-CompileCase "error_http_handler_no_connection_id" "cases/http-handlers/error_http_handler_no_connection_id.cpp" "failure" "no member named 'connection_id' in 'SpacetimeDB::HandlerContext'")
            (New-CompileCase "error_http_handler_no_db" "cases/http-handlers/error_http_handler_no_db.cpp" "failure" "no member named 'db' in 'SpacetimeDB::HandlerContext'")
            (New-CompileCase "error_http_router_not_a_function" "cases/http-handlers/error_http_router_not_a_function.cpp" "failure" "illegal initializer")
            (New-CompileCase "error_http_router_with_args" "cases/http-handlers/error_http_router_with_args.cpp" "failure" "too many arguments provided to function-like macro invocation")
            (New-CompileCase "error_http_router_wrong_return_type" "cases/http-handlers/error_http_router_wrong_return_type.cpp" "failure" "no viable conversion from returned value of type 'unsigned int' to function return type 'SpacetimeDB::Router'")
        )
    }
}

New-Item -ItemType Directory -Force -Path $buildRoot | Out-Null
New-Item -ItemType Directory -Force -Path $libraryLogDir | Out-Null

$libraryConfigureLog = Join-Path $libraryLogDir "library-configure.log"
$libraryBuildLog = Join-Path $libraryLogDir "library-build.log"

Write-Host "Building bindings library..."
$configureExit = Invoke-LoggedCommand -FilePath $emcmake -Arguments @(
    "cmake",
    "-S", $bindingsRoot,
    "-B", $libraryBuildDir
) -LogPath $libraryConfigureLog -WorkingDirectory $scriptDir

if ($configureExit -ne 0) {
    Write-Host "Library configure failed. See $libraryConfigureLog"
    exit 1
}

$buildExit = Invoke-LoggedCommand -FilePath "cmake" -Arguments @(
    "--build", $libraryBuildDir
) -LogPath $libraryBuildLog -WorkingDirectory $scriptDir

if ($buildExit -ne 0) {
    Write-Host "Library build failed. See $libraryBuildLog"
    exit 1
}

$results = @()

foreach ($case in $cases) {
    $caseSource = Join-Path $scriptDir $case.RelativePath
    $caseBuildDir = Join-Path $buildRoot $case.Name
    $configureLog = Join-Path $caseBuildDir "configure.log"
    $buildLog = Join-Path $caseBuildDir "build.log"

    if (Test-Path $caseBuildDir) {
        Remove-Item $caseBuildDir -Recurse -Force
    }

    New-Item -ItemType Directory -Force -Path $caseBuildDir | Out-Null
    Copy-Item $templatePath (Join-Path $caseBuildDir "CMakeLists.txt")

    Write-Host "Running $($case.Name)..."
    $configureExit = Invoke-LoggedCommand -FilePath $emcmake -Arguments @(
        "cmake",
        "-S", $caseBuildDir,
        "-B", $caseBuildDir,
        "-DMODULE_SOURCE=$caseSource",
        "-DOUTPUT_NAME=$($case.Name)",
        "-DSPACETIMEDB_LIBRARY_DIR=$libraryBuildDir",
        "-DSPACETIMEDB_INCLUDE_DIR=$includeDir"
    ) -LogPath $configureLog -WorkingDirectory $scriptDir

    $buildExit = 0
    if ($configureExit -eq 0) {
        $buildExit = Invoke-LoggedCommand -FilePath "cmake" -Arguments @(
            "--build", $caseBuildDir
        ) -LogPath $buildLog -WorkingDirectory $scriptDir
    }

    $combinedLog = ""
    if (Test-Path $configureLog) {
        $combinedLog += Get-Content $configureLog -Raw
    }
    if (Test-Path $buildLog) {
        $combinedLog += "`n"
        $combinedLog += Get-Content $buildLog -Raw
    }

    $passed = $false
    $detail = ""
    if ($case.Expectation -eq "success") {
        $passed = ($configureExit -eq 0 -and $buildExit -eq 0)
        if (-not $passed) {
            $detail = "Expected build success."
        }
    } else {
        $failedBuild = ($configureExit -ne 0 -or $buildExit -ne 0)
        $matchedMarker = ($case.Marker -and $combinedLog.Contains($case.Marker))
        $passed = ($failedBuild -and $matchedMarker)
        if (-not $passed) {
            if (-not $failedBuild) {
                $detail = "Expected build failure."
            } else {
                $detail = "Expected marker not found: $($case.Marker)"
            }
        }
    }

    if (-not $passed -and -not $detail) {
        $detail = (($combinedLog -split "`r?`n" | Where-Object { $_.Trim() }) | Select-Object -First 8) -join " "
    }

    $results += [pscustomobject]@{
        Case = $case.Name
        Expectation = $case.Expectation
        Result = if ($passed) { "PASS" } else { "FAIL" }
        Detail = $detail
    }
}

$results | Format-Table -AutoSize

if ($results.Result -contains "FAIL") {
    Write-Host ""
    Write-Host "Failures:"
    $results | Where-Object Result -eq "FAIL" | ForEach-Object {
        Write-Host "- $($_.Case): $($_.Detail)"
    }
    exit 1
}

Write-Host ""
Write-Host "All compile tests passed."
