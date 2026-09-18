# Convex native integration slice

This model-free fixture tests the native protocol before Convex becomes a
selectable stack. It is not an ecommerce reference, password-auth implementation,
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
