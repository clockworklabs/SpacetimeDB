# Local container image preparation

`spacetime container build` reads one database's `container` declaration from
`spacetime.json` and prepares a verified local OCI image. It does not resolve a
database through a server or publish anything. The optional `DATABASE` argument
selects an exact local configuration target. Omit it only when the configuration
contains one container target. Container declarations do not pass to children.
The command is dispatched before saved CLI server settings or credentials are
opened; only project configuration and explicitly selected build credentials
are read.

This slice implements local preparation. Managed publication and container
lifecycle commands are separate integration work. The existing `publish` command
rejects a selected container declaration so it cannot silently publish only the
module.

## Configuration

```json
{
  "database": "example",
  "container": {
    "image": {
      "build": {
        "builder": "dockerfile",
        "context": ".",
        "dockerfile": "Dockerfile"
      }
    },
    "env_keys": ["API_KEY"],
    "resources": {
      "cpu_millicores": 1000,
      "memory_bytes": 1073741824,
      "scratch_bytes": 1073741824,
      "pids_max": 256
    }
  }
}
```

`image` accepts exactly one source:

- `{"build":{"context":"."}}` selects Dockerfile by default.
- `{"build":{"builder":"railpack","context":"."}}` explicitly selects Railpack.
- `{"oci_ref":"registry.example/team/image:tag"}` imports a registry image.
- `{"oci_ref":"oci:./existing-layout"}` imports a local OCI image-layout directory.

Build contexts and local layout paths are relative to the configuration
directory. The Dockerfile path is relative to its build context. A tag is
resolved once during import; the result records the selected immutable manifest
digest. Registry import requires Skopeo. A local directory import requires no
external image tools.

The image's `Entrypoint` followed by `Cmd` supplies the command by default.
`command` replaces the complete argv. Image `User` and `WorkingDir` are preserved
unless `user` or `working_directory` overrides them. An omitted or empty image
`WorkingDir` normalizes to `/`. Image `Env` remains in the
immutable configuration blob. `env_keys` contains runtime store references,
never runtime values. Reserved `SPACETIMEDB_` keys are rejected. Mounts must be
empty in Stage 1. Shared resource, startup-string, port, and environment-key
limits apply to the normalized specification.

## Tools and credentials

Source builds require an explicitly selected local BuildKit Unix socket:

```sh
spacetime container build example \
  --project-path ./project \
  --platform linux/amd64 \
  --out-dir ./prepared-image \
  --buildkit-host unix:///absolute/path/to/disposable-buildkit.sock
```

Use an endpoint you own and have verified. The command never selects a saved
Docker daemon or BuildKit endpoint. This implementation invokes `buildctl`
directly and currently runs external tools on Linux and macOS. An existing local
OCI directory can also be imported without those subprocesses on Windows.
`--platform` is required and accepts `linux/amd64` or `linux/arm64`; it never
silently uses the build computer's platform. `--out-dir` must not already exist,
and its parent must exist.

`--buildctl`, `--skopeo`, and `--railpack` select absolute executable paths or
command names on PATH. Missing tools
produce installation/path errors. No executable is automatically downloaded.
Railpack is pinned to `0.35.0`, paired with
`ghcr.io/railwayapp/railpack-frontend:v0.35.0`. Detection or version failures end
the build without changing builders. Its `prepare` output is given to BuildKit's
matching gateway frontend, following the [Railpack production integration](https://railpack.com/platforms/running-railpack-in-production).

Registry access is anonymous unless `--registry-auth-file FILE` explicitly names
an auth JSON file. The CLI creates an isolated auth directory and does not load
saved Docker credentials. Registry credentials are distinct from Spacetime
credentials and are never copied into prepared metadata.

Pass build secrets as `--build-secret NAME=FILE`. Files enter BuildKit's secret
interface; Railpack receives only their names during plan generation. Builds
with secrets disable cached build results. Build secrets are separate from
`env_keys`, and prebuilt imports reject them. Credential and secret files are
bounded to 1 MiB each. Subprocess environments are cleared except for basic
executable/temporary-directory paths and the explicitly isolated tool paths.
Do not put secret values in a Dockerfile, image command, or image defaults.

## Output and failure behavior

The new output directory contains an OCI image layout plus `prepared.json`:

- `container` is the normalized container specification.
- `manifest` is the selected immutable manifest descriptor.
- `objects` contains the executable manifest/config/layer closure, with digest,
  media type, size, purpose, and a path relative to the layout.

Prepared metadata omits image environment values and build credentials. The
image configuration blob necessarily retains its image defaults. Consumers must
verify descriptors when reopening output; the local metadata is not an
admission receipt from a server.

`prepare_container` returns `PreparedContainer`, which owns its temporary
layout. Dropping it removes the temporary output. `persist` transfers the verified
layout with an atomic operation that does not replace an existing output.
Credentials, plans, and intermediate builder files remain in the temporary
workspace and are removed. Failed verification never returns a prepared object.
The shared `Runner` interface permits structured fake builders in tests and
reuse by a later managed publication path.

Verification checks manifest/config digests, platform, exact compressed layer
digests, uncompressed diff IDs, and bounded tar structure. Layers are inspected
as streams and are never unpacked into the project. Imports reject archive
links, special entries, unknown paths, and duplicate object paths. Bounds include
256 layers, 64 GiB compressed image data, 128 GiB expanded tar data, and one
million expanded entries, with the shared per-object/decompression limits.
Two image tools and two verification workers may run concurrently. Tool calls
have a 30-minute deadline; verification has a 5-minute deadline. Captured stdout
and stderr each have a 4 MiB limit and are not echoed, avoiding accidental
secret disclosure in tool diagnostics.

Cancellation retains the tool's workspace until its Unix process group has been
signalled and its leader reaped. The group leader remains unreaped until the
signal, preventing reuse of its numeric process-group identity. This cleanup is
for locally trusted tools; it is not a sandbox for a tool that deliberately
escapes its group. BuildKit daemon work is subject to the selected daemon's own
client-disconnect cancellation and retention policies. Blocking verification
retains its worker permit and workspace while checking cancellation between
bounded reads.

Tests use generated local OCI fixtures and fake tool invocations, including an
owned shell fixture for process cleanup. They do not execute Docker, BuildKit,
Railpack, Skopeo, or any server operation. Actual supported-builder acceptance
remains a separate integration check.
