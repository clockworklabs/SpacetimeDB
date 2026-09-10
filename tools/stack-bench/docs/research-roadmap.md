# Stack Bench research roadmap

This document defines the collection method. Campaign manifests and their evidence
record completed work and the next frozen protocol. This roadmap does not authorize
runs, qualification, or publication.

## Decision and current position

Measure how much model usage each defined stack needs to implement the same
product, and how much of the selected behavior it completes. Compare delivered
stack packages, including the intentional SpacetimeDB skills. This is not a
database-only experiment.

The no-repair question is which expected production behaviors appear without
failure feedback. The repair question is how much completion and cost follow
actionable failure reports. Neither question sets a preferred stack's outcome.

Use general-purpose names for shipped campaigns, commands, reports, and examples.
Describe the experiment or function, not a prospective customer or recipient.
Keep private delivery context in local notes. This naming rule does not change
the disclosed method, support conditions, exclusions, or evidence. Historical
run identities remain immutable in their original records.

The baseline study uses dependency mode with progressive work selection and no
repairs or execution retries. Keep the graph, target depth, model, guidance,
budgets, repetition count, and concurrency in the frozen campaign manifest.
The model-free Docker demo is setup evidence, not agent performance evidence.

Dependency depth comes from the feature graph. Progressive selection groups
available new work at each depth. Failed prerequisites block dependent features;
other branches can remain available. Previously completed behavior is checked
again as the app grows. Targeting a depth does not guarantee reaching every selected node.
Use the graph definition for its available depth range. Graph depths are not sequential L1/L2/L3
product releases; keep those experiment names and denominators separate.

Grading remains provisional. The recent source audit and selected reference
checks do not qualify the full selected dependency scope. Exploratory paid collection
can proceed before public qualification. A verified public comparison cannot.

## Collection sequence

Set sample counts from the study's purpose; a pilot count is not a statistical
power calculation. One block means one fresh attempt on each selected stack under the same
protocol. Repetitions start with clean apps and independent agent sessions.

| Stage | Collection | Purpose and exit condition |
| --- | --- | --- |
| 1: measurement pilot | A balanced block on the selected stacks | Confirm that the harness produces valid measurements. Diagnose failures before scaling. |
| 2: initial comparison dataset | A fixed number of new balanced blocks under one frozen protocol | Show every result and its variation. Choose the count and concurrency before launch. |
| 3: focused confirmation | Separate frozen batch; size set after pilot variance and decision threshold | Test a stated claim with uncertainty. Freeze count, budget, exclusions, and analysis before launch. |
| 4: wider scope | Deeper dependency work, another product, or another model | Test whether findings extend beyond the initial condition. Keep each condition separate. |

Keep pilots and revised cohorts separate. Do not replace original inputs or
increase an attempt's allowance after seeing its result. The time and money
limits cover the complete attempt, not each depth. They are ceilings, not price
or duration forecasts. Early blocking and regression checks change actual usage.
Do not extrapolate from sequential L1 by multiplying by three.

Before each comparison batch, review the available evidence and freeze the exact graph, selected
checks, guidance, images, model, limits, and analysis. Keep repairs and retries
at zero for this comparison. If any protocol input changes, start a separate
dataset and disclose the change. Each additional batch needs authorization.

If selected work is blocked, report it. A repair-enabled dependency study is a
separate optional experiment, with a new frozen allowance and all repair cost
included. It is not the baseline protocol. Its primary question is how much completion
and total model cost each stack achieves under the same repair allowance.
The no-repair baseline instead measures delivery without failed-check feedback.
Neither experiment assumes which stack will win. A claim that repairs improve
results needs separate comparable repair and no-repair cohorts. Comparing a
repaired app with its own earlier checkpoint alone does not isolate feedback
from additional work and model usage.

### Optional future sequential experiment

