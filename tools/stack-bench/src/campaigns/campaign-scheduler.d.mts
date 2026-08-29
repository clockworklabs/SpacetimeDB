import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.mjs';

export interface CampaignExecution {
  id: string;
  ordinal: number;
  status: string;
  outcome: unknown;
  reason: string | null;
  startedAt: string | null;
  completedAt: string | null;
  runIndex?: number;
  output: string;
}

export interface CampaignAttemptState {
  plan: CampaignAttemptPlan;
  status: string;
  executions: CampaignExecution[];
}

export interface CampaignState {
  campaignId: string;
  campaignSha256: string;
  maxParallel: number;
  status: string;
  attempts: CampaignAttemptState[];
  summary: {
    running: number;
    [key: string]: number;
  };
  createdAt: string;
  updatedAt: string;
}

export function validateCampaignState(input: unknown): CampaignState;
export function readCampaignState(directory: string,
  options?: { requireCurrentInputs?: boolean }): CampaignState;
export function createCampaignState(plan: CompiledCampaignPlan,
  options?: { now?: string }): CampaignState;
export function claimNextAttempt(state: CampaignState,
  options?: { now?: string; admissionId?: string }): {
    state: CampaignState;
    claim: { executionId: string; output: string };
  };
export function finishCampaignExecution(state: CampaignState, executionId: string,
  result: unknown, options?: Record<string, unknown>): CampaignState;
