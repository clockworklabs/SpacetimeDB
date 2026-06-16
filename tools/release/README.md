# SpacetimeDB Release CLI

A command-line tool for managing SpacetimeDB releases and deployments.

Note: The permissions related to this tool are very complex. For publishing a package you will need to be a member of the clockwork labs org for that package and in the case of crates.io you will need to be added to each package individually. Generally it is recommended to not attempt to run this locally unless you know what you're doing. We recommend using our Github workflow which is already setup with the correct permissions/tokens. This allows anyone to publish a release: https://github.com/clockworklabs/SpacetimeDB/actions/workflows/release.yml

## Key Objectives

1. **Platform Independence**: This tool is designed to use minimal shell scripting and platform-specific commands, making it as platform-independent as possible.

2. **CI/CD Integration**: While the tool can be executed locally, it's primarily designed to run within GitHub workflows. This approach eliminates the need for local tool installations and special permissions or secret keys to perform releases.

3. **Configurability**: The tool provides fine-grained control over which components are released, allowing you to choose exactly what gets released and what doesn't.

## Installation

This tool is part of the SpacetimeDB repository. To install it as a cargo subcommand:

```bash
cd tools/release
cargo install --path .
```

This will install the `cargo-release` binary to your `~/.cargo/bin` directory, allowing you to run it as `cargo release` from anywhere.

To verify the installation:

```bash
cargo release --help
```

## Usage

The release CLI provides commands for releasing various components of the SpacetimeDB ecosystem:

### Crates.io Packages

Release the following packages to crates.io:
- memory-usage
- primitives
- metrics
- bindings-macro
- bindings-sys
- bindings
- data-structures
- client-api-messages
- sats
- lib
- sdk

```bash
cargo release crates 1.2.0
```

You can also perform a dry run to see what would be published without actually publishing:

```bash
cargo release crates 1.2.0 --dry-run
```

After each crate is published, the release waits for that crate version to become visible in the crates.io index before publishing dependent crates.

### NPM Package

Release the TypeScript SDK to npm. This will:
1. Run `pnpm publish` which automatically triggers the `prepublishOnly` script
2. The `prepublishOnly` script will build, test, and size up the package
3. Publish the package to npm as `@clockworklabs/spacetimedb-sdk`
4. Set the dist-tag to `latest`
5. Verify the dist-tags

```bash
cargo release npm 1.2.0
```

You can also perform a dry run to test the build and publish process without actually publishing:

```bash
cargo release npm 1.2.0 --dry-run
```

**Note:** In dry-run mode, `pnpm publish --dry-run` will be executed to verify the build and packaging process works correctly, but the package will NOT be published to npm.

**Prerequisites:**
- Node.js and npm must be installed
- pnpm must be installed (`npm install -g pnpm`)
- You must be logged in to npm (`npm login`)
- You must have publish access to the `@clockworklabs/spacetimedb-sdk` package

### C# SDK (NuGet + Unity SDK)

Release the C# SDK to both NuGet and the Unity SDK repository. This unified release process:
1. Builds the C# DLLs once using `dotnet pack`
2. Publishes NuGet packages to nuget.org:
   - SpacetimeDB.BSATN.Runtime
   - SpacetimeDB.Runtime
   - SpacetimeDB.ClientSDK
   - SpacetimeDB.ClientSDK.Godot
3. Updates the Unity project with new DLLs:
   - Removes existing `sdks/csharp/packages/spacetimedb.bsatn.runtime` directory
   - Runs `dotnet restore` to populate DLLs from NuGet cache (creates `{version}/` directory)
   - Copies `.meta` files from `sdks/csharp/release~/spacetimedb.bsatn.runtime/unversioned/` to packages
   - Commits the changes
4. Publishes to the Unity SDK repository:
   - Fetches the `release/mirror/csharp` branch
   - Creates a git subtree split from `sdks/csharp`
   - Pushes to `clockworklabs/com.clockworklabs.spacetimedbsdk` as `release/latest`
   - Tags the Unity SDK repository with the version

