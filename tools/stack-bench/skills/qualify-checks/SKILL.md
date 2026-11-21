---
name: qualify-checks
description: Prove the StackBench checks on every stack's reference app, seeded bugs, and the empty app before publishing results
metadata:
  author: clockworklabs
  role: operator
---

# Qualify the checks

Results are provisional until the checks that produced them have current
qualification evidence. Qualification runs every check, on every stack, against
three controls:

- the **reference app**, a correct store: every check must pass;
- the reference app with one **seeded bug** at a time: the checks listed for
  that bug must fail and nothing else may;
- the **empty app**, which answers 404 to everything: every scored check must
  fail.

Any change to a file in the grading import graph makes the evidence stale.
Qualify right before publishing, not after every fix; a full run takes hours.

Every command runs from `tools/stack-bench`. `controller` means:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller
```

## See what is missing

```sh
controller qualification status --track ecommerce --level 3 --recipe ecommerce.progression-catalog
```

The output lists the evidence still required and prints the exact commands to
produce it. Run only what it names; do not repeat evidence whose inputs have not
changed.

## Produce evidence

Build the controller image from a clean tree first
([release](../release/SKILL.md)). Then, per stack:

```sh
controller qualify-reference --backend <postgres|mongodb|spacetime|convex|supabase> --track ecommerce --level 3 --recipe ecommerce.progression-catalog --feature-catalog progression/ecommerce.json --repetitions 1 --mutations --full-mutations --mutation-workers 8 --out <new-path>.json
```

and once for the empty app:

```sh
controller qualify-null --track ecommerce --level 3 --recipe ecommerce.progression-catalog --out <new-path>.json
```

Use a new output path every time. Never edit an old artifact to match a changed
source. Set `--repetitions` to the count the calibration declares
(`tracks/ecommerce/composition/calibrations/dependency-l3.json`).

During development, `--mutation-id <id>` runs one seeded bug, and
`qualify-null --selected-check <key>` runs one empty-app control. That output
is diagnostic; it does not qualify the full scope.

## Record it

1. Copy each artifact byte for byte into `qualification-evidence/<name>/`:
   `<stack>-reference.json`, `<stack>-mutation.json`, and `null.json`, with
   their `.inputs.json` companions.
2. Add an evidence entry per artifact to the calibration:
   `{ kind, stack, repetition, path, sha256 }`.
3. Run `qualification status` again. It must report nothing missing.

## When the code changed but the grading did not

A change outside the grading path, such as plumbing or dashboard work, can keep
old evidence through a `qualificationReuse` block in the calibration that
names the old and new executable hashes. Write it by hand and keep the
supporting diff. A commit label alone is not evidence.

## Pitfalls

- Oversubscribing the host produces load-induced undecided results, such as
  navigation timeouts and a missed crash window. Rerun that stack alone before
  treating it as a defect.
- Seeded bugs must fail at an `expect` or `dbExpect` step. A bug that breaks a
  click or sign-in instead is `CAUGHT_OFF_ASSERTION` and does not count.
