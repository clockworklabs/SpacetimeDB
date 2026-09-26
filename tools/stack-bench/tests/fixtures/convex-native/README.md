# Convex native integration slice

This model-free fixture tests the native protocol before Convex becomes a
selectable stack. It is not an ecommerce reference, qualified account implementation,
qualification receipt, or paid campaign. Do not supply it to a coding agent.

Runtime pin: `ghcr.io/get-convex/convex-backend@sha256:afbf4292df387c8f031a68d00048551cf1640ddf0013c51ac704a89d7e73e743`
(Linux amd64, backend revision `4e9b83a27fb214e470c9706a0bae234e411d134f`).
The fixture lockfile pins the native client/CLI to `1.46.0`.

Use a fresh, owned backend with its own instance secret and data directory.
Set `CONVEX_SELF_HOSTED_URL` and `CONVEX_SELF_HOSTED_ADMIN_KEY` for that backend.
The admin key is for deployment, seeding and independent observations only.
The two ordinary callers use locally signed test JWTs with different subjects;
only their public verification key is deployed. No cloud account or auth service
is used. These identities do not prove the username/password contract.

Build Stack Bench once, then run from this fixture directory:

```sh
npm ci --ignore-scripts --no-audit --no-fund
node prepare.mjs
node node_modules/convex/bin/main.js dev --once --typecheck disable
node probe.mjs
```

If copied outside the package, set `STACK_BENCH_CONVEX_PROTOCOL` to the file URL
of the compiled `dist/src/stacks/backends/convex-protocol.js` module.
The probe refuses to seed an existing populated fixture. It records native
responses, consistent stock/order snapshots, and the final state in
`probe-evidence.json`. Preserve failed evidence too.

The probe checks accepted purchases, unauthorized and anonymous refusal without
effects, deliberate versus unhandled errors, missing functions, an ordinary
caller's live subscription, independent targeted stock edits, and a committed
purchase whose HTTP response is dropped. A lost response remains unknown to the
caller even when independent state proves the commit. More payloads add no points.

Restart the owned backend with the same storage and credentials, wait for its
API to be ready, then run `node verify-restart.mjs`. Preserve the resulting
`restart-evidence.json`. This observes recovered data; it does not inject a crash
during an operation. Use a fresh owned data directory and redeploy the identical
fixture to test clean initialization again. Do not clear individual tables and
claim that scheduled work and component state were reset.

