# Probe removal review and qualification gate

Reviewed 2026-09-28 UTC against base `a6edb1cfe22020859a58872429a02aad741351f5`. This is a working-diff review, not the required final staged commit review. Exact final reviewed inputs are in `reviewed-inputs.json`; `reviewed-working-diff.patch` has SHA-256 `015bbbdd01ec7a2afc5d2d0156ce40b3a563d6b2ab58a3fb71e3ba91e8af8df7`.

## Result

No correctness defect found in the six-file probe-removal diff. One unused chain remained after the command was deleted. The parent authorized its removal during this review. The final diff removes that chain too. Selected action execution, reset, score, outcome and progression behavior are not changed by this diff. Historical action evidence remains readable. This review does not qualify the candidate or establish broad product readiness.

## Ponytail review

Initial finding, now resolved:

`src/stacks/stack-adapter-contract.ts:L31,L44: delete: NamedActionProbe and browserAccounts/probe option members have no remaining consumer. Nothing replaces them.`

`src/stacks/backends/convex-adapter.ts:L11,L33; supabase-adapter.ts:L38: delete: removed probe import and adapter bindings. Keep namedAction.request.`

`src/stacks/backends/convex-operations.ts:L269; tests/convex-operations.test.ts:L5,L32,L107: delete: native presence probe, its fixture branch and presence-only test are unused after command removal. Nothing replaces them.`

The existing recovery integration test gained one write trap and one assertion. It exercises the actual runner and catches an unsolicited request. This is bounded behavioral coverage, not an unnecessary new framework or a source-shape test. No new helper, selector or dependency was added.

Final ponytail result: Lean already. Ship. This statement concerns complexity only; the qualification gate below remains open.

## Correctness and historical compatibility

- `commands/run-suite.ts` no longer invokes the deleted command or writes `bundle.actions`. No other executable, package-script, documented or CI caller was found. Its remaining `ARTIFACT_FILE.actions` cleanup entry removes stale outputs from reused grading directories; it is not a remaining probe caller.
- The now-removed adapter metadata was only read by `check-actions`. `probeConvexNamedAction` had only that adapter binding and its presence-only test. A final source/commands/tests search found no `browserAccounts`, `NamedActionProbe`, `probeConvexNamedAction`, `named.probe`, `namedAction.probe` or `check-actions` references.
- The Convex and Supabase `namedAction.request` operations remain. Track action definitions, selected-scenario executors, native observers and refusal classification remain. Removing diagnostic metadata does not remove a required graded action.
- `commands/test-loop.ts` no longer requires a new action artifact. It still checks lint/grade parent identities and public-secret exclusion. It now checks that a new bundle/artifact has no unsolicited action probe. The focused integration separately tests the actual application write effect.
- `src/evidence/artifact-schema.ts:37,96,115-117,855` still accepts historical `action_check` artifacts and the old grade-bundle `actions` field. `artifact-layout.ts:7`, dashboard artifact listing/labeling (`dashboard-model.ts:26,89`) and the export allowlist (`campaign-report.ts:1273`) remain unchanged. Historical records and qualification manifests were not edited.
- The only qualification-graph edit removes the deleted child command from `qualification-scope.ts`. This is the actual former call edge. Other grader/linter/reset edges remain.
- A first whitespace check reported a trailing blank line left by test deletion. That line was removed; the final `git diff --check` passed.

No current-study score contamination is established by the old probe. It produced misleading diagnostics and could issue writes, but the paid campaign uses reset before each scored scenario. See the separate study note `../sol6-four-stack-20260925/sol6-four-stack-defensible-a6edb1cfe2/startup-action-probe-audit.md` for the original caller, consumer and reset trace.

## Validation inspected and performed

The saved `persistent/receipt.json` and raw bundle show a real runner/browser/grader execution with zero unselected writes, selected positive check 1/1, selected defect 0/1, a persistent navigation failure still inconclusive after one normal retry, score 1/3 and lint pass. The initial and retry reports remain present. No `actions.json` or bundle `actions` field was produced. `run-summary.md` reports a red-to-green result; this review inspected the saved passing receipt, not a separate saved red receipt.

That generic write trap is sufficient for the removed shared request path. A separate custom account-route fixture would repeat the same absence-of-probe property after the entire command is removed. It is not needed for this deletion. The positive/negative result assertions prevent removal from turning required check failures into passes.

After the dead-chain deletion, this reviewer ran:

- `npm run typecheck`: passed; `dead-chain-typecheck.log`.
- `node node_modules/typescript/bin/tsc -p tsconfig.build.json`: passed; `dead-chain-compile.log` (empty successful output). This compiles the modified source; it is not a new controller-image build.
- `node --test dist/tests/convex-operations.test.js`: 4/4 passed; `dead-chain-convex-checks.log`. These cover native reads/writes, order snapshots, invalid/stalled data rejection and native response classification.
- `git diff --check`: passed after the whitespace correction.

The parent reported prior build/lint and 12 qualification-scope tests passing for the original six-file diff. This review does not present those as independently repeated after the follow-up deletion. No new behavioral test was added for the dead code. No model calls, Docker launches, full qualification, commits or evidence-hash edits were made.

## Minimum qualification-reuse gate

The existing real `run-suite` positive/negative integration can support an evidence-backed reuse review for this removal. No new backend reference/mutation run is justified solely by a changed coarse executable hash. The behavior removed was outside scoring; all scored observers, prompts, contracts, scenarios, points, progression rules, reference sources and registered defects are unchanged by this diff. Existing qualified reference/defect/null receipts still supply that coverage if their exact inputs validate.

Required before calling a future candidate qualified:

1. Freeze the final source after this small deletion is complete. Review the exact staged diff again before commit. Bind the final source identity and these focused receipts; the pre-cleanup integration engine hash alone is not an identity for a later candidate.
2. Compute the old/new executable scope inputs for each affected reference/mutation stack and null control. Inspect each added, removed or changed input. Expected changes are the removed startup child, runner deletion, and unused adapter/type/native-metadata deletion. Do not assume the null hash is unchanged or that every imported source file is unchanged. Stop and widen only if an additional behavior change is found.
3. Verify unchanged check selection, recipe execution identity, reference source identities and mutation inputs against the existing receipts. `calibration-compiler.ts:640-669` requires exact old/new scope mapping plus matching check, reference/stack and mutation inputs. `:1349-1374` verifies the target scope and cited evidence. Source/calibration binding also remains required. The new review must include the final diff and focused evidence; no blind checksum replacement.
4. Retain the existing required positive, deliberate-defect and applicable null receipts through that validated mapping. The current write-trap positive/negative integration plus the unchanged scored code supports the non-scoring equivalence rationale. No new reference/defect execution is currently identified as necessary. If the actual scope comparison or existing validator rejects reuse, record that exact gap and replace only its missing scope; do not bypass it or infer that all checks need rerunning.
5. Run the existing calibration validator against that frozen evidence-backed mapping. Build/check the future immutable image only after the gate passes. Do not modify the current paid worker, its grading or historical results.

This artifact recommends a bounded reuse path. It does not contain a reuse decision, new calibration hashes or a claim that qualification has passed.
