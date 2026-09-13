# Compile the actual CLI libtest, then run only owned local exec fixtures.
# A forced cleanup is a failed gate, never evidence that the fixture joined.
# These receipts cover direct children. Each native fixture owns and joins its
# own children; the wrapper does not claim independent descendant retirement.
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or $env:RUNNER_OS -ne 'Windows') {
    throw 'This gate requires the native Windows CI runner.'
}

$workspace = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$results = Join-Path $env:RUNNER_TEMP ('container-exec-' + [Guid]::NewGuid().ToString('N'))
if (Test-Path $results) { throw 'The owned result directory already exists.' }
$null = New-Item -ItemType Directory -Path $results
$runtime = Join-Path $results 'runtime'
$null = New-Item -ItemType Directory -Path $runtime
"WINDOWS_EXEC_RESULTS=$results" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
$windows = [Environment]::GetFolderPath([Environment+SpecialFolder]::Windows)
if (-not [IO.Path]::IsPathFullyQualified($windows) -or -not (Test-Path $windows -PathType Container)) {
    throw 'The Windows system directory is unavailable.'
}
$system32 = Join-Path $windows 'System32'
$report = [ordered]@{
    state = 'running'
    windows = [Environment]::OSVersion.VersionString
    processes = [Collections.Generic.List[object]]::new()
}

function Invoke-OwnedProcess {
    param([string]$Executable, [string[]]$Arguments, [string]$Name,
          [int]$TimeoutSeconds, [switch]$Sterile)
    $stdoutPath = Join-Path $results "$Name.stdout"
    $stderrPath = Join-Path $results "$Name.stderr"
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Executable
    $start.UseShellExecute = $false
    $start.WorkingDirectory = if ($Sterile) { $runtime } else { $workspace }
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    if ($Sterile) {
        $start.Environment.Clear()
        $start.Environment['SystemRoot'] = $windows
        $start.Environment['WINDIR'] = $windows
        $start.Environment['PATH'] = $system32
        $start.Environment['COMSPEC'] = Join-Path $system32 'cmd.exe'
        $start.Environment['TEMP'] = $runtime
        $start.Environment['TMP'] = $runtime
        $start.Environment['RUST_BACKTRACE'] = '1'
    }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    $stdout = [IO.File]::Open($stdoutPath, 'CreateNew', 'Write', 'ReadWrite')
    $stderr = [IO.File]::Open($stderrPath, 'CreateNew', 'Write', 'ReadWrite')
    $cancel = [Threading.CancellationTokenSource]::new()
    $receipt = [ordered]@{
        name = $Name; started = $false; waited = $false; forced = $false; streamsJoined = $false
        cleanupErrors = [Collections.Generic.List[string]]::new()
    }
    $report.processes.Add($receipt)
    $copies = @()
    $failure = $null
    $clock = [Diagnostics.Stopwatch]::StartNew()
    try {
        if (-not $process.Start()) { throw 'The owned child did not start.' }
        $receipt.started = $true
        $receipt.pid = $process.Id
        $process.StandardInput.Close()
        $copies = @(
            $process.StandardOutput.BaseStream.CopyToAsync($stdout, 81920, $cancel.Token),
            $process.StandardError.BaseStream.CopyToAsync($stderr, 81920, $cancel.Token)
        )
        while (-not $process.WaitForExit(100)) {
            if ($clock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { throw 'The owned child exceeded its time bound.' }
            if ($stdout.Length + $stderr.Length -gt 64MB) { throw 'The owned child exceeded its output bound.' }
        }
        $receipt.waited = $true
        $receipt.exitCode = $process.ExitCode
        if (-not [Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]$copies).Wait(5000)) {
            throw 'The exited child retained output handles.'
        }
        $receipt.streamsJoined = $true
        if ($stdout.Length + $stderr.Length -gt 64MB) { throw 'The owned child exceeded its output bound.' }
        if ($process.ExitCode -ne 0) { throw "The $Name child failed; see retained output." }
    } catch {
        $failure = $_
        $receipt.failure = $_.Exception.Message
    } finally {
        try {
            try {
                if ($receipt.started -and -not $process.HasExited) {
                    $receipt.forced = $true
                    $process.Kill($true)
                }
            } catch {
                $receipt.cleanupErrors.Add('Terminate direct child/tree: ' + $_.Exception.Message)
                if ($null -eq $failure) { $failure = $_ }
            }
            try {
                if ($receipt.started) {
                    if (-not $process.WaitForExit(30000)) { throw 'The direct child did not terminate.' }
                    $receipt.waited = $true
                    $receipt.exitCode = $process.ExitCode
                }
            } catch {
                $receipt.cleanupErrors.Add('Join direct child: ' + $_.Exception.Message)
                if ($null -eq $failure) { $failure = $_ }
            }
            try {
                if (-not $receipt.streamsJoined) {
                    $cancel.Cancel()
                    if ($receipt.started) {
                        try { $process.StandardOutput.Close() } finally { $process.StandardError.Close() }
                    }
                    foreach ($copy in $copies) {
                        try { $copy.GetAwaiter().GetResult() } catch { }
                    }
                    $receipt.streamsJoined = $true
                }
            } catch {
                $receipt.cleanupErrors.Add('Join output readers: ' + $_.Exception.Message)
                if ($null -eq $failure) { $failure = $_ }
            }
        } finally {
            $receipt.seconds = $clock.Elapsed.TotalSeconds
            $stdout.Dispose()
            $stderr.Dispose()
            $cancel.Dispose()
            $process.Dispose()
        }
    }
    if ($null -ne $failure) { throw $failure }
    return [IO.File]::ReadAllText($stdoutPath)
}

