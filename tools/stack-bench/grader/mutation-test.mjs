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
// The manifest binds backend, track, scenario, source edits, and stable check
// IDs. The selected level and recipe come from the run. Resets and restarts use
// the same authenticated Docker lease as normal grading. Mutation control has
// no host-mode or free-form shell escape.

import {
  copyFileSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join, relative, resolve, sep } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { currentEngineIdentity, emptyArtifactIdentities, readArtifactPayload,
  writeRunJson } from "../src/evidence/artifacts.js";
import { controlBackend } from "../src/runtime/backend-control.js";
import {
  classifyMutationResult,
  groupMutationsByScenario,
  isRetryableMutationBaseline,
  mutationFileEdits,
  mutationTargetKeys,
  releaseScenarioCheckKeys,
  reusableMutationBaseline,
  resolveMutationFile,
  validateMutationBaseline,
  validateMutationDefinitions,
} from "../src/evidence/mutation-analysis.js";
import { dbName, loadTrack } from "../src/composition/tracks.mjs";
import { resolveRecipeRelease } from "../src/composition/recipe-release.js";
import { resetBackend } from "../src/stacks/backend-reset.js";
import { fetchStatus } from "../src/runtime/readiness.js";
import { executeStackCapability } from "../src/stacks/stack-adapter-contract.js";
import { STACK_ADAPTER_REGISTRY } from "../src/stacks/stack-adapters.js";
import { mutationShard } from "../src/evidence/mutation-shards.js";
import { reusableMutationEvidence } from "../src/evidence/mutation-checkpoint.js";
import { MUTATION_GRADE_MAX_TIMEOUT_MS, mutationGradeTimeoutMs }
  from "../src/evidence/mutation-control.js";
import { assertAppSourceIdentity } from "../src/runtime/source-snapshot.js";

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
    else if (argv[i] === "--selected-check") {
      a.selectedCheckKeys ??= [];
      a.selectedCheckKeys.push(argv[++i]);
    }
    else if (argv[i] === "--db-name") a.dbName = argv[++i];
    else if (argv[i] === "--run-index") a.runIndex = argv[++i];
    else if (argv[i] === "--track-slug") a.slug = argv[++i];
    else if (argv[i] === "--probe") a.probe = argv[++i];
    else if (argv[i] === "--restart-spec") a.restartSpec = JSON.parse(argv[++i]);
    else if (argv[i] === "--out") a.out = argv[++i];
    else if (argv[i] === "--parent-attempt-id") a.parentAttemptId = argv[++i];
    else if (argv[i] === "--mutation-shard-index") a.mutationShardIndex = Number(argv[++i]);
    else if (argv[i] === "--mutation-shard-count") a.mutationShardCount = Number(argv[++i]);
    else if (argv[i] === "--resume-from") a.resumeFrom = resolve(argv[++i]);
    else if (argv[i] === "--checkpoint-out") a.checkpointOut = resolve(argv[++i]);
    else if (argv[i] === "--baseline-bundle") a.baselineBundle = resolve(argv[++i]);
    else if (argv[i] === "--expected-calibration-json") {
      a.expectedCalibrationIdentity = JSON.parse(argv[++i]);
    }
    else if (argv[i] === "--max-runtime-minutes") a.maxRuntimeMinutes = Number(argv[++i]);
    else if (argv[i] === "--image-id") a.imageId = argv[++i];
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
  const shardFields = [a.mutationShardIndex, a.mutationShardCount]
    .filter(value => value !== undefined);
  if (shardFields.length === 1) {
    throw new Error('--mutation-shard-index and --mutation-shard-count must be supplied together');
  }
  a.maxRuntimeMinutes ??= 60;
  if (!Number.isFinite(a.maxRuntimeMinutes) || a.maxRuntimeMinutes < 1
      || a.maxRuntimeMinutes > 120) {
    throw new Error('--max-runtime-minutes must be from 1 through 120');
  }
  if (a.resumeFrom && !a.checkpointOut) a.checkpointOut = a.resumeFrom;
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
    await sleep(250);
  }
  throw new Error(`app did not answer at ${probe} within ${seconds}s`);
}

