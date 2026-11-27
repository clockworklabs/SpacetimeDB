# Study method

How to run a comparison that holds up: what to decide before the first run,
what to freeze, what to measure, and what the results can and cannot claim.
The campaign plan and its evidence record what a study ran; this covers the
decisions around them.

## What a study measures

A study measures how much model usage each stack needs to build the same store,
and how much of the selected behavior passes. It compares stack packages as an
agent receives them, including the selected SDK skills. It is not a
database-only experiment, and no result can say how much of a difference came
from the database, the API, the documentation, or the model's familiarity.

The baseline design is dependency mode, progressive work selection, no repairs,
no retries. A repair-enabled study is a separate experiment with repair cost
included, and a claim that repairs help needs matching repair and no-repair
cohorts.

## Before the first run

Set the sample count from the question, and start every repetition from a
clean store and a fresh agent session. A pilot count is not a power calculation.

| Stage | Collection | Exit condition |
| --- | --- | --- |
| Pilot | One block: one attempt per stack | The harness produces valid measurements |
| Comparison | A fixed number of blocks under one frozen protocol | Every result and its variation are shown |
| Confirmation | A separate batch, sized from pilot variance and a decision threshold | A stated claim is tested with uncertainty |
| Wider scope | Deeper work, another product, or another model | Findings extend, or do not, beyond the first condition |

Freeze before the comparison batch, and start a new dataset if any of it
changes: the feature graph and target depth, the selected checks and their
treatments, the guidance profile, the images, the model and adapter versions,
the time and cost caps, the repetition count and concurrency, the exclusion
rule, and the analysis. The plan records most of this; keep the research
question and analysis decisions in a short note beside it.

Set one per-attempt cost cap, well above observed usage. It is a safety stop,
not a condition. An attempt that hits it is recorded as capped and rerun under
the plan's rule; it stays in the assigned population and leaves the
completed-only distributions. Never add runs until a preferred stack wins, and
never choose representative apps after seeing scores.

Run balanced waves, with every stack in each wave, in the plan's rotated order.
Host capacity delays a launch; it never changes the requested parallelism.
Report contention separately from application defects, and keep batch identity
if capacity changes between batches.

## Measures

Cost and completion are the two primary outcomes. Never fold them into one
composite.

| Measure | Interpretation |
| --- | --- |
| Check completion | Passed checks over selected checks, with both counts |
| Feature completion | Fully passed features over selected features |
| Full target delivery | Complete targets over all assigned attempts; stops and exclusions stay in the denominator |
| Reach | Features started, passed, failed, and blocked at each depth |
| Cost | From receipts at the plan's rates; exact, upper bound, and unknown stay distinct, and unknown is never zero |
| Tokens | Input, output, cache reads, and cache writes separately |
| Time | Wall time, planned pauses, and working time separately |
| Reliability | Harness and provider failures, cleanup failures, and cap stops |
| Regression | Checks that passed and later failed, with the checkpoint that lost them |

Eligibility is per measure: a completed run keeps its completion when its cost
is unknown, but a run whose missing receipts prevent cap verification is not
eligible for comparison. Blocked work is not a measured failure; label it
blocked. Do not sum repeated checks across depths, and do not report completion
only among runs that reached the target depth.

Show every attempt with the median, interquartile range, and mean, and state
the sample size beside every comparison. Analyze whole attempts or blocks; a
check is not an independent sample, and levels, repairs, and regrades of one
store are not new builds. Use a binomial interval for full-target success
rates. For a confirmation, choose the smallest decision-relevant difference
first and size the batch from observed between-build variation. Three runs per
stack support an initial comparison, not a claim of superiority.

## What the results can claim

- A package-level difference: this agent, with this stack setup, delivered
  more or less. Not why.
- Delivery under the gating policy. A failed feature blocks the features built
  on it, so two attempts can receive different later work. Show the originating
  failure and its blocked work.
- Coverage of the frozen checklist, on this ecommerce workload. For a broader
  claim, preselect materially different workloads.
- Completion that depends on what the checks can observe. Report measurement
  failures separately and inspect their distribution by stack.
- Cost conditional on the cap. Report any raised-cap rerun.

Qualification of the exact selection, which proves every check on every
stack's reference app, seeded bugs, and the empty app, is required before
results are published as verified; see the
[qualify-checks skill](../skills/qualify-checks/SKILL.md). Collection can start
before that, labelled provisional.

## Failure review and external review

Review failures with the [grading procedure](grading.md#review-a-failed-check),
without stack labels where feasible. Do not repair generated stores by hand in
the primary dataset. A new prompt or interface requirement needs a new cohort.

Before a public comparative claim, get independent external review of the
frozen protocol, exclusions, scoring, and analysis; record unresolved
objections and reviewer affiliations. Review by parallel agents is an internal
check. Do not claim replication until another team reproduces the method.

Keep the complete campaign evidence privately, archived with the
[archive-evidence skill](../skills/archive-evidence/SKILL.md). The research
pack from `campaign export` omits source, transcripts, and media; review its
free text before sharing it.
