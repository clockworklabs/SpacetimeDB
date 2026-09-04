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
controller, build containers, grader, dynamic lint server, databases,
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
- a short-lived token for the controller-owned provider broker.

## V1 topology

The controller runs with:

- `network_mode: host`, Linux only;
- `/var/run/docker.sock`, read-write;
- `/var/lib/stack-bench`, bind-mounted at the identical host/container path;
- a read-only release-dependency volume after initialization;
- provider credential files below the private appliance state root, never
  secret values in the Compose file or process environment;
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
- coding and reference containers share the appliance host network and reach
  controller-hosted lint, SpacetimeDB, and the dedicated PostgreSQL/MongoDB
  published ports through `127.0.0.1`;
- only provider-declared HTTPS endpoints and package registries are required
  outbound from coding containers;
- the controller needs registry access only during the explicit pull/verify
  step, not during a benchmark attempt.

`preflight --smoke` must exercise these paths from the delivered build image in
the same host-network namespace used by benchmark builds before a model call. A
release is unsupported if host networking or the fixed state path is changed.

## Secrets

The release contains a template naming required secrets but no secret values.
The operator supplies the selected provider credential as a secret file. The
controller reads it and starts a provider broker for each coding session. The
coding container receives a random session token and the broker URL. It does
not receive the provider credential or credential file. The broker stops when
the coding session ends.

The controller must not write the provider credential to an artifact, command
log, transcript path, or long-lived environment block. The controller still
has the long-lived credential and root-equivalent Docker access. V1 therefore
requires a dedicated disposable runner and a credential scoped to the
campaign. A shared or persistent host is not supported.

Coding containers drop all Linux capabilities, use `no-new-privileges`, and
have a finite process limit. This prevents a host-network coding session from
using raw sockets to inspect loopback traffic.

Developer-home credential mounts are not part of the appliance. They remain a
local-development convenience only. Adding another provider requires its agent
adapter to declare its credential alternatives and outbound HTTPS destinations.

## Release contents

The delivered bundle must include:

- controller and build-sandbox images by registry digest;
- PostgreSQL, MongoDB, and npm registry cache images by registry digest;
- every digest names the exact Linux/amd64 platform manifest, not a
  multi-architecture index;
- the SpacetimeDB runtime, CLI, standalone binary, and TypeScript SDK source by
  checksum;
- exact engine, recipe, pack, fixture, calibration, reference, mutation, and
  experiment inputs;
- Compose files and a secrets template;
- a release manifest binding every file and image;
- SPDX 2.3 SBOMs bound to every exact first- and third-party image digest;
- registry-attached signatures for images and a detached signature bundle for
  the manifest;
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
5. Inspect the selected test plan.
6. Start the campaign. The controller records the exact plan, runs admission,
   and creates durable state before model work starts.
7. Observe durable state; use logs only to diagnose a live phase or failure.
8. Generate the deterministic report from retained raw artifacts.
9. Copy results off-runner, verify their manifest, then destroy the runner.
```

The plan owns intent. Campaign state owns scheduling. Attempt artifacts own
results. Reports and the dashboard are reproducible views of those records.
No view can schedule work or change a verdict without calling the controller's
bounded command for that state transition.

## Release requirements

A clean dedicated runner must be able to do the following from the delivered
bundle alone:

- verify every file and image identity before execution;
- initialize dependencies without a source checkout;
- run exact-scope preflight and the no-model smoke;
- prove the coding container cannot read definitions or results;
- preserve artifacts after the controller container is removed;
- remove only exact benchmark-owned resources;
- emit a public quarantine artifact and retain private authenticated recovery
  state whenever exact cleanup cannot be proven;
- emit no provider, registry, database, or lease secret in public artifacts;
- reproduce the same release identity on a second runner.
