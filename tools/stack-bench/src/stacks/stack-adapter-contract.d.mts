export interface StackCapability {
  readonly operations: readonly string[];
  execute(operation: string, input: unknown): unknown;
}

export interface StackAdapter {
  id: string;
  version: string;
  capabilities: Readonly<Record<string, StackCapability | undefined>>;
}

export interface StackAdapterRegistry {
  readonly ids: readonly string[];
  get(id: string): StackAdapter;
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

export function createStackAdapterRegistry(
  adapters: readonly StackAdapter[],
): StackAdapterRegistry;
