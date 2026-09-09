# Execution jobs

The campaign fixes the experiment. A job assigns its execution policy and credentials. A worker runs the existing campaign engine. Secret values never belong in a campaign or job record.

## Invariants

- Preserve the full TypeScript server/client guidance selected by the campaign.
- Preserve requested parallelism. Resource waits must be visible; do not rewrite concurrency.
- Give each attempt only its selected credential. Record non-secret credential identity before execution.
- Use immutable submissions and exclusive claims. A lost worker is not permission to run a duplicate.
- Release resources only after verified cleanup. Keep host resource locks local.
- Preserve existing evidence readers and the running campaigns' frozen images.

## Delivery sequence

1. Add named credential profiles and per-attempt assignments using the existing provider adapters and credential broker.
2. Reserve only the dispatched attempt's stack resources. Release each reservation after verified cleanup. Support explicit wait/fail policy and cancellation.
3. Remove arbitrary repetition, initial-duration, and broker-request ceilings. Retain numerical, memory, request-size, cost, authentication, and isolation checks.
4. Add durable, idempotent job submission and a worker command. Snapshot the campaign input. Record the selected host and preserve a claim after worker loss. Reuse the existing atomic record writer and campaign runner.
5. Expose the same submission operation through the authenticated dashboard controls. External services can call the exported submission/worker functions or CLI without implementing campaign internals.
6. Verify synthetic credentials, duplicate submissions, competing workers, cancellation, resource reuse, failure ownership, and retained evidence. Run model-free integration before any paid execution.
7. Supply an opt-in local worker service that polls submitted jobs, uses explicit job concurrency, and drains on shutdown. Reuse the same job claim and runner. Keep per-campaign attempt parallelism unchanged.

The first placement unit is a complete campaign on one worker host. Multiple hosts can claim different jobs. Splitting one campaign across hosts requires a separate distributed attempt coordinator and artifact-transfer contract; do not disguise filesystem locks as that coordinator.

## Local worker status

The seven steps above are implemented. The CLI, authenticated dashboard submission, and opt-in Compose worker use the same job store and campaign runner. Worker concurrency counts campaigns; it does not reduce a campaign's nine requested attempts.

Synthetic tests cover exclusive claims, cancellation, retained failures, placement, concurrent campaigns, graceful drain, and independent named account secrets. They also check that a pre-claim error stops admission and drains active work without cancelling it or retrying the bad job. These tests do not prove shared provider quota enforcement or multi-host operation.

Recovery remains explicit. A retained claim is not a lease that can expire. Inspect the campaign and reconcile its resources before further execution. Do not delete a claim or resubmit the same work to bypass uncertain paid execution. The surrounding service must authorize account access and manage shared account quotas; the local worker supplies neither automatic account rotation nor account-wide spend limits.

An existing service queue and secret store should call this boundary directly. The standalone job store requires a filesystem that supports atomic hard links and rename. It is not an internet-facing authentication service. Shared account spending and provider request coordination belong at the credential service boundary, not in grading.
