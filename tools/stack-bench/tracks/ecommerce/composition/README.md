# Ecommerce benchmark composition

This directory is the versioned, composable source model for the ecommerce
benchmark. Packs define independently selectable features and specifications;
recipes bind exact pack, fixture, prompt, execution, and scoring versions.

- `packs/` contains both legacy behavior packs and the modular catalog. Modular
  packs explicitly identify either a visible product feature or an optional
  specification. Feature dependencies may add prerequisite features;
  specifications never add features and instead declare which selected feature
  surface makes each check applicable.
- `fixtures/` records exact starting products, stock, accounts, and empty state.
- `recipes/` selects exact pack and fixture versions, supplies only global task
  framing, and defines execution order and scoring.
- `promotions.json` contains the promoted and retired releases behind public
  aliases such as L1 and L2.
- `candidates.json` contains exact draft releases that may be selected for
  qualification without changing a public alias.
- `calibrations/` binds one exact recipe to canonical reference apps, mutation
  manifests, null expectations, repetition policy, stack status, and promotion
  state. Calibration applies to the whole combination, not to packs separately.

Older parity recipes use `legacy-source-points` and remain only where a retained
qualification artifact depends on their exact bytes. Current modular recipes
use explicit weights attached to permanent check keys.

At runtime, pack selection is requested-task selection. Its transitive declared
dependencies are included in both prompt composition and grading. A check
selection is only a measurement filter inside that task: without an explicit
pack selection the full recipe prompt remains, and with one, an out-of-scope
check is rejected rather than grading behavior the agent was never asked to
build. Campaign identities retain explicit packs, resolved task packs, selected
checks, and the exact composed-task hashes.

Run the source checks without Docker:

```text
npm run check:composition
npm run check:calibration
```

The live runner resolves the requested release before launching a browser and
records its exact identity in every grade and bundle. Scenario actions execute
through the versioned, capability-scoped action registry.

Normal runs resolve the promoted alias. A single-level run may select a
catalogued candidate exactly with `--recipe <id>@<version>`. Exact selection is
also accepted by preflight and the reference, mutation, null, budget, and
qualification commands, all of which use the normal runtime path.

Task fragments are source slices identified by permanent IDs, a numeric order,
an exact contained path, optional unique start/end markers, and the task modes
in which they apply. Compilation sorts by order then ID, deduplicates an
explicitly shared ID only when its full definition and selected bytes match,
and includes the resulting text in recipe meaning. L1 session durability is an
independent pack; the smoke recipe demonstrates that account and review
behavior can be selected without also prescribing session durability.

Recipe identity uses canonical JSON: object keys are sorted, while arrays whose
order affects tasks or execution retain their order. Human versions are labels;
the hashes are the proof:

- `meaningSha256` covers task/contract text, permanent check keys, requirements,
  assertions, roles, and recipe points;
- `executionSha256` covers fixtures, selectors, timing, actions, runtime probes,
  capabilities, evidence modes, and budgets;
- `contentSha256` binds those two fingerprints into one exact recipe identity;
- `sourceManifestSha256` separately binds track-relative source names and raw
  bytes, so formatting-only source changes remain visible without creating a
  false semantic cohort.

Saved releases include source digests and the compact check catalog. They do not
copy fixture passwords, prompt contents, or the full executable grader plan.

## Current release status

| Alias | Exact release | Catalog state | Current qualification status |
|---|---|---|---|
| L1 | `ecommerce.l1-modular@2.4.0` | promoted | qualified; 46/46 scored checks have exact defect definitions on MongoDB, PostgreSQL, and SpacetimeDB |
| L2 | `ecommerce.l2-standard@1.5.0` | promoted | qualified; 74/74 scored checks have exact defect definitions on MongoDB, PostgreSQL, and SpacetimeDB |
| Previous L2 | `ecommerce.l2-standard@1.4.0` | retired | its source-bound qualification evidence remains verifiable |

The L2 1.5 calibration binds one reference run and one mutation run for each
supported stack, plus one null-control run. Check the compiler-owned status:

```text
node commands/qualification-cli.mjs status --track ecommerce --level 1 --recipe ecommerce.l1-modular@2.4.0
node commands/qualification-cli.mjs status --track ecommerce --level 2 --recipe ecommerce.l2-standard@1.5.0
```

Specification treatment is independent from feature selection:

- **requested** specifications appear in the initial prompt and are scored;
- **expected** specifications are not prescribed initially, but are scored and
  may enter a correction report after a failure;
- **observed** specifications are evaluated separately after the first build
  and do not alter the requested-feature score or correction loop.

Exact candidate selection cannot change the promotion-catalog hash bound into
qualified calibration evidence. Retired releases remain verifiable but cannot
be launched as new runs. Campaign conditions and direct CLI requests resolve
through the same content-bound task and grading entrypoints.
