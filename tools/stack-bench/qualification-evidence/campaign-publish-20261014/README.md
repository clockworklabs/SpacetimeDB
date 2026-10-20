# Campaign publish check

The CLI exported and uploaded a synthetic, frozen dependency campaign. The real
StackBench API stored one pending upload with two attempts. The repeated upload
returned the same upload ID and `duplicate: true`.

This checks the exporter and upload boundary. It is not reference qualification
or evidence from a paid study. No model calls or stack containers were started.

## Inputs and results

- Fixture owner: `tests/campaign-publish.contract.ts`.
- Base: `tests/fixtures/dependency-model-free-campaign.json`. The test freezes it,
  uses two selected features, adds a prerequisite, disables repair budget, and
  retains one measured failure and one invalid attempt.
- Expected and observed: zero of two features passed. The blocked child remains
  in the total. Unknown cost is null. The invalid attempt has no feature
  measurement and keeps its exclusion reason. Qualification remains pending.
- Six CLI/publish tests passed. They cover missing destination, unfinished work,
  missing/stale reports, file replacement, HTTP rejection, bad acknowledgement,
  response-field filtering and upload delivery.
- All 27 existing report contracts passed, including repeatable reports,
  continuation costs, unknown/bounded cost, comparison cohorts and path guards.
- The actual website check used private suite `phase3-test-a738293d`. The CLI
  called `http://localhost:5175/api/uploads` twice and received upload ID
  `89dfcd2b-8a09-40e0-9a1b-dc95e5d49458`, first as new and then as a duplicate.
- Postgres and the public loader confirmed private storage and the expected
  measurements. The suite, upload, runs and new registry rows were removed.
  Two suites, two uploads and 264 samples remain. The owner's Google account
  and admin session remain.

`identities.json` records the checked source hashes and runtime. `upload-fixture.json`
is the CLI's original file output. The actual website test used the private suite
above instead of that file's `fixture-suite` destination. `site-check.json` records
the database checks and cleanup. The TAP logs retain both the passed checks and
the failed HTTP execution.

## Repeat

From `tools/stack-bench`, compile into an isolated directory and run:

```sh
node node_modules/typescript/bin/tsc -p tsconfig.build.json --outDir .loop-test/publish-check
node --test .loop-test/publish-check/tests/campaign-cli.test.js .loop-test/publish-check/tests/campaign-publish.contract.js
node --test .loop-test/publish-check/tests/campaign-report.contract.js
```

The contract can save its upload with `STACKBENCH_PUBLISH_CHECK_PAYLOAD` and copy
its valid retained campaign with `STACKBENCH_PUBLISH_CHECK_CAMPAIGN`. The copy
destination must not exist. The ordinary test removes its temporary directory.
Register a private site suite with the payload's catalog hash and feature total
before running the copied campaign through `publish --to`. Supply the upload key
only to that process. Remove the private site rows and copied fixture afterward.

## Earlier failures

Fixture setup first used the wrong authored dependency shape, unpinned image
names, a missing admission identity and an inconsistent grade outcome. Those
inputs were corrected through the existing validators. The initial use of the
Linux-only write lock was replaced with the portable active-lock and stable-state
checks used for read-only publishing.

The expanded local HTTP check then failed with `fetch failed` on a reused socket.
Rejected response bodies are now canceled. The HTTP test server closes each
response connection so the publisher's synchronous file checks do not race a
same-process idle socket. The rerun passed. The failed TAP output is retained in
`publish-contract.failed.log`.

Automatic approval review blocked final removal of the ignored isolated build,
copied campaign and temporary site-check files, with reason `blocked by policy`.
They remain under `.loop-test` and the site's ignored design-check output folder.
Database cleanup passed. The isolated compiled CLI can be used until the normal
runner rebuild; active study builds were not changed.
