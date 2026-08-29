import type { CompiledCampaignPlan } from '../campaigns/campaign-compiler.mjs';

export interface CompiledProgressionCheck {
  id: string;
  points: number;
}

export interface CompiledProgressionNode {
  id: string;
  title: string;
  level: number;
  questline: string;
  featureRefs: string[];
  promptModules: string[];
  gradingChecks: CompiledProgressionCheck[];
  dependencies: string[];
  dependencyReasons: Record<string, string>;
}

export interface CompiledProgressionDefinition {
  id: string;
  version: string;
  state: string;
  nodes: CompiledProgressionNode[];
  questlines: Array<{ id: string; title: string }>;
}

export function dependencyRuntimeDefinition(featureCatalog: CompiledCampaignPlan['featureCatalog'],
  dependencyPolicy: CompiledCampaignPlan['dependencyPolicy']): unknown;
export function compileProgressionInput(input: unknown): {
  definition: unknown;
  [key: string]: unknown;
};
export function compileProgressionDefinitionFile(
  path: string,
  options?: { trackRoot?: string },
): CompiledProgressionDefinition;
export function validateProgressionInput(input: unknown): {
  definition: unknown;
  [key: string]: unknown;
};
