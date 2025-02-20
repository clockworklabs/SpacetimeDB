Param(
    [Parameter(Mandatory=$false)]
    [Switch]$Nightly
)

function Install {
    $ErrorActionPreference = 'Stop'
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

    $DownloadUrl = "https://github.com/clockworklabs/SpacetimeDB/releases/latest/download/spacetimedb-update-x86_64-pc-windows-msvc.exe"
    $DownloadUrl = "http://localhost:8000/spacetimedb-update-x86_64-pc-windows-msvc.exe"
    Write-Output "Downloading installer..."

    function UpdatePathIfNotExists {
        param (
            [string]$DirectoryToAdd
        )
        $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
        if (-not $currentPath.Contains($DirectoryToAdd)) {
            [Environment]::SetEnvironmentVariable("Path", $currentPath + ";" + $DirectoryToAdd, "User")
        }
    }
    
    $Executable = Join-Path ([System.IO.Path]::GetTempPath()) "spacetime-install.exe"
    Invoke-WebRequest $DownloadUrl -OutFile $Executable -UseBasicParsing
    & $Executable

    # TODO: do this in spacetimedb-update
    $InstallDir = Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "SpacetimeDB"
    UpdatePathIfNotExists $InstallDir
    Write-Output "We have added spacetimedb to your Path. You may have to logout and log back in to reload your environment."
}

Install

