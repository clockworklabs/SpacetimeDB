import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.js';
import type { CampaignState } from './campaign-scheduler.js';

export interface InspectedCampaign {
  plan: CompiledCampaignPlan;
  state: CampaignState;
  paths: { root: string; state: string; [key: string]: string };
}

export interface CampaignAdmissionPreflightRequest {
  backends: string[];
  track: string;
  levels: string;
  levelList: number[];
  runIndex: number;
  parallelism: number;
  agentAdapter: string;
  packIds: unknown[];
  checkKeys: unknown[];
  requestedScopes: unknown[];
  featureCatalog: {
    identity: { sha256: string };
    definition: { nodes: Array<{ level: number }> };
  };
  mode: unknown;
  smoke: true;
  image: string;
  resultsDir: string;
}

export function attemptArgv(plan: CompiledCampaignPlan, attempt: CampaignAttemptPlan,
  output: string, runIndex: number, campaignPlanPath?: string | null,
  progressionResume?: string | null, campaignAdmissionId?: string | null,
  options?: Record<string, unknown>): string[];

export function runCampaignAdmission(
  plan: CompiledCampaignPlan,
  directory: string,
  options?: {
    env?: NodeJS.ProcessEnv;
    preflight?: (request: CampaignAdmissionPreflightRequest, options: {
      env: NodeJS.ProcessEnv;
    }) => unknown;
    now?: string;
    uuid?: () => string;
  },
): { id: string; path: string; payload: { ok: boolean } };

export function inspectCampaign(directory: string,
  options?: { requireCurrentInputs?: boolean }): InspectedCampaign;
export function validateCampaignRun(plan: CompiledCampaignPlan, attempt: CampaignAttemptPlan,
  input: unknown, options?: { buildImage?: string | null; resultDir?: string | null }): unknown;
export function prepareCampaign(campaignFile: string, directory: string): InspectedCampaign;
export function reconcileCampaign(campaignFile: string, directory: string): CampaignState;
export function validateCampaignRun<T extends Record<string, unknown>>(
  plan: unknown,
  attempt: unknown,
  run: T,
  options?: { resultDir?: string | null },
): T;
export function executeCampaign(campaignFile: string, directory: string, options: {
  mode: 'model-free-trial' | 'frozen';
  signal: AbortSignal;
}): Promise<CampaignState>;
