# Evaluator HTTP repair: 0fc6c1b

Commit: `0fc6c1bf8a3ab02e97c397ae8a4029853a050206`. Exact staged Ponytail review: `e0687c8a3934b2ff96bb445b7dea698f2bc368ea`, no findings. Independent source review also passed.

The evaluator now uses an unproxied temporary Playwright API context when the browser has a container-local proxy. Direct evaluator calls preserve current cookies, cookie updates, User-Agent and explicit headers. Response consumption precedes disposal. Conflicting cookie changes, partitioned cookies and an active network cut remain inconclusive. Routed replies leave response-cookie handling to browser delivery. Unregistered contexts retain their existing API context.

## Repeatable checks

Run from `tools/stack-bench`:

```powershell
npm run build --silent
$env:STACK_BENCH_BROWSER_REQUEST_EVIDENCE = 'local-notes/sol6-four-stack-20260925/browser-request-green.json'
node --test --test-concurrency=3 dist/tests/browser-request.integration.js dist/tests/auth-request-patch.test.js dist/tests/actor-transport-action-executors.test.js dist/tests/runtime-action-executors.test.js dist/tests/response-loss.integration.js
node --test --test-concurrency=2 dist/tests/network-interruption.integration.js dist/tests/transport-cache.integration.js
npm run typecheck
npx eslint src/actions/browser-request.ts src/actions/network-interruption.ts src/actions/actor-transport-action-executors.ts src/actions/runtime-action-executors.ts src/actions/auth-request-patch.ts src/stacks/backends/spacetime-browser-session.ts grader/response-loss.ts tests/browser-request.integration.ts
```

Results: focused **107/107**, observer/interruption **63/63**, typecheck and lint passed. Fixtures use local ephemeral servers and real Chromium, with no model calls. Fixture servers, contexts and browsers close in teardown.

Preserved red evidence: `browser-request-red.log` proves the original unreachable-proxy behavior. `browser-request-route-red.log` proves the delayed-fulfillment cookie conflict and dropped-response cookie effect before the reconciliation guard. Three other failures in that second log were assertions inspecting the short error message instead of its structured finding; the final assertions verify the typed inconclusive result and its reason. The first green build failed on a narrowed page type; the request now obtains its actual owning page instead of widening the actor interface. That failed build is preserved too.

Final logs: `browser-request-green.log`, `browser-request-green.json`, `browser-request-observer.log`, `browser-request-typecheck.log`, `browser-request-lint.log`.

## Docker qualification remains pending

Diagnostic image: `sha256:c39b77a16cc38daaad89f2acfd2207cf9368588188aed9a0f235a1730a08c7d5`. Its 1,132 source/compiled/calibration files match the frozen inputs. Four stacks now run the nine affected checks and 46 registered defect controls, plus the selected null control. See `http-capture-0fc6c1b-launch.json`. This image is explicitly unqualified. Do not apply qualification reuse or launch paid tests until the new evidence passes and normal calibration/release validation succeeds.
