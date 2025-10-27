Param(
    [Parameter(Mandatory=$false)]
    [Switch]$Nightly
)

function Install {
    $ErrorActionPreference = 'Stop'
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

    function CheckVCRuntime {
        $paths = @(
            "$env:SystemRoot\System32\vcruntime140.dll",
            "$env:SystemRoot\SysWOW64\vcruntime140.dll"
        )
        foreach ($path in $paths) {
            if (Test-Path $path) {
                Write-Host "vcruntime140.dll is already installed at $path"
                return $true
            }
        }
        Write-Host "vcruntime140.dll is not installed"
        return $false
    }

    if (-not (CheckVCRuntime)) {
        $DownloadUrl = "https://aka.ms/vs/17/release/vc_redist.x64.exe"
        Write-Output "Downloading vcruntime140.dll..."
        $Installer = Join-Path ([System.IO.Path]::GetTempPath()) "vc_redist.x64.exe"
        Invoke-WebRequest $DownloadUrl -OutFile $Installer -UseBasicParsing
        Start-Process -Wait -FilePath $Installer -ArgumentList "/quiet", "/install"
    }

    $DownloadUrl = "https://github.com/clockworklabs/SpacetimeDB/releases/latest/download/spacetimedb-update-x86_64-pc-windows-msvc.exe"
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
    Start-Process -Wait -FilePath $Executable

    # TODO: do this in spacetimedb-update
    $InstallDir = Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "SpacetimeDB"
    UpdatePathIfNotExists $InstallDir
    Write-Output "We have added spacetimedb to your Path. You may have to logout and log back in to reload your environment."
}

Install
