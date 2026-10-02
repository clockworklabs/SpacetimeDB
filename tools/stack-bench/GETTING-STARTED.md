# Start your first Stack Bench run

## Requirements

- A clean Git clone of this repository.
- Docker running Linux containers, with Docker Compose and BuildKit.
- Internet access for the first image and package downloads.
- At least 4 CPUs and 8 GiB of memory available to Docker. For the first source
  build, plan for 16 GiB of memory and 60 GiB of free Docker storage.

Docker stores the images and results on its configured disk. Choose that disk in
Docker Desktop before the first build if the system drive is short of space.
These are planning allowances, not measured minimums for every run.

Use one installation per Docker daemon. The demo reuses the appliance state volume,
container names, and local ports. Do not run it alongside another Stack Bench
installation on the same daemon.

## Run the model-free demo

From the repository root:

```sh
docker compose -f tools/stack-bench/appliance/demo.compose.yaml run --build --rm demo
```

Wait for startup, then open [Stack Bench](http://localhost:7331). The first source
build includes the server and SDK and can take substantial time. Leave the command
running until it reports completion or an error.

The demo tests supplied reference apps on three stacks. It makes no model calls
and needs no provider credentials. Open its campaign to see progress and results.
This checks the runner; it does not measure a coding model.

## Start a model run

1. Configure a provider credential using the
   [credential setup instructions](appliance/README.md#provider-credentials).
   There is no separate dashboard password.
2. Open **New run**. Select the workload, level, stacks, model, reasoning effort,
   SDK skills, dev workflow, repetitions, repairs, and limits
   ([defaults](dashboard/README.md#pages)). **Production-quality app** is on by default. It adds: “Build a production-quality
   application suitable for real users, not a prototype or demo.” You can turn it
   off before review.
3. Review the configuration, attempt count, and cost cap. Select **Start run**.
4. Open the run to follow its status. Keep excluded and incomplete attempts in
   your review. Provisional results are not qualified comparisons.

The demo and model runs use the same local dashboard. Starting model work can
consume provider credit or account usage; the demo does not.

The production-quality option changes the request, not the checks. It is recorded
with the run. Earlier frozen runs retain their original prompts. Compare results
with the same setting, or label the prompt difference.
For CLI/API run preparation, set `productionQuality` to `false` to opt out; omitted
values default to `true`. Direct coding runs also accept `--no-production-quality`.

## Read and keep results

Open a run, then an attempt, to inspect checks, logs, and transcripts. The campaign's
**Files** menu links to available report files. Check completion and feature
completion are different measures; the dashboard lets you select either.

Results remain in the `stack-bench-state` Docker volume. Follow
[results and export](appliance/README.md#results-and-cleanup) to copy a report and
its evidence to the host before you remove that volume.

## Stop and clean up

Use the run's **Stop** control to cancel its active work. Stopping a run is not a
pause. Wait for cleanup to finish and inspect any cleanup error.

When no run is active, stop the Stack Bench dashboard and cache containers in
Docker Desktop. This preserves results. Do not delete the state volume to stop
the dashboard. For interrupted work, follow [recovery](appliance/RECOVERY.md).

## Use the CLI instead

The controller's `job options`, `job prepare`, and `job start` commands run the
same flow without the browser. `prepare` returns the same review the dashboard
shows; `start` accepts only that review. See the
[run-setup interface](dashboard/README.md#workload-setup-and-ai-access) for the
selection fields, and the [appliance guide](appliance/README.md#run-a-campaign)
for running an authored campaign file directly.

For development and grading internals, use the [documentation index](docs/README.md).