Sequential L1 covers the storefront, L2 adds operations, and L3 adds deferred
work. That runner requires a whole level to pass before advancing. It answers
a different question from the dependency study. No sequential study or
repair allowance is scheduled by this roadmap. A reference-seeded L3 upgrade
would be a further distinct experiment: verify its launch path, record baseline
provenance and excluded construction cost, and do not call it a fresh build.

Do not weaken gates or give successful source to selected stacks merely to
produce higher-level scores. Reference-seeded and fresh-build outcomes must
have separate tables and claims.

## Parallel execution and collection cost

Three stacks by three repetitions gives nine attempts. The current operating
default is nine parallel attempts unless the operator specifies otherwise.
Declare concurrency before launch. Resource or credential admission can delay
dispatch; report that delay rather than silently reducing the experiment to
three parallel attempts. Resource leases are allocated automatically and do not
require a manually sized runner pool.

Use the existing admission and resource controls. Verify the selected concurrency
against measured capacity; do not repeat successful capacity checks for unchanged conditions.
A balanced wave contains equal numbers of all stacks. Do not assign each stack
a different host or load level.
Use the existing balanced-rotation order and retain its seed. A seed controls
ordering; it does not make model generation deterministic.

For each capacity step, retain Docker allocation, host/architecture, actual
concurrency over time, peak memory, CPU pressure, OOM events, disk availability,
provider throttling, phase wall time, and cleanup outcome. Separate configured
limits from measured usage. If a measurement is unavailable, say so. Do not
invent a RAM minimum. Stop increasing load on OOM, admission/ownership failure,
incomplete evidence, or saturation that prevents a fair comparison. Diagnose
the failed capacity step; do not rerun all earlier successful gates.

Do not treat load-test timing and steady-load comparison timing as interchangeable.
Report infrastructure contention separately from application defects. If capacity
changes between study batches, retain batch identity and report results by batch.

Before collection, choose a common per-attempt cap from observed usage plus a stated
headroom allowance. The maximum campaign authorization is attempts multiplied
by that cap, plus any explicitly authorized retries. Report the cap and actual
spend. Reaching the cap is an outcome, not permission to increase it mid-study.

## Freeze the method before the main batch

Use the compiled campaign manifest and its retained artifacts for machine-recorded fields.
Keep only the research question and analysis decisions not represented there in a small method
note beside it. Link the manifest; do not copy its fields into a second configuration. Record:

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
expose scoring material to agents. STDB skills stay enabled and disclosed. Give
each stack the same opportunity to use its declared tools and supported setup.
If desired later, measure guidance as a separate ablation; it is not a condition
for accepting the main stack-package comparison.

Never add runs until a preferred stack wins. Do not choose “representative” apps
after seeing scores. Preserve failures and costs from every execution. A harness
or provider failure is excluded from app comparison under the frozen rule, but
remains in the operational and spending tables. Report attempted, eligible,
excluded, stopped, and reached counts for every stack. Missing cost stays unknown.

## Measures and analysis

Keep cost and completion as two primary outcomes. Do not hide their tradeoff in
one composite score.

Evaluate eligibility separately for each measure. A valid completed outcome can retain its
completion metric when exact cost is unavailable. Exact cost, an upper bound, and unknown cost
are distinct; never replace an unknown amount with zero. This does not waive run validation:
if missing receipts also prevent verification of the declared spending cap, the attempt has an
unresolved protocol issue and is not automatically eligible for comparison.

Use the report's selected, passed, failed, blocked, and unmeasured counts. Its unmeasured count
does not distinguish all unattempted, deferred, and inconclusive checks. Use linked grade
evidence for those distinctions when available, and state when a breakdown cannot be recovered.
Do not infer attempted counts from selected minus blocked, or sum overlapping property groups.
Timing failures need evidence-based attribution; timing alone is not a harness failure class.

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
as independent accomplishments. Work
not reached gets no completion credit; label it blocked, not measured app failure.
Do not report completion only among apps that reached depth 3. Validate the
denominator against the frozen selection and keep this research view distinct
from any existing report metric with different semantics.

