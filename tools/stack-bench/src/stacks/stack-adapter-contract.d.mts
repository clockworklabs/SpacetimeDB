export interface StackAdapter {
  id: string;
  capabilities: Record<string, unknown>;
}

export function executeStackCapability(
  adapter: StackAdapter,
  capabilityName: string,
  operation: string,
  input?: unknown,
): unknown;
