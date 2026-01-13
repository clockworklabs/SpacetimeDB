# SpacetimeDB Paint App - Docker Deployment Script (PowerShell)
# Run from the paint-app-20260109-180000 directory

$ErrorActionPreference = "Stop"

Write-Host "🎨 SpacetimeDB Paint App - Docker Deployment" -ForegroundColor Cyan
Write-Host "=============================================" -ForegroundColor Cyan

# Step 1: Clean up any existing containers
Write-Host "`n📦 Step 1: Cleaning up existing containers..." -ForegroundColor Yellow
docker-compose down -v 2>$null

# Step 2: Start SpacetimeDB in Docker
Write-Host "`n🚀 Step 2: Starting SpacetimeDB container..." -ForegroundColor Yellow
docker-compose up -d

# Wait for SpacetimeDB to be ready
Write-Host "`n⏳ Waiting for SpacetimeDB to be ready..." -ForegroundColor Yellow
Start-Sleep -Seconds 5

# Step 3: Add Docker server to spacetime CLI
Write-Host "`n🔧 Step 3: Configuring spacetime CLI..." -ForegroundColor Yellow
spacetime server add docker http://localhost:3000 --no-fingerprint 2>$null
spacetime server set-default docker

# Step 4: Build the C# module
Write-Host "`n🔨 Step 4: Building C# SpacetimeDB module..." -ForegroundColor Yellow
Push-Location backend

# Check if .NET 8 SDK is available
$sdkVersion = dotnet --list-sdks | Select-String "8\."
if (-not $sdkVersion) {
    Write-Host "⚠️  .NET 8 SDK not found. Building in Docker instead..." -ForegroundColor Yellow
    
    # Build using Docker
    docker build --target build -t paint-app-builder .
    docker create --name paint-app-extract paint-app-builder
    docker cp paint-app-extract:/app/backend.wasm ./backend.wasm
    docker rm paint-app-extract
} else {
    Write-Host "✅ .NET 8 SDK found. Building locally..." -ForegroundColor Green
    
    # Install wasi workload if not present
    dotnet workload install wasi-experimental 2>$null
    
    # Build the module
    dotnet publish -c Release
}

Pop-Location

# Step 5: Publish module to SpacetimeDB
Write-Host "`n📤 Step 5: Publishing module to SpacetimeDB..." -ForegroundColor Yellow

# Determine WASM path
$wasmPath = "backend/bin/Release/net8.0/wasi-wasm/AppBundle/backend.wasm"
if (-not (Test-Path $wasmPath)) {
    $wasmPath = "backend/backend.wasm"
}

if (-not (Test-Path $wasmPath)) {
    Write-Host "❌ Error: Could not find compiled WASM module" -ForegroundColor Red
    exit 1
}

Write-Host "y" | spacetime publish paint-app --clear-database --bin-path $wasmPath

# Step 6: Generate client bindings
Write-Host "`n🔄 Step 6: Generating C# client bindings..." -ForegroundColor Yellow
New-Item -ItemType Directory -Force -Path "client/module_bindings" | Out-Null
spacetime generate --lang csharp --out-dir client/module_bindings --bin-path $wasmPath

# Step 7: Build and run client
Write-Host "`n🖥️  Step 7: Building client application..." -ForegroundColor Yellow
Push-Location client
dotnet restore
dotnet build

Write-Host "`n✅ Deployment complete!" -ForegroundColor Green
Write-Host ""
Write-Host "SpacetimeDB is running at: http://localhost:3000" -ForegroundColor Cyan
Write-Host ""
Write-Host "To run the client:" -ForegroundColor Yellow
Write-Host "  cd client" -ForegroundColor White
Write-Host "  dotnet run" -ForegroundColor White
Write-Host ""
Write-Host "To view SpacetimeDB logs:" -ForegroundColor Yellow
Write-Host "  docker-compose logs -f spacetimedb" -ForegroundColor White
Write-Host ""
Write-Host "To stop SpacetimeDB:" -ForegroundColor Yellow
Write-Host "  docker-compose down" -ForegroundColor White

Pop-Location
