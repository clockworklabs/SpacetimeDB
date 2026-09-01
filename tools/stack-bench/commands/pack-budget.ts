#!/usr/bin/env node

import { existsSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

import { artifactPayload, recipeArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.js';
import { calibrationQualificationIdentity, resolveCalibrationForRelease }
  from '../src/composition/calibration-compiler.js';
import { loadPackBudgetEvidence, PACK_BUDGET_POLICY, recommendPackBudgets }
  from '../src/composition/pack-budget.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { isDeclaredLevel, listTracks, loadTrack } from '../src/composition/tracks.js';

interface PackBudgetArgs {
  command: 'recommend';
  track: string;
  level: number;
  evidence: string[];
  out: string;
  recipe?: string;
}

const USAGE = 'usage: pack-budget.js recommend --track <name> --level <n> '
  + '[--recipe <id>@<version>] --evidence <reference.json> [--evidence ...] '
  + '--out <measurement.json>';

export function parsePackBudgetArgs(argv: string[]): PackBudgetArgs {
  const [command, ...options] = argv.slice(2);
  const { values } = parseArgs({ args: options, options: {
    track: { type: 'string' },
    level: { type: 'string' },
    recipe: { type: 'string' },
    evidence: { type: 'string', multiple: true },
    out: { type: 'string' },
  } });
  const level = Number(values.level);
  const evidence = (values.evidence ?? []).map(path => resolve(path));
  if (command !== 'recommend' || !values.track || !Number.isInteger(level) || level < 1
    || !evidence.length || !values.out) throw new Error(USAGE);
  if (new Set(evidence).size !== evidence.length) throw new Error('--evidence paths must be unique');
  return { command, track: values.track, level, evidence, out: resolve(values.out),
    ...(values.recipe ? { recipe: values.recipe } : {}) };
}

function main(): void {
  const args = parsePackBudgetArgs(process.argv);
  if (!listTracks().includes(args.track)) throw new Error(`unknown track ${args.track}`);
  const track = loadTrack(args.track);
  if (!isDeclaredLevel(track, args.level)) throw new Error(`L${args.level} is not declared for ${args.track}`);
  const binding = resolveRecipeRelease(track, args.level, args.recipe);
  if (!binding) throw new Error(`${args.track} L${args.level} has no recipe release`);
  const calibration = resolveCalibrationForRelease(binding.release, { trackRoot: track.dir });
  if (!calibration) throw new Error(`${binding.release.id}@${binding.release.version} has no calibration`);
  const loaded = loadPackBudgetEvidence(args.evidence);
  const result = recommendPackBudgets({ binding, calibration, evidence: loaded });
  if (existsSync(args.out)) throw new Error(`refusing to replace existing budget measurement: ${args.out}`);
  const id = `pack-budget-${args.track}-l${args.level}-${new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14)}`;
  const artifact = writeArtifact(args.out, { kind: 'pack_budget_measurement', id,
    identities: recipeArtifactIdentities(binding.release, {
      calibration: { ...calibrationQualificationIdentity(calibration), state: calibration.state },
    }),
    payload: { schemaVersion: 1, track: args.track, level: args.level, policy: PACK_BUDGET_POLICY,
      runner: result.measuredRunner,
      evidence: loaded.map(item => {
        const stackAdapter = item.artifact.identities.stackAdapter;
        if (!stackAdapter) throw new Error(`${item.path} has no stack adapter identity`);
        return { path: relative(dirname(args.out), item.path).replaceAll('\\', '/'),
          sha256: item.sha256, stack: stackAdapter.id };
      }),
      samples: result.samples, recommendations: result.recommendations } });
  console.log(JSON.stringify(artifactPayload(artifact), null, 2));
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try { main(); }
  catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 2;
  }
}
