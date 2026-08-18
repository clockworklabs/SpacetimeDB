#!/usr/bin/env node
// Stack Bench grader validation by mutation testing.
//
// A grader that never fails anything is worthless, and one that fails the wrong
// thing is worse. This deliberately breaks a KNOWN-GOOD app one defect at a
// time and checks that the grader (a) notices, and (b) notices in the right
// criterion and nowhere else.
//
// A mutation is cleanly caught only when its declared criterion fails
// conclusively, the known-good baseline is fully passing, and no other
// criterion regresses. Setup failure, inconclusive evidence and collateral
// damage are invalid evidence rather than successful kills.
//
// Usage: node mutation-test.mjs --app <app-dir> --url <url> --mutations mutations/<file>.json
//
// The manifest binds backend, track, scenario, source edits, feature and exact
// criterion ids. Resets and restarts use the same authenticated Docker lease as
// normal grading; mutation control has no host-mode or free-form shell escape.

import {
  copyFileSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve, sep } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { emptyArtifactIdentities, readArtifactPayload, writeRunJson } from "../src/evidence/artifacts.mjs";
import { controlBackend } from "../src/runtime/backend-control.mjs";
import {
  classifyMutationResult,
  groupMutationsByScenario,
  mutationFileEdits,
  mutationTargetKeys,
  releaseScenarioCheckKeys,
  resolveMutationFile,
  validateMutationBaseline,
  validateMutationDefinitions,
} from "../src/evidence/mutation-analysis.mjs";
import { dbName, loadTrack } from "../src/composition/tracks.mjs";
import { resolveRecipeRelease } from "../src/composition/recipe-release.mjs";
import { resetBackend } from "../src/stacks/backend-reset.mjs";
import { fetchStatus } from "../src/runtime/readiness.mjs";
import { executeStackCapability } from "../src/stacks/stack-adapter-contract.mjs";
import { STACK_ADAPTER_REGISTRY } from "../src/stacks/stack-adapters.mjs";

// Resolve tooling relative to this file so the runner works from any directory.
const HERE = dirname(fileURLToPath(import.meta.url));
const GRADER = join(HERE, "grade.mjs");
const TRACKS = resolve(HERE, "..", "tracks");

function parseArgs(argv) {
  const a = {};
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === "--app") a.app = argv[++i];
    else if (argv[i] === "--url") a.url = argv[++i];
    else if (argv[i] === "--mutations") a.mutations = argv[++i];
    else if (argv[i] === "--level") a.level = argv[++i];
    else if (argv[i] === "--spec") a.spec = argv[++i];
    else if (argv[i] === "--backend") a.backend = argv[++i];
    else if (argv[i] === "--track") a.track = argv[++i];
    else if (argv[i] === "--recipe") a.recipe = argv[++i];
    else if (argv[i] === "--db-name") a.dbName = argv[++i];
    else if (argv[i] === "--run-index") a.runIndex = argv[++i];
    else if (argv[i] === "--track-slug") a.slug = argv[++i];
    else if (argv[i] === "--probe") a.probe = argv[++i];
    else if (argv[i] === "--restart-spec") a.restartSpec = JSON.parse(argv[++i]);
    else if (argv[i] === "--out") a.out = argv[++i];
    else if (argv[i] === "--parent-attempt-id") a.parentAttemptId = argv[++i];
    else {
      console.error(`Unknown arg ${argv[i]}`);
      process.exit(2);
    }
  }
  if (!a.app || !a.url || !a.mutations) {
    console.error(
      "Usage: node mutation-test.mjs --app <dir> --url <url> --mutations <file>",
    );
    process.exit(2);
  }
  a.runIndex ??= "0";
  return a;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// Editing a watched source file restarts the server. Grading before it is back
// fails EVERY feature, which reads as "the mutation was caught" in the target
// and as collateral everywhere else — three probes were wasted that way. Wait
// for the app to answer instead of guessing with a sleep.
async function waitForApp(a, seconds = 120) {
  // Git Bash rewrites a bare "/" argument into its own install directory, so a
  // `--probe /` arrives as "c:/Program Files/Git/" and the check quietly waits
  // out its timeout against a path that was never part of the app. Anything
  // that is not a URL or a rooted path is not a probe.
  let path = a.probe ?? "/api/rooms";
  if (
    !/^https?:\/\//.test(path) &&
    (/^[a-zA-Z]:/.test(path) || /Program Files|\\/.test(path))
  ) {
    console.log(
      `  (ignoring mangled --probe "${path}" — pass a full URL to avoid shell path conversion)`,
    );
    path = "/";
  }
  const probe = new URL(path, a.url).toString();
  const deadline = Date.now() + seconds * 1000;
  while (Date.now() < deadline) {
    const status = await fetchStatus(probe, { timeoutMs: 3000 });
    if (status !== null && status < 500) return; // 401/404 still means it is serving
    await sleep(2000);
  }
  throw new Error(`app did not answer at ${probe} within ${seconds}s`);
}

