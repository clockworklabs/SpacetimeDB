# Convex anonymous refusal: failure cases before implementation

2026-09-27. Scope: saved job993215 Convex r1 L2 101a, no paid calls. First change only adds safe classifier fields to the existing callAction receipt; no grading rule changes.

- A real native argument-validation refusal must be recorded as validation, without raw response bodies, arguments or credentials in the diagnostic observation.
- A ConvexError refusal must remain application-rejected. HTTP status alone must not decide native acceptance.
- A successful valid purchase must remain accepted.
- A missing/incomplete response must never become deliberate refusal.
- If the anonymous request is rejected because the required session argument is absent, the current refusal rule must be shown to fail before any rule change. Stored state must then be checked in any proposed corrected scenario, with positive purchase controls around the refusal.
- A malformed business request or wrong route is not evidence of access control. Do not widen refusal acceptance without a proven matched operation and unchanged business arguments.
- Historical artifacts and generated source remain immutable; output goes to a new diagnostics directory. Record image identity, source hash, zero model calls, and cleanup.
## First replay result

The normal saved-source regrade confirms `responseContract:convex-mutation`, `refusalKind:validation`, `applicationRejected:false`, `accepted:false`, HTTP 200 for the anonymous caller. The buyer positive control passed. The current rule then fails 101a at `expectActionOutcome(refused)`, before its unchanged-state check. This establishes the disputed classifier path, not unauthorized purchase.

Evidence: `stack-bench-state/results/diagnostics/convex-refusal-20260927/before/regrade.json` and `grading/grading-selected-source-022.json`.

Controller image: `sha256:9314ed27658f3d034b9c07efd08bbebfb3f84972477bb16b7df0ae2b51e2b8bf`. Saved source: `3712b85864ecd64bb9324b09488392f9fff378f32fbafed29369dc8a6ff8b777` (15 files). Zero model calls, zero additional model cost, cleanup succeeded. The three leased containers were removed by the regrade; the stopped diagnostic controller was then removed. The launch script is `launch-convex-refusal-replay.ps1` in this folder.

Only tracked change so far: safe enum/boolean response classification fields in the existing callAction receipt. No scoring change.
## Bounded change and guards

The six 101a refusal assertions now use the existing `application-refused` outcome. There is no global classifier or refusal-rule change. Existing behavior still rejects an accepted unauthorized call, incomplete transport, unclassified handler exceptions, and unproved missing routes.

The Convex interface now states that handlers use ConvexError and native argument validation can reject before handler execution. This is a deliberate contract/scoring clarification for future results, not a corrected historical score.

Guard audit:

| Caller state | Matched valid call | State evidence |
| --- | --- | --- |
| Guest | Same declared Bluetooth Speaker argument read from the positively tested buyer | No new order; stock remains at post-control count |
| Wrong password | Same buyer business arguments after refused login and reload | Same no-purchase and stock checks |
| Query-like login | Same buyer business arguments | No-purchase and stock checks run before final refusal assertion |
| Duplicate registration | Same buyer business arguments after refused registration and reload | No-purchase and stock checks |
| Tampered session | Existing request fingerprint requires the same successful request with the current credential first | No-purchase and stock checks before refusal; valid purchase after |
| Signed out | Same buyer, same item argument; session removed and current-user absent | No-purchase and stock checks before refusal; sign-in and valid purchase after |

No business argument, route, expected stock delta, or accepted-purchase control changed. A handler that rejects all requests cannot pass the positive controls. A missing native function or unhandled exception is not the recognized validation category. Deliberately vulnerable and reject-all controls remain mandatory in affected qualification.

The existing test `purchase and restock privacy refusals require their successful control` was updated to find either supported refusal outcome; it still verifies that missing/unrelated positive route proof fails. No new unit test was added after implementation. Type checking passed; two matching existing behavior tests passed (`convex-refusal-existing-behavior.tap`).

After-replay image: `sha256:30cd4486206669d551bf112e59bcbd9dd1419a543fbd0c8b07f19513ceeca6b0`. Its build includes the final navigation scenario edits and visitor baselines, the clarified interface, the safe receipt fields, and matching diagnostic recipe hashes. No temporary raw-response logging was added.
## After replay: passed

The identical saved app now passes 101a, 2/2 points, through the normal grade-from path. Five missing-session cases produce native validation refusals (guest, wrong password, query-like login, duplicate registration, signed out). The tampered-session case produces a typed application refusal. Every no-purchase and stock assertion passes. Valid purchases before, between, and after the refusal probes pass, including session reauthentication. The two browser-origin probes also show no purchase or stock effect.

Evidence: `stack-bench-state/results/diagnostics/convex-refusal-20260927/after/regrade.json`; grade `grading/grading-selected-source-022.json`. Saved source SHA-256 remains `3712b85864ecd64bb9324b09488392f9fff378f32fbafed29369dc8a6ff8b777` (15 files). Regrade receipt SHA-256: `ffb874c004f04228c84e7c42db9478e0b7ae3904b33c890cbacd5d5d1ff953bf`. Grade bundle SHA-256: `464ea29fdacd38ff0fcd86f45d3ae5aa634a5fa5e3a29959a603c54846cb1fd1`.

No inconclusive or harness failures. Zero model calls and additional model cost. Cleanup succeeded; the three leased containers were removed by the regrade, then the stopped diagnostic controller was removed. Repeat with `launch-convex-refusal-after.ps1` and a fresh output path/container name. The diagnostic before and after artifacts remain separate from the canceled study.

This resolves the classification question for this saved app. Four-stack affected reference/defect qualification and null control are running separately under the parent on the same frozen image. They are not claimed passed here.
