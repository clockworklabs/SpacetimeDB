import type { CompiledCampaignPlan } from '../campaigns/campaign-compiler.mjs';

export function dependencyRuntimeDefinition(featureCatalog: CompiledCampaignPlan['featureCatalog'],
  dependencyPolicy: CompiledCampaignPlan['dependencyPolicy']): unknown;
export function compileProgressionInput(input: unknown): {
  definition: unknown;
  [key: string]: unknown;
};
