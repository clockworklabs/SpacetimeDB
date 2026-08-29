export interface StackAdapter {
  id: string;
  version: string;
  capabilities: Record<string, { operations: string[] } | undefined>;
}

export class StackCapabilityUnsupportedError extends Error {}

export function executeStackCapability(
  adapter: StackAdapter,
  capabilityName: 'teardown',
  operation: 'host',
  input?: unknown,
): boolean;

export function executeStackCapability(
  adapter: StackAdapter,
  capabilityName: string,
  operation: string,
  input?: unknown,
): unknown;
