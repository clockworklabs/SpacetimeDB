import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { currentEngineIdentity } from '../evidence/artifacts.js';
import { sha256 } from '../evidence/provenance.js';
import { validateReleaseManifest } from '../releases/release-manifest.js';
import { DEFAULT_SPACETIME_SERVER_URI, loopbackHttpUri } from '../runtime/backend-lease.js';

interface CampaignRuntimePlan {
  state: string;
  identities: { engine?: { sha256?: string | null } };
  definition: { runtime: {
    releaseManifestSha256: string | null;
    controllerImage: string | null;
    buildImage: string | null;
    platform: string;
  } };
}

export function verifyCampaignRuntime(plan: CampaignRuntimePlan,
  env: NodeJS.ProcessEnv = process.env): CampaignRuntimePlan['definition']['runtime'] {
  if (plan.state !== 'frozen') return structuredClone(plan.definition.runtime);
  const expected = plan.definition.runtime;
  const plannedEngine = plan.identities?.engine;
  if (!plannedEngine?.sha256 || currentEngineIdentity().sha256 !== plannedEngine.sha256) {
    throw new Error('running Stack Bench engine does not match the frozen campaign');
  }
  if (env.STACK_BENCH_CONTROLLER_IMAGE !== expected.controllerImage) {
    throw new Error('running controller image does not match the frozen campaign');
  }
  if (expected.releaseManifestSha256 === null) return structuredClone(expected);
  if (typeof env.STACK_BENCH_RELEASE_MANIFEST !== 'string'
    || env.STACK_BENCH_RELEASE_MANIFEST.trim() === '') {
    throw new Error('STACK_BENCH_RELEASE_MANIFEST is required for a frozen campaign');
  }
  let bytes;
  try { bytes = readFileSync(resolve(env.STACK_BENCH_RELEASE_MANIFEST)); }
  catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`cannot read frozen campaign release manifest: ${message}`, { cause: error });
  }
  if (sha256(bytes) !== expected.releaseManifestSha256) {
    throw new Error('release manifest does not match the frozen campaign');
  }
  let manifest;
  try { manifest = validateReleaseManifest(JSON.parse(bytes.toString('utf8'))); }
  catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`frozen campaign release manifest is invalid: ${message}`, { cause: error });
  }
  const controller = manifest.images.find(image => image.role === 'controller');
  const build = manifest.images.find(image => image.role === 'build-sandbox');
  if (controller?.reference !== expected.controllerImage
    || build?.reference !== expected.buildImage
    || controller?.platform !== expected.platform
    || build?.platform !== expected.platform) {
    throw new Error('release manifest images do not match the frozen campaign');
  }
  return structuredClone(expected);
}

export function campaignExecutionEnvironment(plan: CampaignRuntimePlan,
  env: NodeJS.ProcessEnv = process.env): NodeJS.ProcessEnv {
  const executionEnv = { ...env };
  if (plan.definition.runtime.buildImage !== null) {
    if (executionEnv.STACK_BENCH_IMAGE
      && executionEnv.STACK_BENCH_IMAGE !== plan.definition.runtime.buildImage) {
      throw new Error('ambient STACK_BENCH_IMAGE conflicts with the campaign build image');
    }
    executionEnv.STACK_BENCH_IMAGE = plan.definition.runtime.buildImage;
  }
  verifyCampaignRuntime(plan, executionEnv);
  return executionEnv;
}

export function campaignSlotEnvironment(env: NodeJS.ProcessEnv, stack: string | null,
  runIndex: number): NodeJS.ProcessEnv {
  const executionEnv = { ...env };
  if (stack !== 'spacetime') return executionEnv;
  const base = loopbackHttpUri(executionEnv.STACK_BENCH_STDB_URI ?? DEFAULT_SPACETIME_SERVER_URI);
  const port = Number(base.port) + runIndex;
  if (!Number.isInteger(runIndex) || runIndex < 0 || port > 65535) {
    throw new Error(`campaign run slot ${runIndex} cannot allocate a SpacetimeDB host port`);
  }
  base.port = String(port);
  executionEnv.STACK_BENCH_STDB_URI = base.toString().replace(/\/$/, '');
  return executionEnv;
}
