# Submit execution jobs

A campaign describes a test. A job selects where and with which credentials to run it.
Each job runs one whole campaign on one host. Multiple workers can run different jobs
at the same time. The runner preserves the campaign's requested parallelism.

## Submission

Store the campaign manifest under the appliance results `plans/` directory. Use a frozen
plan for paid work. A draft is accepted only for a non-billable, model-free trial.
Configure [named credential profiles](credential-profiles.md) in trusted worker storage.

Create a submission file:

```json
{
  "key": "release-42-l2-repairs",
  "planFile": "l2-repairs.json",
  "credentials": {
    "adapters": { "claude-code": "claude-work", "codex": "openai-api" }
  },
  "hostId": "worker-east",
  "capacityPolicy": "wait"
}
```

Only name adapters present in the plan. Credentials can also have a `default` and an
`attempts` map keyed by exact compiled attempt IDs. Attempt selections take precedence.
Omit `hostId` to let an eligible worker claim the job. This field restricts placement;
it is not host authentication. Omitting credentials retains the existing operator environment.

Through the controller:

```sh
job submit submission.json
job status <returned-id>
job list --limit 50
job work <returned-id> --host worker-east
job cancel <returned-id>
```

For source development, use `node dist/commands/job-cli.js` before these arguments.
Set `STACK_BENCH_RESULTS_DIR` or pass `--results`. The normal appliance controller command
sets runtime image identity for `job work`. A worker must use the matching frozen controller
and coding images. Named secret paths must exist on that worker.

Submission snapshots the plan. Repeating the same key and request returns the same job.
Reusing a key for different inputs fails. Submission does not start a model call.
`job work` claims and runs one job; an existing task queue can invoke that command on the
chosen worker. Credentials are resolved and pinned at attempt admission, not at submission.

## API and service integration

The existing local dashboard controls expose:

- `POST /api/jobs`: submit the JSON above; returns 202 and the durable job status.
- `GET /api/jobs?limit=50&after=<cursor>`: list one page.
- `GET /api/jobs/<id>`: read status, assigned host, capacity wait, and campaign directory.
- `POST /api/jobs/<id>/cancel`: request cancellation.

Writes require the same origin, browser token, and control-secret headers as existing
dashboard controls. This remains a local operator API. An authenticated product service
can instead call `submitExecutionJob` and `workExecutionJob` from
`src/campaigns/execution-jobs.ts`. Authenticate callers and authorize credential/profile
access before calling them. Scope idempotency keys by caller in that service.

The job records contain references, not secret values or secret file paths. Per-execution
evidence records the admitted credential profile and version. Unexpected credential changes
fail before further provider calls. No automatic account rotation occurs.

## Ownership, waiting, and recovery

### Automatic local dispatch

Run a worker to pick up queued jobs without invoking `job work` for each submission:

```sh
job worker --host worker-east --concurrency 2
```

Concurrency here counts **campaign jobs**, not attempts. Two jobs can each run nine
attempts. Each campaign retains its selected parallelism. There is no fixed job ceiling;
set concurrency to the work the host and selected provider accounts can support.
The worker checks host assignments and uses the same exclusive job claims as `job work`.
It polls the local job store once per second when idle. No second queue or dependency is used.

The appliance provides an opt-in `worker` Compose profile. Set `STACK_BENCH_HOST_ID`
and `STACK_BENCH_JOB_CONCURRENCY`, then start the `worker` service with the normal setup
environment. Starting it authorizes execution of eligible queued jobs. Do not point an
experimental worker at a live queue. Use the controller image required by those plans.

SIGTERM/SIGINT stops new claims and waits for active jobs. Use `job cancel` to stop a
specific campaign. Compose allows 24 hours for draining; override `stop_grace_period`
if admitted jobs can run longer. A forced kill retains claims and requires inspection.
A job failure stays recorded while the worker continues. A store or pre-claim error
stops admission and drains active work, so broken input does not enter a retry loop.

This dispatcher is for the local appliance. At large backlog sizes, use the surrounding
product's durable queue to call `job work`; the local store scans directories. A production
multi-host deployment also needs shared credential quotas, a durable central job store,
and explicit evidence transfer. These are not supplied by the local dispatcher.

Workers claim jobs with an atomic immutable record. A second worker cannot launch the
same job. Claims do not expire: a worker that loses contact may still have paid requests
in flight. A killed worker therefore leaves a retained claim for investigation rather than
an automatic duplicate. Use campaign status, stop, and authenticated reconciliation to
resolve owned resources. Failed jobs are not automatically retried by `job work`.
Reconciliation proves cleanup; it does not restore a live database or agent session.
See [interruption and recovery](../appliance/RECOVERY.md) before releasing retained work.

The worker reserves only the actual stack resources for a dispatched attempt. It releases
them after verified cleanup. `capacityPolicy: "wait"` retains pending work and reports the
capacity wait; `"fail"` returns the resource error. Configuration and credential errors are
not retried as capacity waits. Cancellation reaches the runner and its cleanup path.

Use the same local resource-lock root for all controllers targeting the same Docker host.
For separate hosts, those locks and runtime/work paths must be host-local. A shared job
store must support atomic hard links, rename, and durable writes, and all workers must see
the same job/result paths. Test these properties before using a remote filesystem.

## Current boundaries

- Placement is per campaign. One campaign's attempts are not distributed across hosts.
- This is not a replacement for the surrounding product's queue, authentication, or secret store.
- Shared provider quota and account-wide spend controls are not supplied by this job store.
  Existing per-attempt money limits and staggered provider retries remain enforced.
- Full campaign state is still materialized. The compiler rejects work that cannot fit
  numeric, array, serialization, or available heap limits before expansion. Removing the
  old repetition ceiling does not make memory unlimited.
- No production multi-host throughput claim is made by the model-free local tests.
