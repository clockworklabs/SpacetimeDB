# Independent review of proposed reuse at 75f5b9014

Verdict: bounded reuse is supportable. I found no demonstrated incorrect retained result. This is a source-based review, not a claim that all112 checks ran on the new image. Promotion still requires the four replacement25-check reports, all146 registered defects, preserved null/native proof, clean recovery, and the normal calibration validator.

## MongoDB618a and9180a

The concern in codex-review-20260927.md is real at the evidence level: both checks retain reference and mutation evidence from `qualification-evidence/ecommerce-l3-navigation-20260927/mongodb.json`, source ca46de68d9e8c7bca25676f57f0dffb0a135993609eea90f5528cea3d392acc4. That report did not execute the subsequently added getSessionToken hook. The current proposal's changedFiles list starts at4f5bd95; by itself it does not explain this earlier transition.

The specific paths support equivalence:

- App.tsx:8–9 declares mongodb_shop_token and returns exactly that localStorage value from the hook.
- App.tsx:186–190 saveSession writes the same token into localStorage and React state. Signup and signin call saveSession with the response token at345–352.
- App.tsx:133–142 apiFetch uses the React token as the Bearer value. The old captured-header path therefore has the same credential as the hook. If there is no captured authenticated write, the old storage fallback also selects mongodb_shop_token.
- named-action-runtime.ts:191–261 retains non-auth request headers, replaces only auth headers for the live hook, and obtains cookies independently.
- progression-review-access.json keeps each actor in one session through all direct calls. Its fresh owner only uses the UI after signin. progression-review-script.json also keeps each direct-call actor in one session. Neither scenario changes identity or signs out before another direct call.
- All six registered Mongo controls for these checks alter review authorization, review storage, or rendering. None changes session storage, token creation, the hook, or captured credentials. They are direct-review-access-is-not-checked; review-owner-trust-username; review-owner-deny-after-write; review-owner-reject-all; review-script-unsafe-render; review-script-reject-all.
- The old JSON helper and current text-plus-JSON helper produce the same result for these complete JSON responses. The reviewed controls do not truncate response bodies. Network-body failure behavior is not declared equivalent by this argument.

Thus these two checks use a different credential lookup branch with the same bearer value, endpoint, payload, and server-side identity. The changed logout/account-switch behavior belongs to fresh101a. Do not extend this argument to a scenario that changes session identity between calls.

Relevant Git file hashes, before183b17bd2d / current75f5b9014:

- client/src/App.tsx: 4bffcc081652c7fe50e98e44fd965389b5173100588d30c18d0f869da511a506 / 2cb7ff02d81e2c934c677be88fbf95f9b0bde2bb7d9d0eb552e1199696af0af9
- client/src/request.ts: 7d806c33e39630b3647dfe2ff3ab7a3c22cdf8cbc8aef7e54f9709691a1a2ca7 / c26ff6caa6598e929d88a1eab07493b4e8a822062fe7ec9bcd98c3d9e3af8a11

Both current files are byte-identical to183b17bd2d. Add this specific older-source review to durable merge evidence. The4f-to75f file list must not be presented as the complete history of every retained source pair.

## New reference changes and shared paths

- MongoDB and Convex add receipt state at profile, notification and role save callers. Their shared act helpers are unchanged in4f-to75f. These callers are in the fresh union.
- SpacetimeDB adds awaited reducer acknowledgements only at those three save callers. Other reducer calls are unchanged. The new role-account setup is in the fresh union.
- PostgreSQL changes shared run(): without a callback, successful work still clears the message and reloads; failed work still stops before reload. Reload failure still reports an error. The new callback marks the acknowledged write before refresh, so refresh failure cannot erase a successful save receipt. Only the fresh receipt callers supply it. No retry or new write is added for retained callers.
- All four seeds add staff2 with the ordinary staff role. Existing account credentials and roles are unchanged. The selected role tests use declared account identity, not row position. Seed code does not write activity records. Fresh role/privacy controls cover the new account and changed role flow.
- The144dd-to75f boundary consists only of four mutation manifests. The helper's exact retained mutation comparisons passed after those corrections. The failed original aggregates are excluded, not repaired.

## Merge limits and gates

The preparation helper passed unchanged scenario/setup checks and exact retained mutation comparisons for87 checks across56 slices, with10 source pairs and29 executable mappings. The merger defaults to read-only and stops on missing terminal evidence. It checks original reports, snapshots, registered mutation IDs, clean catches, native exact statuses, and cleanup. It retains the old null snapshot against exact704f manifests rather than rewriting it.

The source-equivalence mechanism remains a reviewed trust boundary: matching hashes prove which files were reviewed, not that a prose claim is mathematically true. This review supports the concrete paths above. It does not justify removing normal hash, scenario, mutation, source, repetition, or cleanup gates. A fresh full qualification would reduce the evidence chain, but no current code defect found here requires it before this bounded merge.

No production file, calibration, report, checkpoint, container, or test was changed or run for this review.