A pre-repair checkpoint at a later depth can inherit guidance from earlier
repairs. It is not an unsolicited-guarantee baseline. Preserve feedback history
when extending or seeding from an existing attempt.

A planned depth pause must be declared in the original full-target plan. It
retains the live app, database, and cumulative budgets while the controller stays
running. It is not restart recovery. Database timers and external services can
advance during the hold. Compare staged and uninterrupted attempts as separate
conditions until evidence supports a narrower equivalence claim. A source-seeded
extension does not restore the original database or become a fresh 0-to-L3 run.

Audit failures against the saved source, exact issued request, and check evidence.
Record the measurement stage (setup, assertion, blocked, or inconclusive), the
application cause, where the requirement was disclosed (current request, earlier
request, or not disclosed), and any unsupported harness assumption. These are
separate facts, not mutually exclusive blame labels. A missing interface can be
an app regression and expose a prompt limitation. Repeating its contract is a
testable treatment, not proof that the omission caused the failure.

Do not count setup failures as measured backend defects. Report provider errors
separately, retain uncertain cost bounds, and require final validated progression
evidence before using a completed process as a completed comparison result.

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
size and precision. Analyze whole attempts/blocks, preserving their dependence;
do not bootstrap individual check rows. Predeclare primary contrasts and treat
other cuts as exploratory. Use an appropriate binomial interval for full-target
success rates, especially with small samples or zero failures. Three runs per
stack support a useful initial comparison, not a general claim of superiority.

