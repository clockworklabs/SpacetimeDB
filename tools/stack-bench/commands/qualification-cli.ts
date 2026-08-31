#!/usr/bin/env node

import { existsSync, readFileSync } from 'node:fs';
import { basename, dirname, extname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { calibrationQualificationIdentity, resolveCalibrationForRelease } from '../src/composition/calibration-compiler.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { isDeclaredLevel, listTracks, loadTrack } from '../src/composition/tracks.js';
import { PACK_BUDGET_POLICY } from '../src/composition/pack-budget.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { companionReferenceArtifactPath } from '../src/references/reference-live.js';
import type { CalibrationPlan } from '../src/composition/calibration-compiler.js';
import type { RecipeBinding, RecipeRelease } from '../src/composition/recipe-release.js';

interface QualificationArgs {
  command?: string;
  track: string | null;
  level: number | null;
  recipe?: string;
}

interface QualificationBlocker {
  code: string;
  path: string;
  summary: string;
}

export function parseQualificationArgs(argv: string[]): QualificationArgs {
  const args: QualificationArgs = { command: argv[2], track: null, level: null };
  for (let index = 3; index < argv.length; index += 1) {
    if (argv[index] === '--track') args.track = argv[++index] ?? '';
    else if (argv[index] === '--level') args.level = Number(argv[++index] ?? '');
    else if (argv[index] === '--recipe') args.recipe = argv[++index] ?? '';
    else throw new Error(`unknown qualification option ${argv[index]}`);
  }
  if (args.command !== 'status' || typeof args.track !== 'string' || !args.track
    || args.level === null || !Number.isInteger(args.level) || args.level < 1) {
    throw new Error('usage: node dist/commands/qualification-cli.js status --track <name> --level <positive integer> '
      + '[--recipe <id>@<version>]');
  }
  return args;
}

function blocker(code: string, path: string, summary: string): QualificationBlocker {
  return { code, path, summary };
}

function evidencePlan(calibration: CalibrationPlan) {
  const stacks = calibration.qualification.stacks
    .filter(stack => stack.status !== 'unsupported').map(stack => stack.id).sort();
  const evidence = [];
  for (const stack of stacks) {
    for (let repetition = 1; repetition <= calibration.qualification.referenceRepetitions; repetition += 1) {
      evidence.push({ kind: 'reference', stack, repetition });
    }
    for (let repetition = 1; repetition <= calibration.qualification.mutationRepetitions; repetition += 1) {
      evidence.push({ kind: 'mutation', stack, repetition });
    }
  }
  for (let repetition = 1; repetition <= calibration.nullControl.repetitions; repetition += 1) {
    evidence.push({ kind: 'null', stack: null, repetition });
  }
  return evidence;
}

export interface CalibrationMutationSelection {
  mutations: Array<{ backend: string; path: string; targets: Array<{ id: string }> }>;
}

export function mutationWorkerCount(calibration: CalibrationMutationSelection, stack: string,
  readManifest: (path: string) => { mutations?: { id: string }[] } = path =>
    JSON.parse(readFileSync(resolve(STACK_BENCH_ROOT, path), 'utf8')) as { mutations?: { id: string }[] }) {
  const entry = calibration.mutations.find(candidate => candidate.backend === stack);
  if (!entry) return 1;
  const manifest = readManifest(entry.path);
  const selectedIds = new Set(entry.targets.map(target => target.id));
  const selectedMutations = (manifest.mutations ?? []).filter(mutation =>
    selectedIds.delete(mutation.id));
  if (selectedIds.size) {
    throw new Error(`${stack} calibration selects missing mutations: ${[...selectedIds].sort().join(', ')}`);
  }
  return Math.min(4, Math.max(1, selectedMutations.length));
}

function mutationWorkerOption(calibration: CalibrationPlan, stack: string) {
  const workers = mutationWorkerCount(calibration, stack);
  return workers > 1 ? ` --mutation-workers ${workers}` : '';
}

function qualificationRunDirectory(artifactPath: string): string {
  return join(dirname(artifactPath), `${basename(artifactPath, extname(artifactPath))}.runs`);
}

function defectCheckCoverage(release: RecipeRelease, calibration: CalibrationPlan) {
  const selected = calibration.qualification.checks
    ? new Set(calibration.qualification.checks) : null;
  const scored = release.checkCatalog.filter(check => check.points > 0
    && (selected === null || selected.has(check.stableKey)));
  const scoredByKey = new Map(scored.map(check => [check.stableKey, check]));
  const stacks = calibration.qualification.stacks
    .filter(stack => stack.status !== 'unsupported').map(stack => stack.id).sort();
  return {
    required: 'every scored check has an exact known-defect test on every supported stack',
    totalChecks: scored.length,
    totalPoints: scored.reduce((total, check) => total + check.points, 0),
    stacks: stacks.map(stack => {
      const covered = new Set(calibration.mutations
        .filter(entry => entry.backend === stack)
        .flatMap(entry => entry.targets.flatMap(target => target.stableKeys))
        .filter(key => scoredByKey.has(key)));
      const missing = scored.filter(check => !covered.has(check.stableKey));
      return {
        stack,
        coveredChecks: covered.size,
        coveredPoints: [...covered].reduce((total, key) => total + (scoredByKey.get(key)?.points ?? 0), 0),
        missingChecks: missing.map(check => check.stableKey),
      };
    }),
  };
}

export function qualificationReadiness(trackName: string, level: number, recipe: string | null = null) {
  if (!listTracks().includes(trackName)) throw new Error(`unknown qualification track ${trackName}`);
  const track = loadTrack(trackName);
  if (!isDeclaredLevel(track, level)) {
    throw new Error(`L${level} is not declared for ${trackName}`);
  }
  const binding: RecipeBinding | null = resolveRecipeRelease(track, level, recipe);
  if (!binding) throw new Error(`${trackName} L${level} has no recipe release`);
  const calibration = resolveCalibrationForRelease(binding.release,
    { trackRoot: track.dir, alias: `L${level}` });
  if (!calibration) {
    throw new Error(`${binding.release.id}@${binding.release.version} has no L${level} calibration`);
  }
  const identity = calibrationQualificationIdentity(calibration);
  const launchBlockers = [];
  if (binding.release.state === 'retired') {
    launchBlockers.push(blocker('recipe_retired', 'recipe.state', 'selected recipe is retired'));
  }
  for (const pack of binding.plan.packs) {
    if (pack.budget.status !== 'bounded') {
      launchBlockers.push(blocker('pack_budget_unbounded', `packs.${pack.id}.budget`,
        `${pack.id}@${pack.version} needs a measured maxRuntimeMs before qualification`));
    }
  }
  for (const entry of calibration.references.entries) {
    if (!['candidate', 'active'].includes(String(entry.status))) {
      launchBlockers.push(blocker('reference_unavailable', `references.${entry.backend}`,
        `${entry.id} is ${entry.status}`));
    }
  }
  for (const entry of calibration.mutations) {
    if (!['candidate', 'active'].includes(String(entry.status))) {
      launchBlockers.push(blocker('mutation_unavailable', `mutations.${entry.backend}`,
        `${entry.path} is ${entry.status}`));
    }
  }

  const requiredEvidence = evidencePlan(calibration);
  const defectChecks = defectCheckCoverage(binding.release, calibration);
  const recorded = new Set(calibration.qualification.evidence.map(entry =>
    `${entry.kind}:${entry.stack ?? ''}:${entry.repetition}`));
  const promotionBlockers = [...launchBlockers];
  for (const coverage of defectChecks.stacks.filter(item => item.missingChecks.length > 0)) {
    promotionBlockers.push(blocker('defect_check_coverage_incomplete',
      `defectChecks.${coverage.stack}`,
      `${coverage.coveredChecks}/${defectChecks.totalChecks} scored checks have exact known-defect tests`));
  }
  for (const item of requiredEvidence) {
    const key = `${item.kind}:${item.stack ?? ''}:${item.repetition}`;
    if (!recorded.has(key)) promotionBlockers.push(blocker('evidence_missing', `evidence.${key}`,
      `${key} has no hash-bound qualification artifact`));
  }
  for (const stale of (calibration.qualificationStaleness ?? []) as {
    kind: string; stack?: string; repetition: number; reason: string;
  }[]) {
    const key = `${stale.kind}:${stale.stack ?? ''}:${stale.repetition}`;
    promotionBlockers.push(blocker('qualification_evidence_stale', `evidence.${key}`,
      `${key} must be regenerated: ${stale.reason}`));
  }
  const sourceStates: [string, string][] = [
    ['recipe.state', binding.release.state],
    ['fixture.state', binding.release.components.fixture.state],
    ...binding.release.components.packs.map(pack => [`packs.${pack.id}.state`, pack.state] as [string, string]),
    ['calibration.state', calibration.state],
    ['promotion.status', binding.status],
  ];
  const governance = sourceStates.map(([path, state]) => ({ path, state,
    target: path === 'promotion.status' ? 'promoted' : 'qualified' }));
  governance.push(...calibration.qualification.stacks.map(stack => ({
    path: `qualification.stacks.${stack.id}`, state: stack.status,
    target: stack.status === 'unsupported' ? 'unsupported' : 'qualified',
  })));

  const output = '/var/lib/stack-bench/results/qualification';
  const stacks = calibration.qualification.stacks
    .filter(stack => stack.status !== 'unsupported').map(stack => stack.id).sort();
  const qualificationLevel = Number(calibration.promotion.alias.slice(1));
  const budgetEvidence = stacks.map(stack =>
    `${output}/budget-input/${trackName}-l${qualificationLevel}-${stack}.json`);
  const budgetPreparationRequired = launchBlockers.some(item => item.code === 'pack_budget_unbounded');
  const recipeOption = recipe
    ? ` --recipe ${binding.release.id}@${binding.release.version}` : '';
  const featureCatalog = calibration.qualification.featureCatalog;
  const featureCatalogOption = featureCatalog
    ? ` --feature-catalog ${featureCatalog.id}@${featureCatalog.version}` : '';
  const combinedReferenceEvidence = calibration.qualification.referenceRepetitions
    === calibration.qualification.mutationRepetitions;
  const artifactStem = `${trackName}-l${qualificationLevel}-${binding.release.contentSha256.slice(0, 12)}`;
  const artifactPaths = {
    references: Object.fromEntries(stacks.map(stack => [stack,
      `${output}/${artifactStem}-${stack}-reference.json`])),
    mutations: Object.fromEntries(stacks.map(stack => [stack,
      `${output}/${artifactStem}-${stack}-mutation.json`])),
    null: `${output}/${artifactStem}-null.json`,
  };
  const launchPaths = new Set<string>([artifactPaths.null]);
  for (const stack of stacks) {
    const mutationPath = artifactPaths.mutations[stack];
    const referencePath = artifactPaths.references[stack];
    if (!mutationPath || !referencePath) throw new Error(`qualification path is missing for ${stack}`);
    launchPaths.add(mutationPath);
    launchPaths.add(qualificationRunDirectory(mutationPath));
    launchPaths.add(combinedReferenceEvidence
      ? companionReferenceArtifactPath(mutationPath) : referencePath);
    if (!combinedReferenceEvidence) {
      launchPaths.add(qualificationRunDirectory(referencePath));
    }
  }
  for (const path of [...launchPaths].filter(existsSync).sort()) {
    launchBlockers.push(blocker('qualification_output_exists', path,
      'qualification output already exists'));
  }
  return {
    qualificationSchemaVersion: 1,
    scope: { track: trackName, level, recipe: { id: binding.release.id,
      version: binding.release.version, contentSha256: binding.release.contentSha256 },
    calibration: { ...identity, contentSha256: calibration.contentSha256 },
    runner: calibration.qualification.runner ?? null },
    launch: { ok: launchBlockers.length === 0, blockers: launchBlockers },
    budgetPreparation: {
      required: budgetPreparationRequired,
      policy: PACK_BUDGET_POLICY,
      commands: budgetPreparationRequired ? [
        ...stacks.map((stack, index) =>
          `qualify-reference --backend ${stack} --track ${trackName} --level ${qualificationLevel}${recipeOption}${featureCatalogOption} --repetitions ${calibration.qualification.referenceRepetitions} --out ${budgetEvidence[index]}`),
        `pack-budget recommend --track ${trackName} --level ${qualificationLevel}${recipeOption} ${budgetEvidence
          .map(path => `--evidence ${path}`).join(' ')} --out ${output}/${trackName}-l${qualificationLevel}-pack-budgets.json`,
      ] : [],
    },
    requiredEvidence,
    defectChecks,
    artifactPaths,
    commands: [
      ...stacks.flatMap(stack => [
        ...(!combinedReferenceEvidence ? [
          `qualify-reference --backend ${stack} --track ${trackName} --level ${qualificationLevel}${recipeOption}${featureCatalogOption} --repetitions ${calibration.qualification.referenceRepetitions} --out ${artifactPaths.references[stack]}`,
        ] : []),
        `qualify-reference --backend ${stack} --track ${trackName} --level ${qualificationLevel}${recipeOption}${featureCatalogOption} --repetitions ${calibration.qualification.mutationRepetitions} --mutations --release-candidate${mutationWorkerOption(calibration, stack)} --out ${artifactPaths.mutations[stack]}`,
      ]),
      `qualify-null --track ${trackName} --level ${qualificationLevel}${recipeOption} --out ${artifactPaths.null}`,
    ],
    promotion: { ready: promotionBlockers.length === 0, blockers: promotionBlockers,
      governance },
  };
}

function main() {
  const args = parseQualificationArgs(process.argv);
  if (!args.track || args.level === null) throw new Error('track and level are required');
  console.log(JSON.stringify(qualificationReadiness(args.track, args.level, args.recipe), null, 2));
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try { main(); }
  catch (error: unknown) { console.error(error instanceof Error ? error.message : String(error)); process.exitCode = 2; }
}
