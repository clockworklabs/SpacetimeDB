# Study method

This guide describes how to collect and report a defensible Stack Bench
comparison. The campaign manifest and its evidence record what a study actually
ran; this guide covers the decisions around them.

## What a study measures

Measure how much model usage each defined stack needs to implement the same
product, and how much of the selected behavior it completes. Compare delivered
stack packages, including the selected SpacetimeDB skills. This is not a
database-only experiment.

The no-repair question is which expected production behaviors appear without
failure feedback. The repair question is how much completion and cost follow
actionable failure reports. Neither question sets a preferred stack's outcome.

The baseline design uses dependency mode with progressive work selection and no
repairs or execution retries. Keep the graph, target depth, model, guidance,
budgets, repetition count, and concurrency in the frozen campaign manifest.
The model-free Docker demo is setup evidence, not agent performance evidence.

Dependency depth comes from the feature graph. Progressive selection groups
available new work at each depth. Failed prerequisites block dependent features;
other branches can remain available. Previously completed behavior is checked
again as the app grows. Targeting a depth does not guarantee reaching every
selected node. Graph depths are not sequential L1/L2/L3 product releases; keep
those experiment names and denominators separate.

## Study stages

Set sample counts from the study's purpose; a pilot count is not a statistical
power calculation. One block means one fresh attempt on each selected stack under
the same protocol. Repetitions start with clean apps and independent agent sessions.

| Stage | Collection | Purpose and exit condition |
| --- | --- | --- |
| Measurement pilot | A balanced block on the selected stacks | Confirm that the harness produces valid measurements. Diagnose failures before scaling. |
| Initial comparison | A fixed number of new balanced blocks under one frozen protocol | Show every result and its variation. Choose the count and concurrency before launch. |
| Focused confirmation | Separate frozen batch, sized from pilot variance and a decision threshold | Test a stated claim with uncertainty. Freeze count, budget, exclusions, and analysis before launch. |
| Wider scope | Deeper dependency work, another product, or another model | Test whether findings extend beyond the initial condition. Keep each condition separate. |

Keep pilots and revised cohorts separate. Do not replace original inputs or
increase an attempt's allowance after seeing its result. The time and money
limits cover the complete attempt, not each depth. They are ceilings, not price
or duration forecasts. Early blocking and regression checks change actual usage.
Do not extrapolate from sequential L1 by multiplying by three.

Before each comparison batch, freeze the exact graph, selected checks, guidance,
images, model, limits, and analysis. If any protocol input changes, start a
separate dataset and disclose the change.

### Other designs

A repair-enabled dependency study is a separate experiment, with its own frozen
allowance and all repair cost included. A claim that repairs improve results needs separate comparable repair and no-repair
cohorts. Comparing a repaired app with its own earlier checkpoint alone does not
isolate feedback from additional work and model usage.

Sequential mode requires a whole level to pass before advancing and answers a
different question from the dependency study. A reference-seeded upgrade is a
further distinct experiment: record baseline provenance and excluded
construction cost, and do not call it a fresh build. Do not weaken gates or give
successful source to selected stacks to produce higher-level scores.
Reference-seeded and fresh-build outcomes need separate tables and claims.

## Parallel execution and collection cost

Each campaign explicitly sets its parallelism. Shared host capacity determines
when the campaign can start; it does not change the requested parallelism.
Resource or credential admission can delay dispatch; report that delay rather
than silently reducing the experiment.

A balanced wave contains equal numbers of all stacks. Do not assign each stack a
different host or load level. Use the existing balanced-rotation order and retain
its seed. A seed controls ordering; it does not make model generation deterministic.

For each capacity step, retain Docker allocation, host/architecture, actual
concurrency over time, peak memory, CPU pressure, OOM events, disk availability,
provider throttling, phase wall time, and cleanup outcome. Separate configured
limits from measured usage. If a measurement is unavailable, say so. Stop
increasing load on OOM, admission or ownership failure, incomplete evidence, or
saturation that prevents a fair comparison.

Load-test timing and steady-load comparison timing are not interchangeable.
Report infrastructure contention separately from application defects. If
capacity changes between study batches, retain batch identity and report results
by batch.

Before collection, choose a common per-attempt cap from observed usage plus a
stated headroom allowance. The maximum campaign authorization is attempts
multiplied by that cap, plus any explicitly authorized retries. Report the cap
and actual spend. An attempt that reaches the cap stops, is recorded as "Cost cap
reached", and is excluded from comparison; it is not permission to increase the
cap mid-study. Frequent cap stops mean the headroom was too small.

## Freeze the method before the main batch

Use the compiled campaign manifest and its retained artifacts for
machine-recorded fields. Keep only the research question and analysis decisions
not represented there in a small method note beside it. Link the manifest; do not
copy its fields into a second configuration. Record:

- Research question, primary outcomes, sample count, stopping rule, and budget.
- Repository commit, image digests, platform, compiled plan and definition hashes.
- Model and adapter versions, provider route, context policy, and pricing snapshot.
- Exact agent-visible product request, contracts, stack material, and skills.
  Retain the text as well as its hash. A hash cannot reconstruct missing content.
- Features and checks, requested/expected/observed specification roles, weights,
  progression rules, repair disclosure, repair allowance, and retry policy.
- Host, resource limits, concurrency, ordering seed, dates, cache treatment,
  and any other work sharing the host.
- Failure classes, exclusion rule, replacement rule, and report calculations.

Keep the normal product request and expected production checks separate. Do not
expose scoring material to agents. Disclose the selected guidance profile and
skills. Give each stack the same opportunity to use its declared tools and
supported setup. Measure guidance as a separate ablation.

Never add runs until a preferred stack wins. Do not choose “representative” apps
after seeing scores. Preserve failures and costs from every execution. A harness
or provider failure is excluded from app comparison under the frozen rule, but
remains in the operational and spending tables. Report attempted, eligible,
excluded, stopped, and reached counts for every stack. Missing cost stays unknown.

## Measures and analysis

Keep cost and completion as two primary outcomes. Do not hide their tradeoff in
one composite score.

Evaluate eligibility separately for each measure. A valid completed outcome can
retain its completion metric when exact cost is unavailable. Exact cost, an upper
bound, and unknown cost are distinct; never replace an unknown amount with zero.
This does not waive run validation: if missing receipts also prevent
verification of the declared spending cap, the attempt has an unresolved protocol
issue and is not automatically eligible for comparison.

Use the report's selected, passed, failed, blocked, and unmeasured counts. Its
unmeasured count does not distinguish all unattempted, deferred, and inconclusive
checks. Use linked grade evidence for those distinctions when available, and
state when a breakdown cannot be recovered. Do not infer attempted counts from
selected minus blocked, or sum overlapping property groups. Timing failures need
evidence-based attribution; timing alone is not a harness failure class.

| Measure | Required interpretation |
| --- | --- |
| Check completion | Passed / selected checks, with both counts. Weighted points remain separate. |
| Feature completion | Fully passed dependency nodes / selected nodes. A node with an unfinished guarantee is not fully complete. |
| Build checkpoints | Show each measured progressive build. For repair cohorts, separate pre-repair and repaired checkpoints and include all repair cost. |
| Feature and depth reach | Show nodes started, passed, failed, and blocked at each graph depth out of all assigned attempts. A reached depth need not mean all its nodes passed. |
| Full target delivery | Fraction of assigned attempts that passed the complete target; show exclusions separately. |
| API-equivalent cost | Use receipt status and frozen rates; distinguish exact, upper-bound, and unknown. It is not a subscription invoice. |
| Token usage | Separate ordinary input, output, cache reads, and cache writes; retain receipt-level cache-write durations. |
| Time | Show end-to-end wall time, planned pause time, and execution duration separately. Campaign timeout excludes verified planned depth pauses; provider waits still consume the allowance. Retain the raw timestamps. |
| Reliability | Harness/provider failures, evidence failures, OOMs, cleanup failures, and cap stops. |
| Regression | Previously passed checks lost after new work or repair, with source/checkpoint identity. |

For dependency results, show node/depth completion and a separate whole-target
view using the frozen selected checks. Do not sum repeated checks across depths
as independent accomplishments. Work not reached gets no completion credit; label
it blocked, not measured app failure. Do not report completion only among apps
that reached the target depth. Validate the denominator against the frozen
selection.

Separate UI, feature, and production-quality checks. Report the latest depth's
results alongside cumulative results; a high cumulative percentage must not
obscure an authorization or concurrency failure.

