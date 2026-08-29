export interface CompiledStep {
  do: string;
  actor?: string;
  from?: string;
  fromActor?: string;
  testid?: string;
  contains?: string;
  absent?: boolean;
  in?: {
    testid?: string;
    contains?: string;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

export interface CompiledCriterion {
  id: string;
  desc: string;
  points: number;
  steps: CompiledStep[];
  statedBy?: string;
  [key: string]: unknown;
}

export interface CompiledFeature {
  id: number;
  name: string;
  actors?: string[];
  max?: number;
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
