import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.mjs';

export interface DependencyProgress {
  level: number | null;
  work: { current: Array<{ id: string; [key: string]: unknown }>; [key: string]: unknown };
  attempts: {
    total: number;
    level: number;
    maxRemaining: number;
    features: Array<Record<string, unknown>>;
  };
  [key: string]: unknown;
}

export function dependencyProgress(plan: CompiledCampaignPlan, attempt: CampaignAttemptPlan,
  executionDirectory: string | null): DependencyProgress | null;