A pre-repair checkpoint at a later depth can inherit guidance from earlier
repairs. It is not an unsolicited-guarantee baseline. Preserve feedback history
when extending or seeding from an existing attempt. A
[planned depth pause](../appliance/README.md#planned-depth-pause) and an
uninterrupted attempt are separate conditions until evidence supports a narrower
equivalence claim. A source-seeded extension does not restore the original
database or become a fresh run from zero.

Audit failures against the saved source, exact issued request, and check
evidence. Record the measurement stage (setup, assertion, blocked, or
inconclusive), the application cause, where the requirement was disclosed
(current request, earlier request, or not disclosed), and any unsupported harness
assumption. These are separate facts, not mutually exclusive blame labels. A
missing interface can be an app regression and expose a prompt limitation.
Repeating its contract is a testable treatment, not proof that the omission
caused the failure.

Total tokens count repeated processing, including cached input. They do not
measure unique prompt size or generated code. Show cost/completion scatter plots
and measured checkpoint curves. Do not interpolate unmeasured success between
checkpoints. Cost per passed check can be an appendix diagnostic, but is a poor
headline: checks differ in difficulty and a zero-score app has no finite ratio.

For each comparison dataset, show every attempt, median, IQR, and mean cost.
State the small sample size beside each comparison. Use matched batch differences
to describe stack contrasts, while recognizing that model outputs are independent
draws, not identical seeded tasks. A check is not an independent sample; levels,
repairs, and regrades from one app are not new app builds.

For confirmation, first choose the smallest decision-relevant cost difference
and completion difference. Use observed between-build variation to plan sample
size and precision. Analyze whole attempts or blocks, preserving their
dependence; do not bootstrap individual check rows. Predeclare primary contrasts
and treat other cuts as exploratory. Use an appropriate binomial interval for
full-target success rates, especially with small samples or zero failures. Three
runs per stack support a useful initial comparison, not a general claim of
superiority.

Blocking is a standard way to account for nuisance factors such as batch or
host conditions ([NIST](https://www.itl.nist.gov/div898/handbook/pri/section3/pri332.htm)).
Small-sample success-rate intervals need care; normal approximations can be
inaccurate ([NIST](https://itl.nist.gov/div898/handbook/prc/section2/prc241.htm)).
Correlated checks from one app are not independent experimental replicates
([NIST](https://www.nist.gov/publications/expanding-ai-evaluation-toolbox-statistical-models)),
and measurement validity needs documentation and independent review
([NIST AI RMF](https://airc.nist.gov/airmf-resources/playbook/measure/)).

## Failure review and defensibility

Review failures with the
[grading coverage procedure](grading-coverage.md#review-a-failed-check). Where
feasible, use the same reviewer rubric without stack labels, then disclose the
source needed to verify the diagnosis. Do not repair generated apps manually in
the primary dataset. New prompt or interface requirements require a new cohort
when old source is not compatible.

Before a public comparative claim, obtain independent external review of the
frozen protocol, exclusions, scoring, and analysis. Record unresolved objections
and disclose reviewer affiliations. Parallel agent review is an internal check;
it is not independent external review or replication. Do not claim replication
until another team reproduces the method and reports its results.

Verified publication requires [qualification](../reference-apps/README.md) of
the exact reported selection. Exploratory collection can proceed before that,
labelled provisional.

## Research pack and archive

Aim for a 4–6 page decision report, a one-page run guide, and linked evidence.
Page count is a reading target, not a limit on the data retained.

The decision report should contain:

1. Scope and method, including guidance, skills, and qualification status.
2. Cost versus completion for every attempt, colored by stack.
3. Feature and selected-depth reach, completion, blocked work, and build checkpoints.
4. Cost/token breakdown and observed variation, with sample counts.
5. A short failure table with confirmed causes and linked evidence.
6. Limits, exclusions, and the next experiment that would change the decision.

Ship the existing HTML report and validated JSON, frozen plan, method file,
attempt-level CSV, and manifest-listed artifacts with their relative paths
intact. Use one row per attempt-level checkpoint where needed; do not count those
rows as independent attempts in analysis.

The `export-manifest.json` is an index, not a portable archive. The
`campaign export <campaign-directory> --out <new-directory>` command copies its
listed public artifacts and adds attempt and execution CSVs. It omits source
trees, raw transcripts, media, and external evidence; links to omitted files
cannot work offline. Check included links and hashes. Add a separately reviewed
source and prompt archive if claiming full reconstruction. The complete campaign
copy described in the [appliance guide](../appliance/README.md#results-and-cleanup)
is an internal backup; it is not automatically safe for public distribution.

Keep full original campaign evidence privately: all receipts, prompts, source
checkpoints, grades, logs, media, admissions, resource records, exclusions, and
qualification evidence. For external sharing, review free text and generated
source for credentials and private data. Exclude private authority, provider
credentials, and environment secret files. Record omissions. Retain immutable
originals and hash the shared pack.

The run guide must distinguish the free reference demo from a paid model
campaign. Include the tested Docker command, platform requirements, credentials
needed for paid work, budget controls, output location, and how to inspect and
copy results. Rebuilding the same plan must be possible; identical stochastic
outputs are not promised.

## Agent and model conditions

The registry includes Claude Code, Codex, and OpenRouter adapters. Registration
is not qualification of every model, credential route, or execution mode. Before
collection with a new adapter condition, verify its declared launch, repair,
continuation, usage, budget, and failure paths with matching evidence.

Keep provider, model, agent runtime and version, tools, reasoning settings, and
context policy distinct in the frozen condition. Compare stacks within each
condition; do not pool different agent conditions into an unexplained stack
average. Evidence from one model does not establish results for another.
