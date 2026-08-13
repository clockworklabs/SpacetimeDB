# Canonical reference applications

These applications validate Stack Bench's grader. They are deliberately boring,
auditable test fixtures—not product examples and not evidence that their code is
the best way to build the application.

`registry.json` is the source of truth. A fixture moves through three states:

- `blocked`: no defensible source and oracle set exists.
- `candidate`: preserved evidence and exact source bytes are identified, but the
  imported app has not passed the current harness in Docker.
- `active`: the checked-in app passes every criterion in its declared baseline,
  every mutation anchor is unique, and every mutant is cleanly caught at its
  declared criterion without setup failure, inconclusive evidence, or collateral.

Provenance is explicit and does not imply quality. `historical-import` records a
preserved source tree and opaque old evidence; `authored` identifies a maintained
benchmark oracle with no historical claim. Both remain candidates until the same
current Docker qualification gates pass.

Logical track/level entries may reuse one source directory when they bind the
same exact hash. Qualification and mutation evidence remain separate per level;
the registry does not require duplicate source trees merely to create a new
selection.

Promotion is fail-closed. An application is not active because an old score was
full, because a source tree happens to compile, or because a feature score drops
after mutation. The following must all be true:

1. Dependencies install from committed lockfiles inside the same pinned build
   image used by benchmark runs.
2. The app starts in Docker with run-specific ports and database/module names;
   no host execution mode or workstation path is allowed.
3. Two clean baseline grades pass every scored and zero-point criterion in the
   bound scenario, with identical criterion outcomes.
4. Source contains no `.env`, credentials, generated bindings, build output,
   transcripts, grader artifacts, or mutation backups.
5. Mutation manifests are regenerated against the checked-in bytes and pass the
   hardened criterion-level mutation runner.
6. The registry records the promoted source hash and retained run artifacts.

The fixtures contain deterministic test accounts and may use deliberately
fixture-specific authentication constraints. They must never be presented as
production-ready application templates.

Run `npm run test:references` from `tools/stack-bench` for the model-free Docker
compile matrix. It copies each fixture to a temporary workspace and leaves the
canonical source untouched. The resulting artifact records exactly which
reference hashes compiled. Compile success is not live grading or promotion
evidence.

Run `npm run qualify:reference -- --backend <mongodb|postgres|spacetime>` for
the live gate. It always performs at least two clean Docker runs, audits scored
and zero-point criteria, requires stable criterion fingerprints and one
immutable image, verifies lease/container/lock teardown, and writes an atomic
summary under `results/reference-live/`. MongoDB, PostgreSQL and SpacetimeDB all
passed this gate twice on 2026-08-11; their mutation gates remain open.

Add `--mutations` to run the exact manifest inside the same authenticated
Docker lifecycle. The qualifier first binds the pristine directory hash to the
manifest, then requires a fully passing baseline and a conclusive, declared,
collateral-free kill for every mutant. Candidate manifests may run this gate;
only a passing retained artifact permits changing both fixture and manifest to
`active`.

SpacetimeDB qualification uses dedicated loopback port `3310` by default so it
does not contend with the benchmark's normal `3210` host. Override it with
`--spacetime-port N`; an occupied port is refused and never adopted or killed.
