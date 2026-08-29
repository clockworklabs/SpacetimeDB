import type { CompiledCampaignPlan } from './campaign-compiler.mjs';
import type { CampaignState } from './campaign-scheduler.js';

export interface InspectedCampaign {
  plan: CompiledCampaignPlan;
  state: CampaignState;
  paths: Record<string, string>;
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
export function prepareCampaign(campaignFile: string, directory: string): InspectedCampaign;
export function reconcileCampaign(campaignFile: string, directory: string): CampaignState;
export function executeCampaign(campaignFile: string, directory: string, options: {
  mode: 'model-free-trial' | 'frozen';
  signal: AbortSignal;
}): Promise<CampaignState>;
