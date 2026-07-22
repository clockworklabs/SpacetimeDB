# Grading Artifact Templates

Canonical formats for the two grading artifacts produced during a sequential
upgrade run. They match the published results in
[`spacetimedb-ai-test-results`](https://github.com/clockworklabs/spacetimedb-ai-test-results)
so every backend (spacetime / postgres / mongodb) files identically-structured
artifacts — which is what keeps the runs comparable.

| Template | Written by | Lives at |
|---|---|---|
| [`BUG_REPORT.template.md`](BUG_REPORT.template.md) | the grader (manual) | `<backend>/results/chat-app-<ts>/BUG_REPORT.md` |
| [`ITERATION_LOG.template.md`](ITERATION_LOG.template.md) | the fix agent (appends), grader may annotate | `<backend>/results/chat-app-<ts>/ITERATION_LOG.md` |

## Usage

When grading an app and finding bugs, copy `BUG_REPORT.template.md` into the app
directory as `BUG_REPORT.md`, fill it in from observed browser behavior, then run
`./run.sh --fix <app-dir>`. Delete `BUG_REPORT.md` when all features pass — the
harness keys `--fix` on its existence.

`ITERATION_LOG.md` is append-only; one `## Iteration N` block per fix cycle. Its
iteration/reprompt count feeds the "iterations to done" benchmark metric.

> Grading is **manual** (graded in-browser by a human), so there is no dependency
> on the automated Playwright suite for the comparison numbers.
