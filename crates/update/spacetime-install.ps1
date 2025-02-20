Param(
    [Parameter(Mandatory=$false)]
    [Switch]$Nightly
)

function Install {
    $ErrorActionPreference = 'Stop'
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

    $DownloadUrl = "https://github.com/clockworklabs/SpacetimeDB/releases/latest/download/spacetime.exe"
    Write-Output "Installing spacetimedb..."

    function UpdatePathIfNotExists {
        param (
            [string]$DirectoryToAdd
        )
        $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
        if (-not $currentPath.Contains($DirectoryToAdd)) {
            [Environment]::SetEnvironmentVariable("Path", $currentPath + ";" + $DirectoryToAdd, "User")
        }
    }

    try {
        $Directory = Join-Path "C:" "Program Files"
        $Directory = Join-Path $Directory "SpacetimeDB"
        $Executable = Join-Path $Directory "spacetime.exe"
        New-Item $Directory -Force -ItemType Directory | Out-Null
        Invoke-WebRequest $DownloadUrl -OutFile $Executable -UseBasicParsing
        UpdatePathIfNotExists $Directory

        Write-Output "We have added spacetimedb to your Path. You may have to logout and log back in to reload your environment."
    } catch {

        Write-Output "Failed to install into C:, we will try to install in your home directory."
        Write-Output "*If you want to install globally, run powershell as admin*"
        $Directory = Join-Path $HOME "SpacetimeDB"
        $Executable = Join-Path $Directory "spacetime.exe"
        New-Item $Directory -Force -ItemType Directory | Out-Null
        Invoke-WebRequest $DownloadUrl -OutFile $Executable -UseBasicParsing
        UpdatePathIfNotExists $Directory
    }
    
    Write-Output ""
    Write-Output "spacetime is installed into $Executable"
    Write-Output "The install process is complete, head over to our quickstart guide to get started!"
    Write-Output "  https://spacetimedb.com/docs/quick-start"
    Write-Output ""
    Write-Output "NOTE: You may have to start a new powershell process to get spacetime to work."
}

Install

