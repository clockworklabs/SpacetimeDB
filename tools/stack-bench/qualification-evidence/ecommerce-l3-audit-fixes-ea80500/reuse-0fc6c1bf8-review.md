# 0fc6c1bf8 evaluator-request reuse review

Proposal only. Native qualification is still running. This note does not qualify the candidate or change historical results.

Preparation is currently blocked by an in-progress response-loss cookie investigation. The unchanged-source assertion rejected the edited `tests/response-loss.integration.ts`. The unregistered response-loss path can apply an upstream Set-Cookie through BrowserContext.request before discarding the browser reply. This is an existing behavior, not evidence that the nine privacy checks cover its correction. A prospective fix needs a separate review of the selected duplicate-checkout group and its controls before final promotion. The scope conclusion below applies only to the committed 0fc proxy fix.

## Scope decision

Keep the same nine checks and 46 registered defects as the b85 observer selection. The boundary from b85dc391e1 to 0fc6c1bf8 changes eight files: one callback-scoped evaluator request helper, its browser tests, six integration sites. There are no scenario, contract, reference-source, or mutation changes at this boundary.

The new helper is registered by `network-interruption.ts:attach`. `grade.ts` creates that interruption only for an actor selected by `expectNotReceived` or `setOffline`; fresh clients get it only when selected for privacy. Unregistered calls return the exact original request object. Thus a shared HTTP caller does not by itself put every caller in the new request-context path.

The selected privacy groups remain: 110a; customer profile 620a/620b/620c; managed support 613b; support history 612a/612b/612c/612d. Whole feature groups preserve actor selection and criterion order. The exact stable keys and 12/12/12/10 defect closure remain in `passive-http-observer-impact.json` and the current frozen launch inputs. The earlier b85 failed aggregates must not supply qualification evidence.

Offline callers were inspected separately. `progression-account-state-reconnect.json` uses browser cart actions and observations. The stock reconnect paths in `01-systems.json` and `01-external-reconnect-sync.json` use database stock writes and browser observations. They do not call evaluator HTTP, credential patches, or response-loss capture. The new registration reads the blank page user agent; their actual cut/restore forwarding is unchanged. Existing interruption tests support reuse of these paths. The helper refuses evaluator work during an active cut if a future caller does attempt it.

`01-duplicate-checkout.json` uses response-loss interception but has no privacy/offline observer, so its request object remains unregistered. The route rewrite uses the same public Playwright API request path as route.fetch. Request method, body, headers, retry/redirect limits and response-loss boundary remain in the caller. Fulfilled responses apply their cookies through the browser; dropped registered responses do not copy evaluator cookies into browser state. Delayed Max-Age and dropped-response browser cases establish this ownership boundary.

The five preserved native diagnostic groups use purchase-session, administrator-write and initial-staff-seed scenarios. They have no privacy/offline observer. Their schema reads stay on the original request object; native WebSocket parsing and replay are unchanged. Their existing 27e/image8ab evidence is retained with its actual identity. No fresh native-group claim is made.

## Evidence and limits

The helper red evidence is `browser-request-red.log` and `browser-request-route-red.log`. The current focused result is `browser-request-green.log`/`.json`; broader observer/interruption checks are in `browser-request-observer.log`. Parent reports 107 focused and 63 observer checks passed, with typecheck and lint passed. These tests support the caller boundary. They do not replace the running four-stack reference/defect controls or normal calibration validation.

The helper preserves current cookies, explicit replay headers and browser user agent. It uses no proxy listener or private Playwright API. Partitioned cookies are conservatively unmeasured; they are not flattened. Conflicting same-key changes are unmeasured, not overwritten. Cookie compare-and-set is not atomic in the public API; the guard detects changes visible at the final cookie read. This is a stated measurement limit, not a new app requirement.

## Merge requirements

Use new 0fc6c1b native reports and null9 from image `sha256:c39b77a16cc38daaad89f2acfd2207cf9368588188aed9a0f235a1730a08c7d5`. Require clean complete positives, every registered defect caught at the intended assertion, immutable snapshots, and released resources. Keep 87 historical checks, 16 slices from wholly successful earlier 25-check reports, and nine fresh checks. Preserve every original image and scope. Null executable identity must be recomputed and explicitly mapped for retained slices; do not claim its hash is unchanged.

The adapted helpers preserve the prior scripts. They record the two newly added files with an absent historical hash and an exact current hash. Only these named additions may have no historical file. Both helpers remain fail-closed on pending evidence; the merger defaults to read-only. Existing contract-text and reference-source reviews remain necessary, including the older MongoDB token-helper ancestry review. The current change adds no new reference-source equivalence. After a reviewed apply, normal check-calibration and release-status validation are still required.
