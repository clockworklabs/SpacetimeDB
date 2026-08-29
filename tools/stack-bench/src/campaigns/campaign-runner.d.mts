import type { CompiledCampaignPlan } from './campaign-compiler.mjs';
import type { CampaignState } from './campaign-scheduler.mjs';

export interface InspectedCampaign {
  plan: CompiledCampaignPlan;
  state: CampaignState;
  paths: Record<string, string>;
}

export function inspectCampaign(directory: string,
  options?: { requireCurrentInputs?: boolean }): InspectedCampaign;
export function prepareCampaign(campaignFile: string, directory: string): InspectedCampaign;
export function reconcileCampaign(campaignFile: string, directory: string): CampaignState;
export function executeCampaign(campaignFile: string, directory: string, options: {
  mode: 'model-free-trial' | 'frozen';
  signal: AbortSignal;
}): Promise<CampaignState>;