The snapshot reader uses the documented [Data Sync API](https://docs.convex.dev/deployment-api/data-sync)
and stops only at a consistent `upToDate` result. Raw pages preserve exact native
timestamps. Its 30-page bound is for this tiny fixture, not a production limit.
The stock writer uses the vendor's internal
[`patchDocumentsFields`](https://github.com/get-convex/convex-backend/blob/4e9b83a27fb214e470c9706a0bae234e411d134f/npm-packages/system-udfs/convex/_system/frontend/patchDocumentsFields.ts)
system mutation. It is independent of fixture business handlers and preserves
document IDs and other fields. This is a pinned compatibility dependency, not a
stable public write API. Stack adoption must explicitly retain that dependency
and its regression test or choose another proven mechanism.

Production lifecycle integration must use the existing namespace anchor and
leased stable ports. Sharing a helper's network directly with a backend container
does not preserve reachability when that container is restarted. This fixture
does not qualify firewall isolation, host-browser routing, actual UI capture,
full auth, order-interface mapping, crash controls, or concurrent snapshot updates.
Remove only owned containers/volumes and local dependency installs after the probe.

## Local account candidate

On a separate fresh deployment, run `node prepare-auth.mjs`, deploy with the same
CLI command, then run `node probe-auth.mjs` instead of the signed-identity probe.
The lockfile pins Convex Auth `0.0.95` and Auth Core `0.41.1`. Preparation supplies
local JWT keys to the owned deployment and uses its own HTTP endpoint for JWKS.
There is no external identity service. The browser probe uses the controller's
Playwright installation; set `PLAYWRIGHT_MODULE` to its module file if elsewhere.

The candidate uses the documented ConvexCredentials provider and public
createAccount/retrieveAccount helpers. The account key is the exact username;
no email address is required. Existing username/password bounds are preserved.
This is fixture configuration, not new agent guidance.
The protected purchase reads the actual issued identity and live session record.
The small browser client covers signup/login, a native WebSocket purchase, reload,
the real session-token hook, signout and refused calls with independent state checks.
It does not implement refresh-token rotation or production UI.

The unmodified pinned Password provider issues a session for a
duplicate signup when its supplied password matches the existing account. Check
1c requires a taken username to be refused. The candidate passes a fresh,
server-generated registration nonce through the supported custom user profile.
The atomic createAccount result must contain that nonce before the provider can
return a user ID and issue a session. An existing user retains its original nonce,
so a duplicate cannot sign in. There is no existence precheck or vendor patch.
Login retains retrieveAccount and the same pinned Scrypt implementation as the
library Password provider. The probe tests 36 concurrent signups across six
same/different-password races, each with one account, user and initial session.
Removing the nonce guard makes the control fail with six winners.
Convex Auth and the deprecated Lucia dependency still need an explicit
maintenance decision before promotion. This bounded probe is not qualification.

Sources: [Password configuration](https://labs.convex.dev/auth/config/passwords),
[manual setup](https://labs.convex.dev/auth/setup/manual),
[Credentials provider](https://labs.convex.dev/auth/api_reference/providers/ConvexCredentials),
[public account helpers](https://labs.convex.dev/auth/api_reference/server), and
[custom user schema](https://labs.convex.dev/auth/setup/schema). The exact duplicate
behavior was checked in the installed package's
`src/server/implementation/mutations/createAccountFromCredentials.ts` and live.

## Namespace, restart and reset probe

Keep one named namespace anchor running. Publish the fixture frontend (3100),
native API (3210) and HTTP actions (3211) on that anchor, bound to host loopback.
The backend and fixture helper join `container:<anchor>`; only the backend mounts
its named data volume. Do not join the helper to the replaceable backend itself.
For this feasibility probe, Docker-assigned host ports avoid collisions. They
are not production allocation: the adapter must use Stack Bench's leased windows.

With the local-account fixture deployed, run these phases from its helper:

1. `node probe-lifecycle.mjs prepare`: create a real account and purchase, and
   observe a scheduled marker execute as a positive control.
2. Keep `node probe-lifecycle.mjs serve` running in the named helper.
3. Run `probe-lifecycle-browser.mjs <page-url> HostUser <output.json>` from the
   host and the same script with `ContainerUser` from the helper. Set
   `PLAYWRIGHT_MODULE` to each platform's installed module. Host page URL is
   `http://127.0.0.1:<frontend>/?api=http%3A%2F%2F127.0.0.1%3A<api-port>`;
   helper URL is `http://127.0.0.1:3100`. Each browser signs up, purchases and reloads.
4. `capture` records independent rows and IDs. Restart only the backend with its
   volume and credentials retained, wait for `/version`, then run `warm`.
5. `schedule-reset` records a pending marker due in 20 seconds. Remove only the
   owned backend and its complete data volume; start the same image with a fresh
   volume and deployment credentials. Run `prepare-auth.mjs` and deploy identical
   fixture source, then run `reset`. It requires empty data/accounts, refuses old
   credentials and observes no marker past its due time. It then checks fresh
   initialization and registration. It waits at most 30 seconds.

Keep the anchor ID and published ports unchanged through both operations. The
reset includes backend storage, not a collection of table-clearing operations.
Preserve phase receipts and source hashes, but do not export `lifecycle-private.json`
or the temporary admin environment. The fixture UI has a seed-specific item ID;
fresh production initialization must restart the app frontend too. This probe
does not establish crash-mid-operation safety, component cleanup, firewall
isolation, refresh rotation or full lifecycle integration. The scheduled marker
uses the documented [scheduler and system-table API](https://docs.convex.dev/scheduling/scheduled-functions).
Remove the helper, backend, anchor and owned volume after preserving evidence.

## Owned lifecycle integration

`tests/convex-owned-lifecycle.integration.ts` calls the private Convex lifecycle
through normal lease, namespace, firewall, browser-container and teardown owners.
It is opt-in with `STACK_BENCH_CONVEX_OWNED_TEST=1` inside the Linux controller.
Mount this fixture at `STACK_BENCH_CONVEX_FIXTURE`, an evidence directory at
`STACK_BENCH_CONVEX_EVIDENCE_DIR`, and the normal shared resource-lock directory.
The test claims ports 14309/14310/14311 and uses 14312/14313/14314 for one
overlapping attempt at a time. These explicit test ports do not define the
future campaign allocation.
The trusted controller deploys this fixture. Only the native client probe runs
inside the restricted browser container; its read-only/noexec policy is retained.
This does not yet test generated-app deployment from a coding container.

`probe-owned.mjs` tests real password signup, signed native HTTP and WebSocket
calls, a purchase, and anonymous refusal without stored effects. Warm restart
must retain stored rows and the original token. Full reset must reject old
credentials and remove pending scheduled work before fresh signup and purchase.
The integration test checks two live attempts for separate accounts/data,
cross-deployment credential refusal, and blocked cross-attempt network access.
It also checks cleanup after a real port-binding failure and a startup-process
interruption after the durable container record. Each cleanup must preserve
the surviving app's stored rows and original session.

The test checks the published API and HTTP-action JWKS endpoint, wrong lease
token refusal, stale process-record refusal, released locks and removed owned
volumes. A retained-state negative control must fail the reset observer. Public
receipts contain no admin key or session token. This slice does not qualify
Chromium UI behavior, component state, full campaign cancellation, controller
death recovery, mid-operation crash checks, or full Convex support. Convex stays
unregistered.

## Coding-container deployment proof

`tests/convex-coding-deploy.integration.ts` deploys the same pinned app through
the normal coding-container preparation, isolation, workspace and teardown
owners. A trusted programmatic preparation plan permits this unregistered stack;
it cannot start a paid session and adds no CLI override or registry entry.

The test requires the normal daemon-visible `STACK_BENCH_WORK_DIR` and a pinned
`STACK_BENCH_BUILD_IMAGE`. Only app source and setup files enter `/app`. Install,
local auth setup and native deployment run as developer UID 10001 with the owned
backend's admin key. The coding container receives no grader, probe, reader,
Docker socket, provider credential or lease token. Native browser calls and an
independent stored-state reader then test the deployed app. Submitted and
deployed app hashes must match; generated auth configuration is checked
separately. Reuse must keep the same coding-container identity, and cleanup must
record workspace handback before removing the container and workspace.

This is a model-free fixture deployment proof. It does not qualify paid-agent
execution, native grader integration, campaign allocation or full Convex support.
