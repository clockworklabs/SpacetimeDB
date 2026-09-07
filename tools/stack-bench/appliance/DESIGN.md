# Stack Bench appliance design

The demo runs through one Docker appliance on a computer with Linux-container
Docker and Compose. The runtime targets `linux/amd64`. A clean image build and
an appliance rehearsal remain release gates. Source tests do not prove that
full delivery path.

## Runtime ownership

The trusted controller owns definitions, grading, results, provider secrets,
and the Docker socket. It runs in Docker's host network so its HTTP checks can
reach the app and SpacetimeDB ports published on loopback. The dashboard uses a
bridge and publishes only `127.0.0.1:7331`. Dashboard actions start a fresh
controller through Compose; the dashboard does not execute a campaign in its
own network namespace.

Each real attempt owns one backend container and one bridge. The backend is
the network namespace anchor. Its coding container, browser, smoke container,
and provider broker join that exact namespace. They do not share another
attempt's loopback or backend. PostgreSQL and MongoDB have private per-attempt
credentials. Their native database ports are not published on the host.

The controller writes a private lease and claims capacity and port locks before
it creates resources. It records creation authority before Docker create and
exact IDs before start. A short-lived helper installs native nftables rules
with `NET_ADMIN`, then exits. Only after that step does native backend startup
run. Generated code and the browser cannot change the firewall.

The anchor uses Docker's init process to reap stopped children. SpacetimeDB
restart replaces its server process inside the same namespace. An anchor
restart or replacement invalidates the recorded namespace start time and
requires recovery of the whole attempt.

## Network access

The namespace permits:

- its own loopback services and Docker DNS;
- the trusted npm cache at its exact IPv4 address and TCP port 4873;
- public IPv4 HTTP and HTTPS traffic;
- replies to connections already established by trusted host checks.

It blocks host, private, link-local, and other-attempt destinations outside the
cache exception. Outbound IPv6 is blocked except loopback. Public HTTP and
HTTPS access is intentional; this is not an offline sandbox or a provider
hostname allowlist.

The trusted npm cache temporarily joins each owned bridge. Teardown detaches
only that bridge membership. Cache publishing and user registration are
disabled. Teardown does not reset or stop the shared cache.

The browser has no Docker socket, provider secret, app mount, or TCP control
port. Playwright carries its control pipes through `docker exec`. Page scripts
run in the attempt namespace. The controller can inspect the app over its
loopback-published port without exposing its grader endpoints to page scripts.

## Files and credentials

The `stack-bench-state` named volume holds work, results, secrets, and private
recovery records. Setup asks Docker for the volume's native mountpoint. The
controller mounts it at that same path. Child bind mounts therefore name the
same bytes on Docker Desktop and Linux. There is no host-directory translation
layer and no required host `/var/lib/stack-bench` directory.

The coding container receives its app workspace, transcript directory, and
only the selected stack's declared dependency mounts. It does not receive the
repository, grader, scenarios, recipes, results, Docker socket, or provider
credential file. It uses a read-only root filesystem, resource limits, and
`no-new-privileges`. Trusted setup can switch users; the coding agent runs with
its reduced identity.

The controller gives each provider broker a private credential file and gives
the agent a short-lived session token. The broker runs as a separate container
and records usage for cost reconciliation. Its private files and identity are
retained if the broker cannot be stopped. Public artifacts omit
lease tokens and creation tokens, and public diagnostics redact credentials.

A read-only dependency volume holds the exact SpacetimeDB CLI, server, and SDK
embedded in the controller image. Its initializer verifies a checksum marker
and refuses incompatible existing contents. Setup pulls the pinned native
PostgreSQL and MongoDB images when they are absent.

## Parallel runs and recovery

`STACK_BENCH_RUNNER_CAPACITY` declares the appliance worker pool. A campaign's
`parallelism` uses that pool. Linux `flock` protects complete capacity and port
claims across controller processes. Campaign children receive private,
single-use delegated authority. A worker remains reserved until exact cleanup
is proven. The dashboard and CLI use the same campaign owner checks for Stop.

Every attempt runs its own model-free smoke after lease activation and before
agent execution. Parent admission cannot substitute an earlier smoke result.
Bounded Docker checks have proven private routes, browser control pipes, native
backend activation, concurrent attempts, and exact cleanup. They do not replace
reference grading qualification or a clean release rehearsal.

Normal teardown removes exact owned sidecar and backend IDs, owned database
volumes, and the owned network. Recovery can also find a resource created before
its ID was saved, but only when its private creation label still matches.
Failed cleanup keeps authority and locks for an authenticated retry. Never
remove a same-name resource merely because its name resembles Stack Bench.

The plan owns requested work and rules. Campaign state owns scheduling.
Attempt artifacts own results. Reports and the dashboard show those records;
they do not change grades. Release identity and qualification requirements are
specified in [RELEASE.md](RELEASE.md). Operator steps are in [README.md](README.md),
and interruption handling is in [RECOVERY.md](RECOVERY.md).
