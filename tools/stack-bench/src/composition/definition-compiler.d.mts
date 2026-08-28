export interface CompiledStep {
  do: string;
  [key: string]: unknown;
}

export interface CompiledCriterion {
  id: string;
  desc: string;
  points: number;
  steps: CompiledStep[];
  [key: string]: unknown;
}

export interface CompiledFeature {
  id: number;
  name: string;
  setup: CompiledStep[];
  criteria: CompiledCriterion[];
  [key: string]: unknown;
}

export interface CompiledScenarioDefinition {
  schemaVersion: number;
  level: number;
  features: CompiledFeature[];
  [key: string]: unknown;
}

export const DEFINITION_SCHEMA_VERSION: number;
export const ACTION_IDS: readonly string[];

export function compileScenarioDefinition(
  input: unknown,
  options?: { source?: string; expectedLevel?: number | null },
): CompiledScenarioDefinition;

export function compileTrackManifest(
  input: unknown,
  options?: { source?: string },
): Record<string, unknown>;
