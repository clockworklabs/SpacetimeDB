# Build stage for C# SpacetimeDB module
FROM mcr.microsoft.com/dotnet/sdk:8.0 AS build

# Install wasi-experimental workload for WASM compilation
RUN dotnet workload install wasi-experimental

WORKDIR /src

# Copy project file and restore
COPY backend.csproj .
RUN dotnet restore

# Copy source and build
COPY *.cs .
RUN dotnet publish -c Release

# Runtime stage with the WASM file
FROM alpine:latest AS runtime
COPY --from=build /src/bin/Release/net8.0/wasi-wasm/AppBundle/ /output/
CMD ["ls", "-la", "/output/"]
