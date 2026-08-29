export const STACK_ADAPTER_SCHEMA_VERSION: number;
export const STACK_CAPABILITY_SCHEMA_VERSION: number;

export type StackOperation = (input: unknown) => unknown;
export type StackOperationHandler = (input: never) => unknown;

export interface StackCapability {
  readonly schemaVersion: number;
  readonly id: string;
  readonly version: string;
  readonly operations: readonly string[];
  execute(operation: string, input: unknown): unknown;
}

export interface StackAdapter {
  readonly schemaVersion: number;
  readonly id: string;
  readonly version: string;
  readonly capabilities: Readonly<Record<string, StackCapability | undefined>>;
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