// Grading a dirty database silently lowers scores, and this compares scores
// across runs — an accumulated room would read as a mutation being "caught".
async function reset(a) {
  resetBackend({ backend: a.backend, app: a.app });
  const requiresReseed = executeStackCapability(STACK_ADAPTER_REGISTRY.get(a.backend),
    "reset", "requires-reseed");
  if (a.reseedOnReset && requiresReseed) {
    if (!a.restartSpec) {
      throw new Error(`track ${a.track} requires a lease-authenticated --restart-spec to reseed after reset`);
    }
    await controlBackend(a.restartSpec, "restart");
    await waitForApp(a);
  }
}

async function grade(a, reportPath) {
  await reset(a);
  if (existsSync(reportPath)) unlinkSync(reportPath);
  const gradeArgs = [
    GRADER,
    "--url",
    a.url,
    "--level",
    a.level,
    "--out",
    reportPath,
    "--spec",
    a.spec,
    "--backend",
    a.backend,
    "--track",
    a.track,
    "--app",
    a.app,
  ];
  if (a.dbName) gradeArgs.push("--db-name", a.dbName);
  if (a.restartSpec) gradeArgs.push("--restart-spec", JSON.stringify(a.restartSpec));
  if (a.mutationAttemptId) gradeArgs.push("--parent-attempt-id", a.mutationAttemptId);
  if (a.recipe) gradeArgs.push("--recipe", a.recipe);
  if (a.expectedRecipeSha256) {
    gradeArgs.push("--expected-recipe-sha256", a.expectedRecipeSha256);
  }
  for (const stableKey of a.selectedCheckKeys ?? []) {
    gradeArgs.push("--selected-check", stableKey);
  }
  execFileSync(process.execPath, gradeArgs, {
    stdio: "pipe",
    encoding: "utf8",
    timeout: 15 * 60_000,
  });
  if (!existsSync(reportPath)) {
    throw new Error("grader completed without producing its report");
  }
  return readArtifactPayload(reportPath, { expectedKind: "grade" });
}

const args = parseArgs(process.argv);
const startedAt = Date.now();
const startedIso = new Date(startedAt).toISOString();
args.mutationAttemptId = `mutation-${new Date().toISOString().replace(/[:.]/g, "-")}`;
const artifactPath = (id) =>
  resolve(args.out ?? join(HERE, "..", "results", `${id}.json`));
let spec;

function recordHarnessFailure(error) {
  const generatedAt = new Date().toISOString();
  const id = args.mutationAttemptId;
  const artifact = {
    id,
    kind: "mutation_control",
    startedAt: startedIso,
    completedAt: generatedAt,
    parentAttemptId: args.parentAttemptId ?? null,
    identities: emptyArtifactIdentities({
      fixture: spec?.fixtureSha256 ? { id: "source-under-mutation", sha256: spec.fixtureSha256 } : null,
      stackAdapter: (args.backend ?? spec?.backend) ? { id: args.backend ?? spec.backend } : null,
    }),
    durationMs: Date.now() - startedAt,
    app: resolve(args.app),
    mutations: resolve(args.mutations),
    manifestStatus: spec?.status ?? null,
    fixtureSha256: spec?.fixtureSha256 ?? null,
    spec: args.spec ? resolve(args.spec) : null,
    backend: args.backend ?? spec?.backend ?? null,
    track: args.track ?? spec?.track ?? null,
    ok: false,
    outcome: {
      kind: "harness_failure",
      phase: "mutation-control",
      reason: error.message,
    },
  };
  try {
    const outputPath = artifactPath(id);
    writeRunJson(outputPath, artifact);
    console.error(
      `mutation harness failure: ${error.message}\nartifact: ${outputPath}`,
    );
  } catch (artifactError) {
    console.error(
      `mutation harness failure: ${error.message}\nfailed to write failure artifact: ${artifactError.message}`,
    );
  }
  process.exitCode = 2;
}

