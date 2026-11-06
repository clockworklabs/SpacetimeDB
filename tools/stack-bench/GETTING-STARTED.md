# Start your first StackBench run

## Requirements

- A clean, normal Git clone of this repository. Release builds reject a changed
  or worktree checkout.
- Docker running Linux containers for `linux/amd64`, with BuildKit and the
  Compose plugin.
- Internet access for base images, npm packages, and the model provider.
- 4 CPUs, 8 GiB of memory, and 10 GiB of free storage allocated to Docker. The
  first source build needs about 16 GiB of memory and 60 GiB of storage.

Docker keeps the images and results on its configured disk; pick that disk in
Docker Desktop before the first build if the system drive is short of space.
Run one StackBench installation per Docker daemon; the demo shares its state
volume, container names, and ports.

## Run the demo

From the repository root:

```sh
docker compose -f tools/stack-bench/appliance/demo.compose.yaml run --build --rm demo
```

The command builds the images, prepares the state volume, starts the dashboard,
and grades the supplied reference apps on three stacks. It makes no model calls
and needs no credentials. The first build includes the SpacetimeDB server and
SDK and can take a long time; leave the command running until it reports
completion or an error, then open [localhost:7331](http://localhost:7331).

## Run a model

1. Store a provider credential, following
   [provider credentials](appliance/README.md#provider-credentials).
2. In the dashboard, open **New run** and fill in the form. The
   **Production-quality app** setting is on by default and recorded with the
   run; it is the production-quality line described in
   [prompting](docs/prompting.md). In a `job prepare` selections file, the
   field is `productionQuality`.
3. Select **Start run**, then open the run to follow it. Model work spends
   provider credit.

The CLI runs the same flow; see [jobs](appliance/README.md#jobs).

## Read and keep results

Open a run, then an attempt, to see its checks, logs, and transcript. Keeping
and exporting results is in
[results and cleanup](appliance/README.md#results-and-cleanup).

## Stop and clean up

The run's **Stop** control ends its active work and keeps its results. When no
run is active, stop the dashboard and cache containers in Docker Desktop. For
an interrupted run, follow the [recover-run skill](skills/recover-run/SKILL.md).
