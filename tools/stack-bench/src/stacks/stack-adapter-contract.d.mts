export interface StackAdapter {
  id: string;
  version: string;
  capabilities: Record<string, unknown>;
}

export class StackCapabilityUnsupportedError extends Error {}

export function executeStackCapability(
  adapter: StackAdapter,
  capabilityName: string,
  operation: string,
  input?: unknown,
): unknown;