async function main() {
  spec = JSON.parse(readFileSync(args.mutations, "utf8"));
  if (spec.schemaVersion !== 1) throw new Error(`unsupported mutation manifest schema ${spec.schemaVersion}`);
  if (!["candidate", "active"].includes(spec.status)) {
    throw new Error(`mutation manifest is ${spec.status ?? "missing a status"}; only candidate or active manifests are executable`);
  }
  for (const field of ["backend", "track", "level", "fixtureSha256"]) {
    if (spec[field] == null) {
      throw new Error(`mutation manifest requires ${field}`);
    }
  }
  if (!/^[a-f0-9]{64}$/.test(spec.fixtureSha256)) {
    throw new Error("mutation manifest fixtureSha256 must be 64 lowercase hex characters");
  }
  if (args.backend && args.backend !== spec.backend) {
    throw new Error(
      `--backend conflicts with manifest backend ${spec.backend}`,
    );
  }
  if (args.track && args.track !== spec.track) {
    throw new Error(`--track conflicts with manifest track ${spec.track}`);
  }
  if (args.level && Number(args.level) !== Number(spec.level)) {
    throw new Error(`--level conflicts with manifest level ${spec.level}`);
  }
  args.backend = spec.backend;
  args.track = spec.track;
  args.level = String(spec.level);
  const track = loadTrack(args.track);
  if (args.recipe) {
    const binding = resolveRecipeRelease(track, Number(args.level), args.recipe);
    if (!binding) throw new Error(`${args.track} L${args.level} has no recipe release`);
    args.recipe = `${binding.release.id}@${binding.release.version}`;
    args.expectedRecipeSha256 = binding.release.contentSha256;
    args.recipeRelease = binding.release;
  }
  args.slug ??= track.slug;
  args.probe ??= track.restartProbe;
  args.dbName ??= dbName(track, Number(args.runIndex));
  args.reseedOnReset = track.reseedOnReset;
  const definitions = validateMutationDefinitions(spec.mutations,
    { defaultScenario: spec.scenario, requireScenario: true });
  if (!definitions.ok) {
    throw new Error(
      `invalid mutation manifest: ${
        definitions.issues.map((issue) =>
          `${issue.mutation ?? "<unnamed>"}:${issue.kind}`
        ).join(", ")
      }`,
    );
  }
  const groups = new Map();
  for (const [scenario, mutations] of groupMutationsByScenario(spec)) {
    const declaredSpec = resolve(HERE, "..", scenario);
    if (!declaredSpec.startsWith(`${TRACKS}${sep}`)) {
      throw new Error(`mutation scenario escapes the tracks directory: ${scenario}`);
    }
    groups.set(declaredSpec, mutations);
  }
  if (args.spec) {
    const requested = resolve(args.spec);
    if (groups.size !== 1 || !groups.has(requested)) {
      throw new Error('--spec conflicts with the mutation manifest scenario selection');
    }
  }
  const work = mkdtempSync(join(tmpdir(), "stack-bench-mutation-"));
  const reportPath = join(work, "grade.json");
  process.once("exit", () => rmSync(work, { recursive: true, force: true }));

  // A killed run never reaches its restore, leaving the app MUTATED on disk with
  // a backup beside it. Every later grade then silently measures the broken app —
  // which read as the grader inventing a defect, and nearly shipped as a
  // three-way comparison. Check BEFORE the baseline, which is the first thing a
  // stale mutation corrupts.
  for (const m of spec.mutations) {
    for (const file of new Set(mutationFileEdits(m).map(edit => edit.file))) {
      const stale = resolveMutationFile(args.app, file) + ".mutation-backup";
      if (existsSync(stale)) {
        throw new Error(
          `${stale} exists; restore the interrupted mutation backup before running again`,
        );
      }
    }
  }

  // Refuse dead and ambiguous edits before spending a full baseline grade. The
  // standalone checker remains useful, but correctness cannot depend on callers
  // remembering to run it first.
  for (const m of spec.mutations) {
    for (const edit of mutationFileEdits(m)) {
      const source = readFileSync(resolveMutationFile(args.app, edit.file), "utf8");
      const matches = source.split(edit.find).length - 1;
      if (matches !== 1) {
        throw new Error(
          `${m.id} anchor matched ${matches} times in ${edit.file}; expected exactly once`,
        );
      }
    }
  }

  const results = [];
  const baselines = [];
  for (const [scenarioPath, mutations] of groups) {
    args.spec = scenarioPath;
    args.selectedCheckKeys = args.recipeRelease
      ? releaseScenarioCheckKeys(args.recipeRelease, track.dir, scenarioPath)
      : [];
    console.log(`Baseline (unmutated app, ${scenarioPath})...`);
    await waitForApp(args);
    const baseline = await grade(args, reportPath);
    const baselineValidation = validateMutationBaseline(baseline, mutations);
    if (!baselineValidation.ok) {
      throw new Error(
        `reference baseline is not known-good for ${scenarioPath}: ${
          JSON.stringify(baselineValidation.issues)
        }`,
      );
    }
    console.log(
      `  baseline: ${baseline.total}/${baseline.max}  ${
        baseline.features.map((f) => `F${f.id}:${f.score}`).join(" ")
      }\n`,
    );
    baselines.push({ scenario: scenarioPath, total: baseline.total, max: baseline.max });

    for (const m of mutations) {
      const byFile = new Map();
      for (const edit of mutationFileEdits(m)) {
        const target = resolveMutationFile(args.app, edit.file);
        if (!byFile.has(target)) {
          byFile.set(target, {
            target,
            backup: `${target}.mutation-backup`,
            original: readFileSync(target, "utf8"),
            edits: [],
          });
        }
        byFile.get(target).edits.push(edit);
      }
      const files = [...byFile.values()];
      const backedUp = [];
      let r;
      try {
        for (const file of files) {
          copyFileSync(file.target, file.backup);
          backedUp.push(file);
        }
        for (const file of files) {
          writeFileSync(file.target,
            file.edits.reduce((src, edit) => src.replace(edit.find, edit.replace),
              file.original));
        }
        await sleep(m.settleMs ?? 4000); // let the watcher notice the edit
        r = await grade(args, reportPath);
      } finally {
        const restoreFailures = [];
        for (const file of backedUp) {
          try {
            copyFileSync(file.backup, file.target);
            unlinkSync(file.backup);
          } catch (error) {
            restoreFailures.push(`${file.target}: ${error.message}`);
          }
        }
        if (restoreFailures.length) {
          throw new Error(`mutation restore failed; do not reuse this app source: ${
            restoreFailures.join('; ')}`);
        }
        await sleep(m.settleMs ?? 4000);
        await reset(args);
        await waitForApp(args); // healthy again before the next probe
      }

      const classified = classifyMutationResult(baseline, r, m);
      results.push({ id: m.id, scenario: scenarioPath,
        targets: mutationTargetKeys(m), ...classified });
      console.log(
        `${classified.status.padEnd(20)} ${m.id} — expected ${
          classified.targetKeys.join(", ")
        }`,
      );
      if (classified.regressions.length) {
        console.log(
          `    failed criteria: ${
            classified.regressions.map((item) => item.key).join(", ")
          }`,
        );
      }
    }
  }

  rmSync(work, { recursive: true, force: true });
  const clean = results.filter((result) => result.status === "CAUGHT");
  const artifact = {
    id: args.mutationAttemptId,
    kind: "mutation_control",
    startedAt: startedIso,
    completedAt: new Date().toISOString(),
    parentAttemptId: args.parentAttemptId ?? null,
    identities: emptyArtifactIdentities({
      fixture: { id: "source-under-mutation", sha256: spec.fixtureSha256 },
      stackAdapter: { id: args.backend },
    }),
    durationMs: Date.now() - startedAt,
    app: resolve(args.app),
    mutations: resolve(args.mutations),
    manifestStatus: spec.status,
    fixtureSha256: spec.fixtureSha256,
    spec: baselines.map(entry => entry.scenario),
    backend: args.backend,
    track: args.track,
    baseline: {
      total: baselines.reduce((sum, entry) => sum + Number(entry.total), 0),
      max: baselines.reduce((sum, entry) => sum + Number(entry.max), 0),
      scenarios: baselines,
    },
    ok: clean.length === results.length,
    summary: { caught: clean.length, total: results.length },
    results,
  };
  const outputPath = artifactPath(artifact.id);
  writeRunJson(outputPath, artifact);
  console.log(`\n${clean.length}/${results.length} mutations cleanly caught`);
  console.log(`artifact: ${outputPath}`);
  if (!artifact.ok) process.exitCode = 1;
}

main().catch(recordHarnessFailure);
