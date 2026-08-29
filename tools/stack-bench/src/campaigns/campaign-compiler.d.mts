export interface CampaignAttemptPlan {
  id: string;
  stack: string;
  model: string;
  guidance: string;
  repetition: number;
  levels: number[];
  agentAdapter: string;
  condition: {
    sha256: string;
    requested?: {
      levels?: Array<{
        level: number;
        recipe?: { id?: string; version?: string };
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
  };
  summary: { attempts: number; parallelism: number; [key: string]: unknown };
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
