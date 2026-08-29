export interface CampaignAttemptPlan {
  id: string;
  stack: string;
  model: string;
  guidance: string;
  repetition: number;
  levels: number[];
  agentAdapter: string;
  skills?: unknown;
  pricing?: unknown;
  mode?: { id?: string; version?: string; [key: string]: unknown };
  condition: {
    id: string;
    version: string;
    sha256: string;
    requested?: {
      levels?: Array<{
        level: number;
        recipe?: { id?: string; version?: string };
        selection?: {
          checks?: unknown[];
          observedChecks?: unknown[];
          specifications?: { requested: string[]; expected: string[]; observed: string[] };
          schemaVersion?: number;
        };
      }>;
    };
  };
}

export interface CompiledCampaignPlan {
  id: string;
  version: string;
  title: string;
  state: string;
  contentSha256: string;
  definition: {
    mode?: { id?: string };
    track: string;
    levels: number[];
    repetitions: number;
    budgets: { fixRounds: number; [key: string]: unknown };
    runtime?: { controllerImage?: string | null; buildImage?: string | null };
    analysis: { primaryMetric: string; secondaryMetrics: string[]; dispersion: string;
      [key: string]: unknown };
    selection: unknown;
    pricing: Record<string, unknown>;
  };
  summary: { attempts: number; parallelism: number;
    repetitionsByStack: Record<string, number>; [key: string]: unknown };
  attempts: CampaignAttemptPlan[];
  stacks: Array<{ id: string }>;
  agents: Array<{
    adapter?: string;
    adapterVersion?: string;
    model?: string;
    identity?: { id?: string; version?: string };
  }>;
  featureCatalog: { identity: unknown; [key: string]: unknown };
  dependencyPolicy: { identity: unknown; [key: string]: unknown };
  identities: { engine: { sha256: string; [key: string]: unknown }; [key: string]: unknown };
  bindings: unknown;
  conditions: unknown;
  campaignSchemaVersion: number;
}

export function compileCampaignFile(path: string,
  options?: Record<string, unknown>): CompiledCampaignPlan;
export function validateCompiledCampaignPlan(input: unknown,
  options?: { requireCurrentInputs?: boolean }): CompiledCampaignPlan;
export function campaignIdentity(plan: CompiledCampaignPlan): {
  id: string;
  version: string;
  sha256: string;
};
