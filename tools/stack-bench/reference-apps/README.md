# Reference applications

Reference applications validate the grader. They are simple, auditable fixtures,
not product examples or recommended application designs.

`registry.json` is the source of truth for fixture identity and status:

- `blocked`: no acceptable source and check set exists;
- `candidate`: exact source is present, but qualification is incomplete;
- `active`: the exact source passed its required reference and mutation gates.

One cumulative source tree can serve several recipes when each registry entry
binds the same source hash. Qualification evidence remains separate for each
recipe and calibration.

## Promotion requirements

An active fixture must satisfy all of these conditions:

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

Run the repetition plan declared by the selected calibration:

```bash
npm run qualify:reference -- --backend <mongodb|postgres|spacetime>
```

The qualifier binds the exact recipe, fixture, source, engine, image, stack,
runner, and check identities. It also verifies lease and resource cleanup.

Add `--mutations` only when mutation evidence is required. The qualifier first
checks the clean baseline, then applies each selected defect through the same
isolated Docker lifecycle.

During development, select only affected defects with `--mutation-id <id>`.
Targeted output is diagnostic evidence. The complete set requires
`--release-candidate` and is used only for release qualification.

Do not edit a registered reference during qualification. A changed source hash
requires new evidence.