async function restartAfterSourceChange(a) {
  if (!a.restartSpec) {
    throw new Error('mutation testing requires a lease-authenticated --restart-spec');
  }
  await controlBackend(a.restartSpec, "restart");
  await waitForApp(a);
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

class MutationBatchDeadlineError extends Error {}

async function grade(a, reportPath, deadlineMs = null) {
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
  const timeout = deadlineMs === null
    ? MUTATION_GRADE_MAX_TIMEOUT_MS
    : mutationGradeTimeoutMs(deadlineMs);
  if (timeout === 0) throw new MutationBatchDeadlineError('mutation batch deadline reached');
  try {
    execFileSync(process.execPath, gradeArgs, {
      stdio: "pipe",
      encoding: "utf8",
      timeout,
    });
  } catch (error) {
    if (error?.code === 'ETIMEDOUT' && timeout < MUTATION_GRADE_MAX_TIMEOUT_MS) {
      throw new MutationBatchDeadlineError('mutation grade reached the remaining batch deadline');
    }
    throw error;
  }
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

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function scenarioKey(path) {
  return relative(resolve(HERE, '..'), path).replaceAll('\\', '/');
}

function checkpointGroup(path, mutations, selectedCheckKeys) {
  const scenario = scenarioKey(path);
  const scenarioSha256 = sha256(readFileSync(path));
  const mutationSha256 = sha256(JSON.stringify(mutations));
  const selectionSha256 = sha256(JSON.stringify([...selectedCheckKeys].sort()));
  return { scenario, scenarioSha256, mutationSha256, selectionSha256,
    identitySha256: sha256(JSON.stringify({ scenarioSha256, mutationSha256, selectionSha256 })),
    mutationIds: mutations.map(mutation => mutation.id) };
}

function checkpointIdentity(groups, shard, track) {
  return {
    schemaVersion: 1,
    engineSha256: currentEngineIdentity().sha256,
    recipeSha256: args.expectedRecipeSha256,
    fixtureSha256: spec.fixtureSha256,
    calibrationSha256: args.expectedCalibrationIdentity?.sha256 ?? null,
    imageId: args.imageId ?? null,
    backend: args.backend,
    track: args.track,
    level: Number(args.level),
    trackSha256: sha256(readFileSync(join(track.dir, 'track.json'))),
    shard: { index: shard.index, count: shard.count, mutationIds: shard.mutationIds },
    groups,
  };
}

function resumableEvidence(path, identity) {
  if (!path || !existsSync(path)) return { results: [], baselines: [] };
  const prior = readArtifactPayload(path, { expectedKind: 'mutation_control' });
  const { results, baselines } = reusableMutationEvidence(prior, identity);
  console.log(`Resuming ${results.length}/${identity.shard.mutationIds.length} completed mutations from ${path}`);
  return { results, baselines };
}

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
  if (spec.schemaVersion !== 2) throw new Error(`unsupported mutation manifest schema ${spec.schemaVersion}`);
  const allowedFields = new Set(["schemaVersion", "status", "fixtureSha256", "backend", "track",
    "scenario", "note", "mutations"]);
  const unknownFields = Object.keys(spec).filter(field => !allowedFields.has(field));
  if (unknownFields.length) {
    throw new Error(`mutation manifest has unknown fields: ${unknownFields.join(', ')}`);
  }
  if (!["candidate", "active"].includes(spec.status)) {
    throw new Error(`mutation manifest is ${spec.status ?? "missing a status"}; only candidate or active manifests are executable`);
  }
  for (const field of ["backend", "track", "fixtureSha256"]) {
    if (spec[field] == null) {
      throw new Error(`mutation manifest requires ${field}`);
    }
  }
  if (!/^[a-f0-9]{64}$/.test(spec.fixtureSha256)) {
    throw new Error("mutation manifest fixtureSha256 must be 64 lowercase hex characters");
  }
  if (!Array.isArray(spec.mutations) || spec.mutations.length === 0) {
    throw new Error('mutation manifest requires at least one mutation');
  }
  const fullMutations = spec.mutations;
  const shard = args.mutationShardCount === undefined
    ? { index: 0, count: 1, mutationIds: fullMutations.map(mutation => mutation.id),
      mutations: fullMutations }
    : mutationShard(fullMutations,
      { index: args.mutationShardIndex, count: args.mutationShardCount,
        defaultScenario: spec.scenario });
  if (shard.mutations.length === 0) throw new Error('mutation shard has no assigned mutations');
  spec.mutations = shard.mutations;
  if (args.backend && args.backend !== spec.backend) {
    throw new Error(
      `--backend conflicts with manifest backend ${spec.backend}`,
    );
  }
  if (args.track && args.track !== spec.track) {
    throw new Error(`--track conflicts with manifest track ${spec.track}`);
  }
  if (!Number.isInteger(Number(args.level)) || Number(args.level) < 1) {
    throw new Error('--level must be a positive integer');
  }
  if (!args.recipe) throw new Error('--recipe is required for mutation control');
  args.backend = spec.backend;
  args.track = spec.track;
  const track = loadTrack(args.track);
  const binding = resolveRecipeRelease(track, Number(args.level), args.recipe);
  if (!binding) throw new Error(`${args.track} L${args.level} has no recipe release`);
  args.recipe = `${binding.release.id}@${binding.release.version}`;
  args.expectedRecipeSha256 = binding.release.contentSha256;
  args.recipeRelease = binding.release;
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

  // Reject backups left by an interrupted run before grading the baseline.
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

  // Catch dirty source even when no backup file remains.
  assertAppSourceIdentity(args.app, spec.fixtureSha256, 'mutation fixture');

  // Reject missing or ambiguous edit anchors before baseline grading.
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

  const plans = [...groups].map(([scenarioPath, mutations]) => {
    const selectedCheckKeys = args.recipeRelease
      ? releaseScenarioCheckKeys(args.recipeRelease, track.dir, scenarioPath,
        args.selectedCheckKeys ?? null) : [];
    return { scenarioPath, scenario: scenarioKey(scenarioPath), mutations, selectedCheckKeys,
      checkpoint: checkpointGroup(scenarioPath, mutations, selectedCheckKeys) };
  });
  const cleanBaselineBundle = args.baselineBundle
    ? readArtifactPayload(args.baselineBundle, { expectedKind: 'grade_bundle' })
    : null;
  if (cleanBaselineBundle && !args.expectedCalibrationIdentity) {
    throw new Error('a reusable clean baseline requires its expected calibration identity');
  }
  const checkpoint = checkpointIdentity(plans.map(plan => plan.checkpoint), shard, track);
  const resumed = resumableEvidence(args.resumeFrom, checkpoint);
  const results = [...resumed.results];
  const baselines = [...resumed.baselines];
  const completedIds = new Set(results.map(result => result.id));
  if (completedIds.size !== results.length) {
    throw new Error('mutation checkpoint contains duplicate results');
  }
  const outputPath = artifactPath(args.mutationAttemptId);
  const deadline = startedAt + args.maxRuntimeMinutes * 60_000;

  const createControlArtifact = (status, reason = null) => {
    const ordered = [...results].sort((left, right) =>
      shard.mutationIds.indexOf(left.id) - shard.mutationIds.indexOf(right.id));
    const clean = ordered.filter(result => result.status === 'CAUGHT');
    const orderedBaselines = plans.map(plan => baselines.find(entry =>
      entry.scenario === plan.scenario)).filter(Boolean);
    const remaining = shard.mutationIds.filter(id => !completedIds.has(id));
    return {
      id: args.mutationAttemptId,
      kind: 'mutation_control',
      startedAt: startedIso,
      completedAt: status === 'running' ? null : new Date().toISOString(),
      parentAttemptId: args.parentAttemptId ?? null,
      identities: emptyArtifactIdentities({
        fixture: { id: 'source-under-mutation', sha256: spec.fixtureSha256 },
        recipe: { id: args.recipeRelease.id, version: args.recipeRelease.version,
          sha256: args.expectedRecipeSha256, state: args.recipeRelease.state },
        stackAdapter: { id: args.backend },
      }),
      durationMs: Date.now() - startedAt,
      app: resolve(args.app),
      mutations: resolve(args.mutations),
      manifestStatus: spec.status,
      fixtureSha256: spec.fixtureSha256,
      spec: plans.map(plan => plan.scenario),
      backend: args.backend,
      track: args.track,
      shard: { index: shard.index, count: shard.count, mutationIds: shard.mutationIds },
      baseline: {
        total: orderedBaselines.reduce((sum, entry) => sum + Number(entry.total), 0),
        max: orderedBaselines.reduce((sum, entry) => sum + Number(entry.max), 0),
        scenarios: orderedBaselines,
      },
      ok: status === 'complete' && clean.length === ordered.length
        && ordered.length === shard.mutationIds.length,
      ...(status === 'complete' ? {} : { outcome: { kind: 'incomplete',
        phase: 'mutation-control', reason: reason ?? 'mutation batch is in progress' } }),
      summary: { caught: clean.length, completed: ordered.length,
        total: shard.mutationIds.length, remaining: remaining.length },
      results: ordered,
      checkpoint: { ...checkpoint, status, maxRuntimeMinutes: args.maxRuntimeMinutes,
        updatedAt: new Date().toISOString() },
    };
  };
  const persist = (status, reason = null) => {
    assertAppSourceIdentity(args.app, spec.fixtureSha256,
      'mutation fixture before checkpoint');
    const artifact = createControlArtifact(status, reason);
    writeRunJson(outputPath, artifact);
    if (args.checkpointOut && resolve(args.checkpointOut) !== outputPath) {
      writeRunJson(args.checkpointOut, artifact);
    }
    return artifact;
  };
  const stopAtBudget = () => {
    const artifact = persist('incomplete',
      `mutation batch reached its ${args.maxRuntimeMinutes} minute limit`);
    console.log(`\n${artifact.summary.completed}/${artifact.summary.total} mutations completed; `
      + `${artifact.summary.remaining} remain`);
    console.log(`checkpoint: ${args.checkpointOut ?? outputPath}`);
    process.exitCode = 3;
  };

  for (const plan of plans) {
    const { scenarioPath, scenario, mutations, selectedCheckKeys } = plan;
    const pending = mutations.filter(mutation => !completedIds.has(mutation.id));
    if (pending.length === 0) continue;
    if (Date.now() >= deadline) return stopAtBudget();
    args.spec = scenarioPath;
    args.selectedCheckKeys = selectedCheckKeys;
    let baseline;
    if (cleanBaselineBundle) {
      const reused = reusableMutationBaseline(cleanBaselineBundle, {
        backend: args.backend,
        track: args.track,
        level: Number(args.level),
        fixtureSha256: spec.fixtureSha256,
        recipe: { id: args.recipeRelease.id, version: args.recipeRelease.version,
          sha256: args.expectedRecipeSha256 },
        identities: {
          engine: currentEngineIdentity(),
          calibration: args.expectedCalibrationIdentity,
          stackAdapter: { id: args.backend },
        },
        selectedCheckKeys,
      });
      if (!reused.ok) {
        throw new Error(`cannot reuse clean baseline for ${scenarioPath}: ${reused.reason}`);
      }
      baseline = reused.report;
      console.log(`Baseline (verified clean evidence, ${scenarioPath})...`);
    } else {
      console.log(`Baseline (unmutated app, ${scenarioPath})...`);
      try {
        baseline = await grade(args, reportPath, deadline);
      } catch (error) {
        if (error instanceof MutationBatchDeadlineError) return stopAtBudget();
        throw error;
      }
      let validation = validateMutationBaseline(baseline, mutations);
      if (!validation.ok && isRetryableMutationBaseline(validation.issues)) {
        console.log('  transient baseline failure; retrying once');
        try {
          baseline = await grade(args, reportPath, deadline);
        } catch (error) {
          if (error instanceof MutationBatchDeadlineError) return stopAtBudget();
          throw error;
        }
      }
    }
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
    const baselineEntry = { scenario, identitySha256: plan.checkpoint.identitySha256,
      total: baseline.total, max: baseline.max };
    const priorBaseline = baselines.findIndex(entry => entry.scenario === scenario);
    if (priorBaseline === -1) baselines.push(baselineEntry);
    else baselines[priorBaseline] = baselineEntry;

    for (const m of pending) {
      if (Date.now() >= deadline) return stopAtBudget();
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
      let deadlineReached = false;
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
        await restartAfterSourceChange(args);
        r = await grade(args, reportPath, deadline);
      } catch (error) {
        if (error instanceof MutationBatchDeadlineError) deadlineReached = true;
        else throw error;
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
        for (const file of files) {
          if (existsSync(file.backup) || readFileSync(file.target, 'utf8') !== file.original) {
            throw new Error(`mutation restore verification failed for ${file.target}`);
          }
        }
        // The next mutation restarts the app after it edits the restored source.
        // If grading aborted, restore the clean runtime before the worker exits.
        if (!r) {
          await restartAfterSourceChange(args);
          await reset(args);
        }
      }
      if (deadlineReached) return stopAtBudget();

      const classified = classifyMutationResult(baseline, r, m);
      results.push({ id: m.id, scenario,
        targets: mutationTargetKeys(m), ...classified });
      completedIds.add(m.id);
      persist('running');
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

  // Detect any source change outside the files restored above.
  assertAppSourceIdentity(args.app, spec.fixtureSha256, 'mutation fixture after worker completion');
  // Restore the clean runtime and database before releasing the worker lease.
  await restartAfterSourceChange(args);
  await reset(args);

  rmSync(work, { recursive: true, force: true });
  const artifact = persist('complete');
  console.log(`\n${artifact.summary.caught}/${artifact.summary.total} mutations cleanly caught`);
  console.log(`artifact: ${outputPath}`);
  if (!artifact.ok) process.exitCode = 1;
}

main().catch(recordHarnessFailure);
