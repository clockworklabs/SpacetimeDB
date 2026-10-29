# Reference apps

A reference app is a correct store for one stack, written by hand to prove the
checks. It is a grading fixture, not a product example or a recommended design.
`registry.json` records each app's source hash and supported recipes.

A reference app qualifies a check selection only when all of this holds:

1. Dependencies install from committed lockfiles in the benchmark build image.
2. The app starts in Docker with run-specific ports and database names.
3. Every required check passes for the exact recipe.
4. The source contains no secrets, generated bindings, build output,
   transcripts, grader output, or seeded-bug backups.
5. Each seeded bug has an exact source anchor and fails at an observation step
   (`expect`, `dbExpect`, or `waitUntilAbsent`) with no unrelated failures. A
   bug that breaks a click, sign-in, or restart instead is
   `CAUGHT_OFF_ASSERTION` and does not count.
6. The registry records the qualified source hash.

Compile success, a build pass, or an old full score does not qualify an app.
Only matching live evidence does.

## Compile

From `tools/stack-bench`, a model-free Docker compile of every app, or one:

```bash
npm run test:references
npm run test:references -- --fixture <fixture-id>
```

The compile copies the source into a temporary workspace and never edits the
registered app.

## Qualify

The live procedure, which runs each app, its seeded bugs, and the empty app on
the appliance and records the evidence in the calibration, is the
[qualify-checks skill](../skills/qualify-checks/SKILL.md). Its rules:

- Set the recipe, depth, and repetition count explicitly from the calibration;
  command defaults are not the policy.
- Write every artifact to a new path. Never edit an old artifact to match a
  changed source or calibration.
- A changed source hash needs new evidence. Do not edit a registered app during
  qualification.
- A code change outside the grading path can keep old evidence only through a
  reviewed `qualificationReuse` decision in the calibration that names the old
  and new executable hashes.
- Adding a stack does not invalidate another stack's evidence, but the new
  stack needs its own complete reference and seeded-bug evidence.
