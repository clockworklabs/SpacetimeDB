# Qualification executable equivalence

The previous full L3 gate and the review-owner targeted gates used the frozen
controller image `sha256:6e820aeaaf9cab078d2fd64f422190922e9f29f2fc55c2833ce00f5d27243c47`.
The targeted null artifact was produced by the same image with the changed
qualification modules and explicit null-selection command mounted read-only.
All source scopes remain recorded in the original artifacts.

The executable differences covered by this decision are confined to:
- recipe-release.ts: expose the existing meaning and execution hash inputs;
  the public recipe release and its hashes are unchanged;
- calibration-compiler.ts and qualification-slices.ts: validate combined evidence
  and save its definition inputs; no application action or grading assertion changes;
- reference-live.ts: save definition inputs alongside the original artifact;
- null-control.ts: select explicit checks within the calibrated selection and
  save definition inputs. Selected checks use the existing grader and null analyzer.

These imports change broad executable identities. They do not change the
underlying reference deployment, request transport, browser action, assertion,
mutation execution, reset, lease or cleanup code. The snapshot compiler verifies
hash preimages, scenario setup, shared runtime/fixture/prompt inputs, per-pack
budgets, exact control definitions, runner/reference inputs and coverage.

The preceding check-definition change (e804c1302) is NOT declared equivalent.
Only its 113 unchanged checks are reused from the previous full gate. Check
618a uses the new targeted clean/mutation observations plus the new null gate.
All prior artifacts are retained unchanged, including their original identities
and diagnostic labels. Targeted mutation gates can supply their validated clean
baseline to the reference slice; they are never relabeled as full reference runs.

The current implementation is limited to independent dependency scenarios with
the same qualification policy and check population. run-suite resets the app
before each selected scenario. Sequential inherited-stage reuse is rejected.
Pack timing still comes from passing observations under unchanged pack budgets;
this is not a new throughput measurement or a claim of perfect app correctness.

Verification: 31 focused tests passed, including the actual registered evidence
and rejection cases. The final qualification-status tests passed (9/9).
Typecheck, build, four calibration checks and four definition snapshots passed.
The status command now returns ready=true with no blockers or rerun commands.
The qualification-cli.ts change only suppresses redundant launch suggestions
when this validated coverage is already complete.
