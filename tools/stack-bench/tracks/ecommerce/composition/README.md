# Ecommerce composition

This directory defines the versioned ecommerce work that Stack Bench can ask
for and grade.

## Ownership

- `packs/` contains selectable product features and specifications. A pack
  names the packs it depends on by id; the recipe pins every version.
- `fixtures/` contains exact starting data.
- `recipes/` selects pack, fixture, prompt, execution, and scoring versions.
- `promotions.json` maps public sequential aliases to exact recipes.
- `candidates.json` lists draft recipes that can be qualified directly.
- `calibrations/` binds a recipe to reference apps, mutations, null controls,
  repetition policy, and qualification state.

These files are the source of truth. Do not copy current versions, counts, or
qualification status into documentation.

## Selection rules

A pack selection changes the requested product work. Its declared dependencies
join both the prompt and grading scope.

A check selection narrows measurement inside that requested work. It does not
change the product request. A check outside the selected packs is rejected.

Prompt selection and scoring selection remain independent. Specifications have
three treatments:

- **requested:** included in the initial request and scored;
- **expected:** omitted from the initial request, but scored and eligible for
  repair feedback after a failure;
- **observed:** measured after the first build without affecting the main score
  or repair loop.

See the [prompting method](../../../docs/prompting.md) for the complete request
pipeline and examples of each treatment.

## Identity

Recipes use normalized JSON. Object keys are sorted. Arrays keep their order when
order changes meaning or execution.

- `meaningSha256` covers task text, contracts, checks, roles, and points.
- `executionSha256` covers fixtures, actions, timing, capabilities, and budgets.
- `contentSha256` binds meaning and execution.
- `sourceManifestSha256` binds source paths and raw bytes.

Human versions are labels. Hashes identify the exact content.

## Authoring commands

Run these commands from `tools/stack-bench`:

```bash
npm run check:composition
npm run check:calibration
npm run pack -- validate <pack.json> --track ecommerce
npm run recipe -- validate <recipe.json> --track ecommerce
npm run recipe -- show <recipe.json> --track ecommerce
npm run recipe -- diff <old-recipe.json> <new-recipe.json> --track ecommerce
```

Use `recipe show --pack <pack-id>` or `--check <check-id>` to inspect a selected
scope and its hash.

Use the compiler-owned command for current qualification status:

```bash
node dist/commands/qualification-cli.js status --track ecommerce --level <N>
```

Do not infer launch or promotion status from this README. Qualification evidence
must match the exact recipe, fixture, calibration, engine, image, stack, and
runner identities.
