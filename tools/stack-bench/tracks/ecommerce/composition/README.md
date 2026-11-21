# Ecommerce composition

This directory defines the ecommerce work StackBench can request and grade.

- `packs/`: selectable features and specifications. A pack names the packs it
  depends on by stable ID.
- `fixtures/`: exact starting data.
- `recipes/`: which packs, fixture, prompts, execution, and scoring a campaign
  gets.
- `sequential.json` and `dependency.json`: mode aliases to recipes.
- `calibrations/`: a recipe bound to its reference apps, seeded bugs, empty-app
  controls, repetition policy, and qualification evidence.

These files are the source of truth. Do not copy versions or qualification
status into documentation.

Selecting a pack changes the requested work; its dependencies join both the
prompt and the grading scope. Selecting checks narrows measurement; the request
is unchanged. A specification's treatment is explained in
[prompting](../../../docs/prompting.md).

## Identity

Recipes are normalized JSON with sorted keys; arrays keep their order where
order matters. `meaningSha256` covers task text, contracts, checks, roles, and
points. `executionSha256` covers fixtures, actions, timing, capabilities, and
budgets. `contentSha256` binds both, and `sourceManifestSha256` binds the raw
source bytes. Version labels are for people; hashes identify content.

## Commands

From `tools/stack-bench`:

```bash
npm run check:composition
npm run check:calibration
npm run pack -- validate <pack.json> --track ecommerce
npm run recipe -- validate <recipe.json> --track ecommerce
npm run recipe -- show <recipe.json> --track ecommerce
npm run recipe -- diff <old-recipe.json> <new-recipe.json> --track ecommerce
```

`recipe show --pack <id>` or `--check <id>` inspects one selected scope and its
hash. Qualification status comes from `qualification status`; see the
[qualify-checks skill](../../../skills/qualify-checks/SKILL.md).
