export interface AgentAdapter {
  id: string;
  version: string;
  costLimit: string;
  apiKeyEnvironmentVariable: string | null;
  credentialEnvironmentVariables: string[];
  credentialFiles: string[];
  outboundDestinations: string[];
  requiredExecutables: string[];
  credentialStatusCommand: string[] | null;
}

export const AGENT_ADAPTER_REGISTRY: Map<string, AgentAdapter>;
export function agentAdapterIdentity(adapter: AgentAdapter): {
  id: string;
  version: string;
  sha256: string;
};
