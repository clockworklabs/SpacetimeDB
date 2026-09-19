# Reference applications

Reference applications validate the grader. They are simple, auditable fixtures,
not product examples or recommended application designs.

`registry.json` is the source of truth for fixture identity, source, and supported
recipes. Qualification evidence proves whether an exact source is usable.

One cumulative source tree can serve several recipes when each registry entry
binds the same source hash. Qualification evidence remains separate for each
recipe and calibration.

## Qualification requirements

A fixture must satisfy all of these conditions before it qualifies a run:

1. Dependencies install from committed lockfiles in the benchmark build image.
2. The app starts in Docker with run-specific ports and database or module names.
3. Every required scored and supporting check passes for the exact recipe.
4. The source contains no secrets, generated bindings, build output,
   transcripts, grader output, or mutation backups.
5. Each mutation has an exact source anchor and produces the intended conclusive
   failure without unrelated failures.
6. The registry records the qualified source hash.

Compile success or an old full score does not promote a fixture.

## Compile fixtures

Run the model-free Docker compile check from `tools/stack-bench`:

```bash
npm run test:references
```

Compile one changed fixture with:

```bash
npm run test:references -- --fixture <fixture-id>
```

The command copies source into a temporary workspace. It does not edit the
registered fixture. Compile success is not live grading evidence.

## Live qualification

Use a configured Linux appliance controller with immutable image identities.
Read the selected calibration before launch. Set the recipe, depth, and repetition
count explicitly; the command defaults are not the calibration policy. For example,
the dependency L3 reference command has this shape:

```bash
node dist/src/references/reference-live.js --backend <mongodb|postgres|spacetime> \
  --track ecommerce --level 3 --recipe ecommerce.progression-catalog \
  --feature-catalog progression/ecommerce.json \
  --repetitions <referenceRepetitions> --out <new-reference-artifact.json>
```

The qualifier binds the exact recipe, fixture, source, engine, image, stack,
runner, and check identities. It also verifies lease and resource cleanup.

`referenceRepetitions` and `mutationRepetitions` come from the selected
[calibration](../tracks/ecommerce/composition/calibrations/). Registered evidence
must match these counts exactly. Extra repeated runs are useful stability
diagnostics, but cannot be substituted for an artifact with a different declared
repetition count. Two clean passes do not prove the absence of intermittent failures.

For mutation evidence, combine `--mutations` with either `--mutation-id <id>`
or `--full-mutations`. The qualifier first
checks the clean baseline, then applies each selected defect through the same
isolated Docker lifecycle.

During development, select only affected defects with `--mutation-id <id>`.
Targeted output is diagnostic evidence. Use `--full-mutations` only when the
complete defect set is required.

Targeted evidence can qualify a defined slice of an unchanged check population.
Each calibration evidence entry then declares `slice.checks` and a hash-pinned
`slice.snapshot` path. Qualifiers save the recipe hash inputs, calibration and
mutation inputs beside their output as `<artifact>.inputs.json`. Preserve these
files with the original artifacts. Reconstructed older inputs must reproduce
the identities in the original evidence; a list of unchanged check IDs is not proof.

The compiler verifies scenario setup, shared inputs, pack budgets, references,
runner, repetition policy and applicable defect definitions. Every required
check must have exactly one reference, mutation and null coverage entry per
required stack/repetition. It rejects missing or overlapping coverage. A targeted
mutation gate may also supply its verified clean baseline for reference coverage.
It retains the artifact's original identities and diagnostic label.

This reuse path supports independently reset dependency scenarios with the same
qualification policy and population. Sequential inherited-stage evidence is not
supported. Changed executable hashes require a reviewed `qualificationReuse`
decision with retained supporting evidence. An unchanged commit label alone is
not sufficient. If any required input or coverage is missing, keep the candidate
unqualified and run only the missing scope; do not substitute a successful summary.

Adding a stack does not invalidate a receipt for an unchanged existing stack or
the stack-neutral empty app. The measured stack must remain in both policies;
check selection and repetition counts must stay unchanged. The added stack still
needs its own complete reference and mutation evidence. Executable changes still
require the equivalence review above.

For full mutation qualification, use the same scope, add
`--mutations --full-mutations`, set `--repetitions` to `mutationRepetitions`, and
choose a new output path. The runner can emit a companion clean-reference artifact
when the baseline repetition count also matches `referenceRepetitions`.
`--mutation-workers` runs independent defect controls with separate leases; it
does not change the selected checks or their pass rules.
The qualification status command generates commands for up to eight workers by
default. Use `qualification status ... --mutation-workers <1-8>` to select fewer.

The matching dependency L3 empty-app control is:

```bash
node dist/commands/null-control.js --track ecommerce --level 3 \
  --recipe ecommerce.progression-catalog --out <new-null-artifact.json>
```

Add repeated `--selected-check <stable-key>` options to run only the affected
null controls. The keys must belong to the calibration's selected checks.

A scored check must fail conclusively on the empty app. Zero awarded points alone
are insufficient if the result is a harness failure or inconclusive. Check the
artifact's failure reasons, not only its process exit code. Reference, mutation,
and null runs make no model calls, but still consume local compute. Obtain
authorization before starting these long-running gates.

Inspect the exact selected definition with:

```bash
node dist/commands/qualification-cli.js status --track ecommerce --level 3 \
  --recipe ecommerce.progression-catalog
```

Static target coverage, a build pass, and historical reports cannot replace
matching live evidence. Preserve failed artifacts and write corrections to new
paths. Never edit old results to match a changed source or calibration.

Do not edit a registered reference during qualification. A changed source hash
requires new evidence.
