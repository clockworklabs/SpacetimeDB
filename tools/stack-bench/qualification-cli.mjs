#!/usr/bin/env node

import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { calibrationQualificationIdentity, resolveCalibrationForRelease } from './calibration-compiler.mjs';
import { resolveLegacyRecipeRelease } from './recipe-release.mjs';
import { listTracks, loadTrack } from './tracks.mjs';
import { PACK_BUDGET_POLICY } from './pack-budget.mjs';

export function parseQualificationArgs(argv) {
  const args = { command: argv[2], track: null, level: null };
  for (let index = 3; index < argv.length; index += 1) {
    if (argv[index] === '--track') args.track = argv[++index];
    else if (argv[index] === '--level') args.level = Number(argv[++index]);
    else throw new Error(`unknown qualification option ${argv[index]}`);
  }
  if (args.command !== 'status' || typeof args.track !== 'string' || !args.track
    || !Number.isInteger(args.level) || args.level < 1) {
    throw new Error('usage: qualification-cli.mjs status --track <name> --level <positive integer>');
  }
  return args;
}

function blocker(code, path, summary) {
  return { code, path, summary };
}

function evidencePlan(calibration) {
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

export function qualificationReadiness(trackName, level) {
  if (!listTracks().includes(trackName)) throw new Error(`unknown qualification track ${trackName}`);
  const track = loadTrack(trackName);
  if (!Number.isInteger(level) || level < 1 || !track.suites[String(level)]) {
    throw new Error(`L${level} is not declared for ${trackName}`);
  }
  const binding = resolveLegacyRecipeRelease(track, level);
  if (!binding) throw new Error(`${trackName} L${level} has no recipe release`);
  const calibration = resolveCalibrationForRelease(binding.release,
    { trackRoot: track.dir });
  if (!calibration) throw new Error(`${binding.release.id}@${binding.release.version} has no calibration`);
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
    if (!['candidate', 'active'].includes(entry.status)) {
      launchBlockers.push(blocker('reference_unavailable', `references.${entry.backend}`,
        `${entry.id} is ${entry.status}`));
    }
  }
  for (const entry of calibration.mutations) {
    if (!['candidate', 'active'].includes(entry.status)) {
      launchBlockers.push(blocker('mutation_unavailable', `mutations.${entry.backend}`,
        `${entry.path} is ${entry.status}`));
    }
  }

  const requiredEvidence = evidencePlan(calibration);
  const recorded = new Set(calibration.qualification.evidence.map(entry =>
    `${entry.kind}:${entry.stack ?? ''}:${entry.repetition}`));
  const promotionBlockers = [...launchBlockers];
  for (const item of requiredEvidence) {
    const key = `${item.kind}:${item.stack ?? ''}:${item.repetition}`;
    if (!recorded.has(key)) promotionBlockers.push(blocker('evidence_missing', `evidence.${key}`,
      `${key} has no hash-bound qualification artifact`));
  }
  const sourceStates = [
    ['recipe.state', binding.release.state],
    ['fixture.state', binding.release.components.fixture.state],
    ...binding.release.components.packs.map(pack => [`packs.${pack.id}.state`, pack.state]),
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
  const budgetEvidence = stacks.map(stack => `${output}/budget-input/${trackName}-l${level}-${stack}.json`);
  const budgetPreparationRequired = launchBlockers.some(item => item.code === 'pack_budget_unbounded');
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
          `qualify-reference --backend ${stack} --track ${trackName} --level ${level} --repetitions ${calibration.qualification.referenceRepetitions} --out ${budgetEvidence[index]}`),
        `pack-budget recommend --track ${trackName} --level ${level} ${budgetEvidence
          .map(path => `--evidence ${path}`).join(' ')} --out ${output}/${trackName}-l${level}-pack-budgets.json`,
      ] : [],
    },
    requiredEvidence,
    commands: [
      ...stacks.flatMap(stack => [
        `qualify-reference --backend ${stack} --track ${trackName} --level ${level} --repetitions ${calibration.qualification.referenceRepetitions} --out ${output}/${trackName}-l${level}-${stack}-reference.json`,
        `qualify-reference --backend ${stack} --track ${trackName} --level ${level} --repetitions ${calibration.qualification.mutationRepetitions} --mutations --out ${output}/${trackName}-l${level}-${stack}-mutation.json`,
      ]),
      `qualify-null --track ${trackName} --level ${level} --out ${output}/${trackName}-l${level}-null.json`,
    ],
    promotion: { ready: promotionBlockers.length === 0, blockers: promotionBlockers,
      governance },
  };
}

function main() {
  const args = parseQualificationArgs(process.argv);
  console.log(JSON.stringify(qualificationReadiness(args.track, args.level), null, 2));
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try { main(); }
  catch (error) { console.error(error.message); process.exitCode = 2; }
}
