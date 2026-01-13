#!/bin/bash
# SpacetimeDB Paint App - Docker Deployment Script (Bash)
# Run from the paint-app-20260109-180000 directory

set -e

echo "🎨 SpacetimeDB Paint App - Docker Deployment"
echo "============================================="

# Step 1: Clean up any existing containers
echo ""
echo "📦 Step 1: Cleaning up existing containers..."
docker-compose down -v 2>/dev/null || true

# Step 2: Start SpacetimeDB in Docker
echo ""
echo "🚀 Step 2: Starting SpacetimeDB container..."
docker-compose up -d

# Wait for SpacetimeDB to be ready
echo ""
echo "⏳ Waiting for SpacetimeDB to be ready..."
sleep 5

# Step 3: Add Docker server to spacetime CLI
echo ""
echo "🔧 Step 3: Configuring spacetime CLI..."
spacetime server add docker http://localhost:3000 --no-fingerprint 2>/dev/null || true
spacetime server set-default docker

# Step 4: Build the C# module
echo ""
echo "🔨 Step 4: Building C# SpacetimeDB module..."
pushd backend > /dev/null

# Check if .NET 8 SDK is available
if dotnet --list-sdks | grep -q "8\."; then
    echo "✅ .NET 8 SDK found. Building locally..."
    
    # Install wasi workload if not present
    dotnet workload install wasi-experimental 2>/dev/null || true
    
    # Build the module
    dotnet publish -c Release
else
    echo "⚠️  .NET 8 SDK not found. Building in Docker instead..."
    
    # Build using Docker
    docker build --target build -t paint-app-builder .
    docker create --name paint-app-extract paint-app-builder
    docker cp paint-app-extract:/app/backend.wasm ./backend.wasm
    docker rm paint-app-extract
fi

popd > /dev/null

# Step 5: Publish module to SpacetimeDB
echo ""
echo "📤 Step 5: Publishing module to SpacetimeDB..."

# Determine WASM path
WASM_PATH="backend/bin/Release/net8.0/wasi-wasm/AppBundle/backend.wasm"
if [ ! -f "$WASM_PATH" ]; then
    WASM_PATH="backend/backend.wasm"
fi

if [ ! -f "$WASM_PATH" ]; then
    echo "❌ Error: Could not find compiled WASM module"
    exit 1
fi

echo "y" | spacetime publish paint-app --clear-database --bin-path "$WASM_PATH"

# Step 6: Generate client bindings
echo ""
echo "🔄 Step 6: Generating C# client bindings..."
mkdir -p client/module_bindings
spacetime generate --lang csharp --out-dir client/module_bindings --bin-path "$WASM_PATH"

# Step 7: Build client
echo ""
echo "🖥️  Step 7: Building client application..."
pushd client > /dev/null
dotnet restore
dotnet build
popd > /dev/null

echo ""
echo "✅ Deployment complete!"
echo ""
echo "SpacetimeDB is running at: http://localhost:3000"
echo ""
echo "To run the client:"
echo "  cd client"
echo "  dotnet run"
echo ""
echo "To view SpacetimeDB logs:"
echo "  docker-compose logs -f spacetimedb"
echo ""
echo "To stop SpacetimeDB:"
echo "  docker-compose down"
