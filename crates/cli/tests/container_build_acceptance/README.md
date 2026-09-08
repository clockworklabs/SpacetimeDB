# Real local container builder acceptance

This opt-in fixture exercises the CLI with actual BuildKit and explicitly
selected Railpack. It does not connect to a SpacetimeDB server. The resulting
verified OCI layouts can be consumed by the separate managed publication
acceptance test.

The checked-in tool lock currently supports macOS on arm64. It records official
release download URLs, SHA-256 checksums, and image digests. The fixture downloads
the two tools into its own workspace and extracts only the expected regular
binary. Nothing is installed globally. Release checksum provenance is recorded
in `tool-lock.json`; image pins were verified against the primary registries.

Build the CLI and deadline harness from the public workspace, without selecting
or connecting to a server:

```sh
cargo build --locked --offline -p spacetimedb-cli --bin spacetimedb-cli
cargo test --locked --offline -p spacetimedb-cli --test real_container_builder --no-run
```

Use the absolute test executable path printed by the second command. Invoke the
script with absolute paths and a new workspace directory:

```sh
python3 crates/cli/tests/container_build_acceptance/acceptance.py \
  --docker-socket /absolute/path/to/verified/docker-desktop.sock \
  --cli /absolute/path/to/public/target/debug/spacetimedb-cli \
  --deadline-test-binary /absolute/path/to/public/target/debug/deps/real_container_builder-HASH \
  --workspace /private/tmp/new-owned-builder-workspace
```

The socket must already be verified as an owned disposable Docker Desktop
endpoint. The script checks its reported host identity again. Every Docker
command specifies that socket and an empty fixture configuration; saved Docker
contexts and registry credentials are not used. The fixture pulls pinned public
images, starts a uniquely named BuildKit container with a dedicated cache volume,
and binds its port to numeric loopback. A private Unix socket forwards only to
that port. The container is limited to two CPUs, 2 GiB of memory and 256 processes.
Container logs rotate at 4 MiB with at most two files. BuildKit requires
privileged execution in this local fixture; the Docker socket
is not mounted into it. This is a trusted-tool test, not a sandbox for arbitrary
build programs.

The checks cover:

- A Dockerfile using a build-secret mount and an explicitly selected Railpack
  shell-script build. Neither successful nor failed build logs may reveal the
  secret, and retained image objects must not contain it.
- Exact digest, size, platform and executable manifest/config/layer closure of
  both prepared outputs. A modified retained object is rejected on import.
- Failed builds and failed Railpack detection, with no automatic builder fallback.
- A destination created while a build is running, which must remain untouched.
- SIGINT cancellation of real `buildctl`, followed by positive PID absence and
  workspace release.
- The same cleanup on a two-second deadline through the production local process
  runner. Only the test runner shortens the normal build deadline.

Commands have finite deadlines. Teardown stops the proxy, validates the exact
owned container's name and label, waits for its successful synchronous removal,
then removes its cache volume. If the run reply is lost, cleanup looks up only
the original generated name and requires the same ownership label. An ambiguous
daemon answer fails cleanup; an arbitrary inspection error is not treated as
proof of absence. A teardown failure fails the fixture. Downloaded binaries,
diagnostic inputs and the two OCI layouts remain in the private workspace; public image cache entries
are not pruned. `acceptance.json` records the completed checks and the output
paths without credentials or build-secret values.

The retained layouts establish build and artifact preparation behavior. They do
not establish container execution, readiness, or isolation in the production
runtime. Those are separate Linux supervisor and Kata acceptance boundaries.

The cleanup failure tests need no Docker daemon or network:

```sh
python3 -B -m unittest discover -s crates/cli/tests/container_build_acceptance -p test_fixture.py -v
```
