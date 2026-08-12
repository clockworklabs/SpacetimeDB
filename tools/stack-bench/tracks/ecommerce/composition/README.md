# Ecommerce benchmark composition

This directory is the versioned, mix-and-match source model for the ecommerce
benchmark. It is deliberately separate from the current `track.json` level/suite
manifest while migration parity is proven.

- `packs/` groups related behavior. Pack files own ordered public requirement
  and testing-hook fragments alongside stable check-group IDs, capabilities,
  dependencies, conflicts, and feature/guarantee/control roles.
- `fixtures/` records exact starting products, stock, accounts, and empty state.
- `recipes/` selects exact pack and fixture versions, supplies only global task
  framing, and defines execution order and scoring.
- `promotions.json` contains candidate or promoted aliases such as L1 and L2.
  An alias cannot be promoted to a draft recipe.
- `calibrations/` binds one exact recipe to canonical reference apps, mutation
  manifests, null expectations, repetition policy, stack status, and promotion
  state. Calibration applies to the whole combination, not to packs separately.

The compatibility recipes explicitly use `legacy-source-points` so their current
scores can be compared byte-for-byte during migration. That scoring mode is
rejected unless the recipe declares its legacy level. New recipes use explicit
stable-key weights; the smoke recipe demonstrates that form.

Run the source checks without Docker:

```text
npm run check:composition
npm run check:calibration
```

Current status: these sources compile and prove exact L1/L2 membership, ordering,
check, and score parity. The live L1/L2 runner resolves the candidate alias,
rechecks that parity before launching a browser, and records the exact recipe
identity in every grade and bundle. It still uses the selected scenario files
through the legacy action dispatcher; general execution of arbitrary recipes
and explicit recipe weights remains later work. These recipes are candidates,
not promotion evidence.

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

The current L1 calibration is draft. It hashes all three candidate reference
apps and their exact mutation manifests, requires two clean reference and two
mutation repetitions per supported stack, and gives every zero-point check an
explicit role. That records what remains to qualify; it is not evidence that
qualification already happened.