Blocking is a standard way to account for nuisance factors such as batch or
host conditions ([NIST](https://www.itl.nist.gov/div898/handbook/pri/section3/pri332.htm)).
Small-sample success-rate intervals need care; normal approximations can be
inaccurate ([NIST](https://itl.nist.gov/div898/handbook/prc/section2/prc241.htm)).

## Failure review and defensibility

Keep each failed check as an observation. Group checks under a common cause only
when logs, source, or a reproduction establish that cause. A group of failed
checks is not that many distinct bugs. Mark suspected causes as unresolved.

Review app, harness, provider, and interrupted outcomes separately. Where feasible,
use the same reviewer rubric without stack labels, then disclose the source
needed to verify the diagnosis. Do not repair generated apps manually in the
primary dataset. Preserve the original grade before correcting a harness defect.
A regrade of unchanged source is paired diagnostic evidence, not another trial.
New prompt/interface requirements require a new cohort when old source is not
compatible. Never rewrite historical results to fit a later contract.

Before a public comparative claim, obtain independent external review of the
frozen protocol, exclusions, scoring, and analysis. Record unresolved objections
and disclose reviewer affiliations. Parallel agent review is an internal check;
it is not independent external review or independent replication. Do not claim
replication until another team reproduces the method and reports its results.

Before verified publication, qualify every selected check: a valid reference,
appropriate valid alternatives, declared defect controls, and the existing
null/release gates. Check qualification coverage for the exact reported selection.
Evidence for one recipe or depth does not qualify another.
Static mutation coverage is an inventory, not proof of defect detection. A live
control must fail at the intended assertion; setup failure, timeout, or missing
evidence cannot substitute. Repeated clean reference passes check repeatability,
but do not prove that timing failures are impossible. See the
[reference qualification guide](../reference-apps/README.md) for exact scope and
repetition requirements.
Keep provisional evidence available with its label while this work
proceeds. Explicitly justify expected production requirements and check weights;
do not claim that this finite test proves an app is production-ready.

## Research pack and durable archive

Aim for a 4–6 page decision report, a one-page run guide, and linked evidence.
Page count is a reading target, not a limit on the data retained.

The decision report should contain:

1. Scope and method, including intentional skills and qualification status.
2. Cost versus completion for every attempt, colored by stack.
3. Feature and selected-depth reach, completion, blocked work, and build checkpoints.
4. Cost/token breakdown and observed variation, with sample counts.
5. A short failure table with confirmed causes and linked evidence.
6. Limits, exclusions, and the next experiment that would change the decision.

Ship the existing HTML report and validated JSON, frozen plan, method file,
attempt-level CSV, and manifest-listed artifacts with their relative paths intact.
The CSV should identify campaign, block, attempt, execution, stack, model,
condition, level, source/definition hash, outcome, passed/selected counts,
weighted points, repair count, cost status/value, token buckets, duration, and
evidence path. Use one row per attempt-level checkpoint where needed; do not
count those rows as independent attempts in analysis.

The `export-manifest.json` is an index, not a portable archive. The
`campaign export <campaign-directory> --out <new-directory>` command copies
its listed public artifacts and adds attempt/execution CSVs using existing report
owners. It omits source trees, raw transcripts, media, and external evidence;
links to omitted files cannot work offline. Check included links and hashes. Add a
separately reviewed source/prompt archive if claiming full reconstruction. The
complete campaign copy described in the [appliance guide](../appliance/README.md)
is an internal backup; it is not automatically safe for public distribution.

Keep full original campaign evidence privately: all receipts, prompts, source
checkpoints, grades, logs, media, admissions, resource records, exclusions, and
qualification evidence. For external sharing, review free text and generated
source for credentials and private data. Exclude private authority, provider
credentials, and environment secret files. Record omissions. Retain immutable
originals and hash the shared pack. Full transcripts and every screenshot belong
in the evidence archive, not in the report body.

The run guide must distinguish the free reference demo from a paid model campaign.
Include the tested Docker command, platform requirements, credentials needed for
paid work, budget controls, output location, and how to inspect/copy results.
Rebuilding the same plan must be possible; identical stochastic outputs are not
promised.

## Agent and model support

The registry includes Claude Code, Codex, and OpenRouter adapters. Registration
is not qualification of every model, credential route, or execution mode.
Before collection with a new adapter condition, verify its declared launch,
repair, continuation, usage, budget, and failure paths with matching evidence.
Any live pilot needs separate authorization.

Keep provider, model, agent runtime/version, tools, reasoning settings, and
context policy distinct in the frozen condition. Preserve and disclose the
intentional SpacetimeDB skills. Compare stacks within each condition; do not
pool different agent conditions into an unexplained stack average. Evidence
from one model does not establish results for another.

## Methods references

[NIST AI RMF Measure](https://airc.nist.gov/airmf-resources/playbook/measure/)
supports documented measurement validity and independent review.
[NIST: Expanding the AI Evaluation Toolbox with Statistical Models](https://www.nist.gov/publications/expanding-ai-evaluation-toolbox-statistical-models)
distinguishes fixed-benchmark accuracy from generalized performance; correlated
checks from one app are not independent experimental replicates.

## Parallel work assignments

Use existing owners. No research service, scheduler, or new frontend is needed.

| Owner | Bounded task | Completion evidence |
| --- | --- | --- |
| Orchestrator | Freeze questions, campaign manifest, budget proposal, and analysis rules | Reviewed compiled plan and method file before any new paid launch |
| Agent A: definitions | Audit/qualify the exact selected dependency checks and expected-spec justifications | Matching source audit and, when separately authorized, qualification evidence; explicit remaining gaps |
| Agent B: report | Derive CSV and copy manifest-listed artifacts through existing report/export code | A small saved-fixture check for row counts, costs, hashes, and portable links |
| Agent C: operations | Retain capacity and wall-time evidence through existing runtime owners; prepare next capacity step | Measured limits and failure reasons; no unsupported RAM or throughput claims |
| Orchestrator/reviewer | Review completed attempts, resolve classifications, assemble the report | All assigned attempts accounted for; report totals match receipts and grades |

Agents can do source/pack work while paid attempts run. Shared runtime or grader
changes apply to a new frozen cohort. Do not rebuild or replace the active runner
under a collecting campaign. Request separate authorization for each concrete
paid or long-running gate; this roadmap alone does not start them.
