import { existsSync, mkdirSync, mkdtempSync, renameSync, rmSync } from 'node:fs';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { ARTIFACT_FILE, readArtifact } from '../evidence/artifacts.js';
import { hashDirectory, sha256 } from '../evidence/provenance.js';
import { snapshotAppSource } from '../runtime/source-snapshot.js';
import { compileCampaignFile } from './campaign-compiler.js';
import type { CampaignAttemptPlan } from './campaign-compiler.js';
import { campaignChildPath } from './campaign-path.js';
import { inspectCampaign } from './campaign-runner.js';
import { addCampaignExtensions, initializeCampaignDirectory, writeCampaignState }
  from './campaign-scheduler.js';
import type { CampaignExtensionSeed } from './campaign-scheduler.js';

type UnknownRecord = Record<string, unknown>;

function key(attempt: CampaignAttemptPlan): string {
  return canonicalDefinitionJson({
    stack: attempt.stack,
    model: attempt.model,
    agentAdapter: attempt.agentAdapter,
    repetition: attempt.repetition,
    condition: { id: attempt.condition.id, version: attempt.condition.version,
      guidance: attempt.condition.guidance, repair: attempt.condition.repair },
  });
}

function object(value: unknown, at: string): UnknownRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${at} must be an object`);
  }
  return value as UnknownRecord;
}

export function prepareCampaignExtension(targetCampaignFile: string, parentDirectory: string,
  outputDirectory: string, fromDepth: number): void {
  if (!Number.isSafeInteger(fromDepth) || fromDepth < 1) {
    throw new Error('extension depth must be a positive integer');
  }
  const targetPlan = compileCampaignFile(resolve(targetCampaignFile));
  if (targetPlan.definition.mode.id !== 'dependency') {
    throw new Error('campaign extension requires a dependency campaign');
  }
  if (targetPlan.definition.mode.workSelection !== 'progressive') {
    throw new Error('campaign extension target must use progressive work selection');
  }
  if (targetPlan.state !== 'frozen') {
    throw new Error('campaign extension requires a complete target test plan');
  }
  const parent = inspectCampaign(resolve(parentDirectory), { requireCurrentInputs: false });
  const target = resolve(outputDirectory);
  if (existsSync(target)) throw new Error('extension output directory already exists');
  const withinParent = relative(parent.paths.root, target);
  if (withinParent === '' || (withinParent !== '..' && !withinParent.startsWith(`..${sep}`))) {
    throw new Error('extension output must be outside the parent campaign directory');
  }

  const parentAttempts = new Map(parent.state.attempts.map(attempt => [key(attempt.plan), attempt]));
  if (parentAttempts.size !== parent.state.attempts.length) {
    throw new Error('parent campaign has ambiguous attempts for extension');
  }

  const prepared = targetPlan.attempts.map(attempt => {
    if (!attempt.levels.includes(fromDepth) || !attempt.levels.some(level => level > fromDepth)) {
      throw new Error(`target attempt ${attempt.id} does not continue after depth ${fromDepth}`);
    }
    const prior = parentAttempts.get(key(attempt));
    if (!prior) throw new Error(`target attempt ${attempt.id} has no matching parent attempt`);
    const execution = prior.executions.at(-1);
    if (prior.status !== 'completed' || execution?.status !== 'completed') {
      throw new Error(`parent attempt ${prior.plan.id} is not complete`);
    }
    const executionDirectory = campaignChildPath(parent.paths.root, execution.output,
      'parent execution');
    const runArtifact = readArtifact(join(executionDirectory, ARTIFACT_FILE.run),
      { expectedKind: 'benchmark_run' });
    const run = object(runArtifact.payload, 'parent run');
    const levels = Array.isArray(run.levels) ? run.levels : [];
    const level = levels.find(candidate => object(candidate, 'parent run level').level === fromDepth);
    const levelRecord = object(level, `parent run depth ${fromDepth}`);
    const outcome = object(levelRecord.outcome, `parent run depth ${fromDepth} outcome`);
    if (levelRecord.graded !== true || outcome.kind !== 'passed') {
      throw new Error(`parent attempt ${prior.plan.id} did not pass depth ${fromDepth}`);
    }
    const checkpointArtifact = readArtifact(
      join(executionDirectory, `level-l${fromDepth}-checkpoint.json`),
      { expectedKind: 'source_checkpoint' });
    const checkpoint = object(checkpointArtifact.payload, 'parent source checkpoint');
    const source = object(checkpoint.source, 'parent source checkpoint source');
    const sourcePath = campaignChildPath(executionDirectory,
      String(source.directory), 'parent checkpoint source');
    const actual = hashDirectory(sourcePath);
    if (source.sha256 !== actual.sha256 || source.files !== actual.files.length) {
      throw new Error(`parent attempt ${prior.plan.id} checkpoint source does not match its evidence`);
    }
    return { attempt, prior, execution, runArtifact, sourcePath, actual };
  });

  mkdirSync(dirname(target), { recursive: true });
  const temporary = mkdtempSync(join(dirname(target), `.${basename(target)}-`));
  try {
    const initialized = initializeCampaignDirectory(targetPlan, temporary);
    const extensions: Record<string, CampaignExtensionSeed> = {};
    for (const item of prepared) {
      const source = `.private/extensions/${item.attempt.id}/source`;
      snapshotAppSource(item.sourcePath, join(temporary, source));
      const copied = hashDirectory(join(temporary, source));
      if (copied.sha256 !== item.actual.sha256 || copied.files.length !== item.actual.files.length) {
        throw new Error(`extension source copy failed for ${item.attempt.id}`);
      }
      extensions[item.attempt.id] = {
        fromDepth,
        source,
        sourceSha256: copied.sha256,
        sourceFiles: copied.files.length,
        parent: {
          campaignId: parent.plan.id,
          campaignSha256: parent.plan.contentSha256,
          attemptId: item.prior.plan.id,
          executionId: item.execution.id,
          runId: item.runArtifact.id,
          runSha256: sha256(canonicalDefinitionJson(item.runArtifact)),
        },
      };
    }
    const state = addCampaignExtensions(initialized.state, extensions);
    writeCampaignState(initialized.paths.state, initialized.plan, state);
    renameSync(temporary, target);
  } catch (error) {
    rmSync(temporary, { recursive: true, force: true });
    throw error;
  }
}