try {
    $cargo = (Get-Command cargo -CommandType Application).Source
    $rustc = (Get-Command rustc -CommandType Application).Source
    $report.rust = Invoke-OwnedProcess $rustc @('--version', '--verbose') 'rust-version' 30
    $toolchain = Get-Content -Raw (Join-Path $workspace 'rust-toolchain.toml')
    $channel = [regex]::Match($toolchain, '(?m)^channel = "([0-9]+\.[0-9]+\.[0-9]+)"\r?$')
    if (-not $channel.Success -or
        $report.rust -notmatch ('(?m)^release: ' + [regex]::Escape($channel.Groups[1].Value) + '\r?$') -or
        $report.rust -notmatch '(?m)^host: x86_64-pc-windows-msvc\r?$') {
        throw 'This gate requires the repository MSVC toolchain.'
    }
    $build = Invoke-OwnedProcess $cargo @('test', '--locked', '--release', '-p', 'spacetimedb-cli', '--lib', '--no-run', '--message-format=json') 'compile' 600
    $artifacts = @($build -split '\r?\n' | Where-Object { $_ } | ForEach-Object {
        $message = $_ | ConvertFrom-Json
        if ($message.reason -eq 'compiler-artifact' -and $message.profile.test -eq $true -and
            $message.target.name -eq 'spacetimedb_cli' -and $message.target.kind -contains 'lib' -and $message.executable) {
            $message.executable
        }
    } | Select-Object -Unique)
    if ($artifacts.Count -ne 1) { throw 'Expected exactly one actual CLI libtest artifact.' }
    $binary = (Resolve-Path $artifacts[0]).Path
    $report.executable = $binary
    $report.executableSha256 = (Get-FileHash -Algorithm SHA256 $binary).Hash.ToLowerInvariant()
    $prefix = 'subcommands::container::execute::'
    $required = @(
        'native_windows_pipe_backpressure_cancellation_joins_both_workers',
        'native_windows_overlapped_pipe_cancel_observes_exact_completion',
        'native_windows_overlapped_pipe_suppresses_inherited_completion_port_packets',
        'native_windows_close_before_ready_restores_then_releases_callback',
        'native_windows_conpty_normal_error_cancel_restore_console',
        'native_windows_console_cleanup_reports_drain_failure'
    )
    $listing = Invoke-OwnedProcess $binary @($prefix, '--list') 'listing' 30 -Sterile
    $selected = @()
    foreach ($suffix in $required) {
        $matches = [regex]::Matches($listing, '(?m)^(' + [regex]::Escape($prefix) + '[^\r\n]*::' + [regex]::Escape($suffix) + '): test\r?$')
        if ($matches.Count -ne 1) { throw "Missing or ambiguous required native test: $suffix" }
        $selected += $matches[0].Groups[1].Value
    }
    $report.requiredNativeTests = $selected
    $output = Invoke-OwnedProcess $binary @($prefix, '--test-threads=1', '--show-output', '--format=pretty') 'tests' 240 -Sterile
    foreach ($name in $selected) {
        if (-not [regex]::IsMatch($output, '(?m)^test ' + [regex]::Escape($name) + ' \.\.\. ok\r?$')) {
            throw "Required native test did not pass: $name"
        }
    }
    if (-not [regex]::IsMatch($output, '(?m)^test result: ok\. [1-9][0-9]* passed; 0 failed;')) {
        throw 'The exec suite did not report a nonempty passing result.'
    }
    $report.state = 'passed'
} catch {
    $report.state = 'failed'
    $report.failure = $_.Exception.Message
    throw
} finally {
    $report | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $results 'report.json')
}
