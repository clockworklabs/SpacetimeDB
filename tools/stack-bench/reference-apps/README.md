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

For full mutation qualification, use the same scope, add
`--mutations --full-mutations`, set `--repetitions` to `mutationRepetitions`, and
choose a new output path. The runner can emit a companion clean-reference artifact
when the baseline repetition count also matches `referenceRepetitions`.
`--mutation-workers` runs independent defect controls with separate leases; it
does not change the selected checks or their pass rules.

The matching dependency L3 empty-app control is:

```bash
node dist/commands/null-control.js --track ecommerce --level 3 \
  --recipe ecommerce.progression-catalog --out <new-null-artifact.json>
```

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
