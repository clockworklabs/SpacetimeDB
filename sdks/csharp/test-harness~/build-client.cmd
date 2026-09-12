@echo off
setlocal

set "SCRIPT_DIR=%~dp0"
set "REPO_ROOT=%SCRIPT_DIR%..\..\.."

dotnet build "%REPO_ROOT%\crates\bindings-csharp\BSATN.Runtime\BSATN.Runtime.csproj" -c Release -p:TargetFramework=net8.0 -p:NuGetAudit=false -p:RestoreIgnoreFailedSources=true
if errorlevel 1 exit /b %errorlevel%

dotnet build -p:NuGetAudit=false -p:RestoreIgnoreFailedSources=true
if errorlevel 1 exit /b %errorlevel%
