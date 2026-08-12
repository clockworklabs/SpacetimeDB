# Stack Bench v1 appliance

## Supported v1 environment

The first distributable release supports one environment deliberately:

- a dedicated, disposable Linux x86-64 runner;
- a local Docker Engine with Compose v2;
- no unrelated workloads or credentials on that runner;
- enough resources to pass `preflight`;
- results copied off the runner before it is destroyed.

V1 is not a general workstation installation. The controller needs the Docker
socket to create, identify, and remove exact per-run containers. That socket is
effectively root access to the runner. Host networking is also required so the
controller, build containers, browser grader, dynamic lint server, databases,
and dedicated SpacetimeDB host keep the already-qualified address model. Those
permissions are acceptable only because the supported runner is disposable and
contains no unrelated data.

## Container and data boundaries

| Component | Can see benchmark definitions? | Can see generated app? | Persistent data |
|---|---:|---:|---|
| controller | yes | yes | results and run state |
| coding/build sandbox | no | its one app only | app workspace and its transcript |
| PostgreSQL/MongoDB | no | database traffic only | dedicated database volumes |
| SpacetimeDB host | no | published module only | one run-owned data directory |

The coding container never mounts the controller image, repository, grader,
scenarios, recipes, calibration, or results. It receives only:

- its unique app directory, read-write;
- the exact selected stack dependencies, read-only;
- its per-run provider transcript directory, read-write;
- its per-run SpacetimeDB CLI configuration when that stack is selected;
- the selected provider credential in the narrow form declared by its adapter.

## V1 topology

The controller runs with:

- `network_mode: host`, Linux only;
- `/var/run/docker.sock`, read-write;
- `/var/lib/stack-bench`, bind-mounted at the identical host/container path;
- a read-only release-dependency volume after initialization;
- provider API keys as Compose secrets, never environment entries in the
  Compose file;
- no registry credential mount. The host pulls images before the controller
  starts.

Using the same `/var/lib/stack-bench` path on both sides is intentional. A
sibling container created through the host Docker socket resolves bind-mount
sources on the host, not inside the controller. The fixed identical path makes
workspaces and results name the same bytes from either side without mounting the
harness into a coding container.

Stack-specific binaries and SDK sources use a named, read-only dependency
volume instead of a host source checkout. A one-shot initializer copies the
content embedded in the signed controller release into that volume and writes a
checksum marker. Both the controller and each selected coding container mount
the volume, but coding containers receive only the paths their stack adapter
declares. The initializer refuses an existing volume whose marker does not match
the release manifest.

## Network paths

Host networking keeps the proven runtime paths:

- the controller grader reaches app ports through `127.0.0.1`;
- coding containers reach controller-hosted lint and SpacetimeDB ports through
  `host.docker.internal` plus `host-gateway`;
- coding containers reach the dedicated PostgreSQL/MongoDB published ports the
  same way;
- only provider-declared HTTPS endpoints and package registries are required
  outbound from coding containers;
- the controller needs registry access only during the explicit pull/verify
  step, not during a benchmark attempt.

`preflight --smoke` must exercise these paths from the delivered build image
before a model call. A release is unsupported if host networking, host-gateway,
or the fixed state path is changed.

## Secrets

The release contains a template naming required secrets but no secret values.
For the first live adapter, the operator supplies an API key file through
Compose secrets. The controller reads it only to launch the selected coding
container; it must not write it to an artifact, command log, transcript path,
or long-lived environment block. Coding containers can read their own provider
credential, which remains a declared residual risk.

Developer-home credential mounts are not part of the appliance. They remain a
local-development convenience only. Adding another provider requires its agent
adapter to declare its credential alternatives and outbound HTTPS destinations.

## Release contents

The delivered bundle must include:

- controller and build-sandbox images by registry digest;
- PostgreSQL and MongoDB images by registry digest;
- the SpacetimeDB runtime, CLI, standalone binary, and TypeScript SDK source by
  checksum;
- exact engine, recipe, pack, fixture, calibration, reference, mutation, and
  experiment inputs;
- Compose files and a secrets template;
- a release manifest binding every file and image;
- SPDX or CycloneDX SBOMs for first-party images;
- signatures/attestations for images and the manifest;
- an operator guide and recovery/quarantine instructions;
- a persistent results directory containing raw artifacts and generated report
  output.

Readable tags may appear in commands, but verification and execution use the
manifest digest. A mutable tag alone can never satisfy preflight.

## Operator flow

```text
1. Verify the bundle checksum and signature.
2. Configure registry login and provider secret files.
3. Pull every image from the release manifest by digest.
4. Initialize the release-dependency volume.
5. Run the exact experiment preflight and no-model smoke.
6. Run qualification or the frozen experiment.
7. Generate the deterministic report from retained raw artifacts.
8. Copy results off-runner, verify their manifest, then destroy the runner.
```

## Implementation order

1. Add a strict release-manifest schema and checksum verifier.
2. Teach build-container plans to mount release dependencies from a named
   volume while retaining the current local-development path mode.
3. Build the controller image and one-shot dependency initializer.
4. Add the dedicated-runner Compose file, secret templates, and fixed state
   directory.
5. Generate SBOM/checksum/signature inputs and validate them in CI.
6. Run preflight and the full model-free Docker smoke from a clean Linux runner.
7. Exercise interruption handling before calling the bundle production-ready.

The first five steps are SB-402. Interruption recovery is SB-403; the frozen
multi-stack campaign and deterministic static report are SB-501 through SB-503.

## Acceptance gates

SB-402 is complete only when a clean dedicated runner can, from the delivered
bundle alone:

- verify every file and image identity before execution;
- initialize dependencies without a source checkout;
- run exact-scope preflight and the no-model smoke;
- prove the coding container cannot read definitions or results;
- preserve artifacts after the controller container is removed;
- remove only exact benchmark-owned resources;
- emit no provider, registry, database, or lease secret in public artifacts;
- reproduce the same release identity on a second runner.
