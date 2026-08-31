# Ecommerce benchmark composition

This directory is the versioned, composable source model for the ecommerce
benchmark. Packs define independently selectable features and specifications;
recipes bind exact pack, fixture, prompt, execution, and scoring versions.

- `packs/` contains feature and specification modules. Packs explicitly identify
  either a visible product feature or an optional
  specification. Feature dependencies may add prerequisite features;
  specifications never add features and instead declare which selected feature
  surface makes each check applicable.
- `fixtures/` records exact starting products, stock, accounts, and empty state.
- `recipes/` selects exact pack and fixture versions, supplies only global task
  framing, and defines execution order and scoring.
- `promotions.json` maps public aliases such as L1 and L2 to exact releases and
  records their promotion state.
- `candidates.json` contains exact draft releases that may be selected for
  qualification without changing a public alias.
- `calibrations/` binds one exact recipe to canonical reference apps, mutation
  manifests, null expectations, repetition policy, stack status, and promotion
  state. Calibration applies to the whole combination, not to packs separately.

Recipes use explicit weights attached to permanent check keys.

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

Normal runs resolve the current alias. A single-level run may select a
catalogued recipe exactly with `--recipe <id>@<version>`. Exact selection is
also accepted by preflight and the reference, mutation, null, budget, and
qualification commands, all of which use the normal runtime path.

Task fragments are source slices identified by permanent IDs, a numeric order,
an exact contained path, optional unique start/end markers, and the task modes
in which they apply. Compilation sorts by order then ID, deduplicates an
explicitly shared ID only when its full definition and selected bytes match,
and includes the resulting text in recipe meaning. L1 session durability is an
independent pack.

Recipe identity uses canonical JSON: object keys are sorted, while arrays whose
order affects tasks or execution retain their order. Human versions are labels;
the hashes are the proof:

- `meaningSha256` covers task/contract text, permanent check keys, requirements,
  assertions, roles, and recipe points;
- `executionSha256` covers fixtures, selectors, timing, actions, runtime probes,
  capabilities, evidence modes, and budgets;
- `contentSha256` binds those two fingerprints into one exact recipe identity;
- `sourceManifestSha256` separately binds track-relative source names and raw
  bytes, so formatting-only source changes remain visible without changing the
  semantic identity.

Saved releases include source digests and the compact check catalog. They do not
copy fixture passwords, prompt contents, or the full executable grader plan.

## Current release status

| Alias | Exact release | Catalog state | Current qualification status |
|---|---|---|---|
| L1 | `ecommerce.sequential-l1@2.5.0` | candidate | 46/46 scored checks have exact defect definitions; no qualification result is accepted |
| L2 | `ecommerce.sequential-l2@1.6.0` | candidate | 74/74 scored checks have exact defect definitions; no qualification result is accepted |
| L3 | `ecommerce.sequential-l3@1.0.0` | candidate | not qualified or promoted |
| Dependency depth 3 | `ecommerce.progression-depth3@2.0.1` | promoted for L1-L3 | qualified on MongoDB, PostgreSQL, and SpacetimeDB for 97 checks and 162 points |
| Dependency progression | `ecommerce.progression-catalog@2.0.1` | candidate | draft L1-L6 catalog; not qualified or promoted |

The L2 1.6 calibration requires one reference run and one mutation run for each
supported stack, plus one null-control run. Check the compiler-owned status:

```text
node dist/commands/qualification-cli.js status --track ecommerce --level 1 --recipe ecommerce.sequential-l1@2.5.0
node dist/commands/qualification-cli.js status --track ecommerce --level 2 --recipe ecommerce.sequential-l2@1.6.0
```

Specification treatment is independent from feature selection:

- **requested** specifications appear in the initial prompt and are scored;
- **expected** specifications are not prescribed initially, but are scored and
  may enter a correction report after a failure;
- **observed** specifications are evaluated separately after the first build
  and do not alter the requested-feature score or correction loop.

Exact candidate selection cannot change the promotion-catalog hash bound into
qualified calibration evidence. Campaign conditions and single-run requests
resolve through the same content-bound task and grading entrypoints.