```bash
cargo release csharp 1.2.0
```

You can also perform a dry run to test the build process without publishing:

```bash
cargo release csharp 1.2.0 --dry-run
```

**Note:** In dry-run mode, the DLLs will be built to verify the build process works correctly, but packages will NOT be pushed to NuGet or the Unity SDK repository.

**Why Combined?** The same DLLs are used for both NuGet packages and the Unity SDK. Building them once ensures consistency and avoids potential version mismatches.

**Prerequisites:**
- .NET SDK must be installed
- NuGet CLI must be installed
  - On Linux: `sudo apt-get install nuget mono-complete`
  - On macOS: `brew install nuget`
  - On Windows: Download from https://www.nuget.org/downloads
- Git must be installed
- You must have SSH access to `git@github.com:clockworklabs/com.clockworklabs.spacetimedbsdk.git`
- You must have a NuGet API key configured (set via environment variable or NuGet config)

### Docker Container

Release the SpacetimeDB public Docker container to DockerHub. This will:
1. Build a multi-platform image (linux/amd64, linux/arm64)
2. Push the versioned image (e.g., clockworklabs/spacetime:v1.2.0)
3. Tag the image as :latest

```bash
cargo release docker v1.2.0
```

You can also perform a dry run to test the build process without pushing to DockerHub:

```bash
cargo release docker v1.2.0 --dry-run
```

**Note:** In dry-run mode, the containers will be built locally to verify the build process works correctly, but they will NOT be pushed to DockerHub.

**Prerequisites:**
- Docker must be installed and running
- You must be logged in to DockerHub (`docker login`)
- You must have push access to the clockworklabs/spacetime repository

## Full Release

To perform a full release of all components:

```bash
cargo release --all
```

You can also skip specific targets:

```bash
cargo release --all --skip docker
```

Or skip multiple targets:

```bash
cargo release --all --skip docker --skip nuget
```

## GitHub Workflow Integration

The release tool is integrated with GitHub Actions via the `.github/workflows/release.yml` workflow.

### Workflow Behavior

- **Manual Trigger**: Can be run in either dry-run or actual release mode via workflow_dispatch

### Docker Release Workflow

The Docker release job automatically:
1. Extracts the version from `Cargo.toml`
2. Sets up Docker Buildx for multi-platform builds
3. Builds the Docker images (dry-run mode) or builds and pushes (release mode)
4. Tags the image as `:latest` (release mode only)

### Required GitHub Secrets and Variables

To run the Docker release workflow in non-dry-run mode, you need to configure the following in your GitHub repository:

**Variables** (Settings → Secrets and variables → Actions → Variables):
- `DOCKERHUB_USERNAME`: Your DockerHub username

**Secrets** (Settings → Secrets and variables → Actions → Secrets):
- `DOCKERHUB_TOKEN`: Your DockerHub access token (create one at https://hub.docker.com/settings/security)

For the crates.io release workflow:

**Secrets**:
- `CARGO_REGISTRY_TOKEN`: Your crates.io API token (create one at https://crates.io/settings/tokens)
  - Permissions: The token must be able to publish crates and add crate owners. You must be an owner of all crates that you are publishing otherwise you will get an error when you go to publish.

For the C# SDK release workflow (NuGet + Unity):

**Secrets**:
- `NUGET_API_KEY`: Your NuGet API key (create one at https://www.nuget.org/account/apikeys)
  - Permissions: Just the `push` permission. It is recommended that you scope your token to just the required packages. You can also use wildcards here like `SpacetimeDB.*`.

For the NPM release workflow:

Configure npm trusted publishing for this workflow in the npm package settings.

### Running the Workflow Manually

1. Go to the "Actions" tab in your GitHub repository
2. Select the "Release" workflow
3. Click "Run workflow"
4. Enter the tag you are releasing. Example: `v1.1.1`
5. Choose whether to run in dry-run mode (default: true)
6. Click "Run workflow"

## Development

To add a new release target, implement the `ReleaseTarget` trait and add it to the appropriate modules.
