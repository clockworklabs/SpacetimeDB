import type { BackendLease } from '../runtime/backend-lease.js';
import type { RuntimeControlMode } from './stack-lifecycle-operations.js';

export const STACK_ADAPTER_SCHEMA_VERSION = 1;
export const STACK_CAPABILITY_SCHEMA_VERSION = 1;

export const STACK_ENGINE_REQUIREMENTS: Readonly<Record<string, readonly string[]>> = Object.freeze({
  ports: Object.freeze(['allocations', 'for-run']),
  lease: Object.freeze(['prepare', 'validate-resources']),
  reset: Object.freeze(['requires-reseed']),
  database: Object.freeze(['prepare']),
  grading: Object.freeze(['context']),
  'named-action': Object.freeze(['request']),
  teardown: Object.freeze(['host']),
  'run-policy': Object.freeze(['reset-enabled', 'supervisor-env', 'retain-host-supported']),
  agent: Object.freeze(['connection-url', 'minimal-guidance-supported', 'default-skills',
    'linux-cli-required', 'setup-metadata', 'server-directory', 'find-database-urls']),
  'build-container': Object.freeze(['plan']),
  orchestrator: Object.freeze(['config']),
});

const ADAPTER_FIELDS = new Set(['schemaVersion', 'id', 'version', 'lifecycle', 'capabilities']);
const CAPABILITY_FIELDS = new Set(['schemaVersion', 'id', 'version', 'operations', 'execute']);
const ID = /^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;

export class StackCapabilityUnsupportedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'StackCapabilityUnsupportedError';
  }
}

export type StackCapabilityExecutor = (operation: string, input: unknown) => unknown;
// The base ports a backend claims, before any per-run offset. A backend that
// serves its own API or runs a database container declares those too.
export interface StackPortBases {
  readonly vite: number;
  readonly express?: number;
  readonly db?: number;
}

// The ports one run owns. express and dbPort are null for a backend that has
// no such port, so a caller must reject null rather than test for undefined.
export interface StackRunPorts {
  readonly vite: number;
  readonly express: number | null;
  readonly dbPort: number | null;
}

export type StackOperation = (input: unknown) => unknown;
export type StackOperationHandler = (input: never) => unknown;

export interface StackCapability {
  schemaVersion: typeof STACK_CAPABILITY_SCHEMA_VERSION;
  id: string;
  version: string;
  operations: readonly string[];
  execute: StackCapabilityExecutor;
}

export interface StackAdapter {
  schemaVersion: typeof STACK_ADAPTER_SCHEMA_VERSION;
  id: string;
  version: string;
  lifecycle: StackLifecycle;
  capabilities: Readonly<Record<string, Readonly<StackCapability>>>;
}

export interface StackLifecycleInput {
  adapterId: string;
  lease: BackendLease;
  app: string;
  port: number;
  probe: string;
  mode: RuntimeControlMode;
  signal?: AbortSignal | null;
}

export interface StackActivationInput {
  leasePath: string;
  leaseToken: string;
  lease: BackendLease;
  cli?: string;
}

export interface StackLifecycle {
  activate(input: StackActivationInput): void;
  control?(input: StackLifecycleInput): Promise<void>;
}

// Dispatching a capability needs only the adapter's name and its capability
// map, so a caller may hold a lease provider without a whole adapter.
export type CapabilityHolder = Pick<StackAdapter, 'id' | 'capabilities'>;

export interface StackAdapterRegistry {
  readonly ids: readonly string[];
  get(id: string): Readonly<StackAdapter>;
}

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function capabilityExecutor(value: unknown): value is StackCapabilityExecutor {
  return typeof value === 'function';
}

function nonEmpty(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0;
}

function strict(value: unknown, fields: ReadonlySet<string>, at: string): Record<string, unknown> {
  if (!object(value)) throw new Error(`${at} must be an object`);
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) throw new Error(`${at}.${key} is unknown`);
  }
  return value;
}

function identifier(value: unknown, at: string): string {
  if (!nonEmpty(value) || !ID.test(value)) throw new Error(`${at} is invalid`);
  return value;
}

function version(value: unknown, at: string): string {
  if (!nonEmpty(value) || !VERSION.test(value)) throw new Error(`${at} is invalid`);
  return value;
}

export function defineStackCapability(
  value: unknown,
  at = 'stack capability',
): Readonly<StackCapability> {
  const record = strict(value, CAPABILITY_FIELDS, at);
  if (record.schemaVersion !== STACK_CAPABILITY_SCHEMA_VERSION) {
    throw new Error(`${at}.schemaVersion is unsupported`);
  }
  const id = identifier(record.id, `${at}.id`);
  const providerVersion = version(record.version, `${at}.version`);
  if (!Array.isArray(record.operations) || record.operations.length === 0) {
    throw new Error(`${at}.operations must be a non-empty array`);
  }
  const declaredOperations = record.operations.map((operation, index) =>
    identifier(operation, `${at}.operations[${index}]`));
  if (new Set(declaredOperations).size !== declaredOperations.length) {
    throw new Error(`${at}.operations contains duplicates`);
  }
  if (!capabilityExecutor(record.execute)) throw new Error(`${at}.execute must be a function`);
  const operations = Object.freeze([...declaredOperations].sort());
  return Object.freeze({ schemaVersion: STACK_CAPABILITY_SCHEMA_VERSION, id,
    version: providerVersion, operations, execute: record.execute });
}

export function defineStackAdapter(value: unknown): Readonly<StackAdapter> {
  const record = strict(value, ADAPTER_FIELDS, 'stack adapter');
  if (record.schemaVersion !== STACK_ADAPTER_SCHEMA_VERSION) {
    throw new Error(`stack adapter ${record.id ?? '<unknown>'} uses unsupported schema ${record.schemaVersion}`);
  }
  const id = identifier(record.id, 'stack adapter.id');
  const adapterVersion = version(record.version, `stack adapter ${id}.version`);
  if (!object(record.lifecycle) || typeof record.lifecycle.activate !== 'function'
    || (record.lifecycle.control !== undefined && typeof record.lifecycle.control !== 'function')) {
    throw new Error(`stack adapter ${id}.lifecycle is invalid`);
  }
  if (!object(record.capabilities)) throw new Error(`stack adapter ${id}.capabilities must be an object`);
  const capabilities: Record<string, Readonly<StackCapability>> = {};
  for (const [name, provider] of Object.entries(record.capabilities)) {
    identifier(name, `stack adapter ${id} capability name`);
    capabilities[name] = defineStackCapability(provider,
      `stack adapter ${id}.capabilities.${name}`);
    if (capabilities[name].id !== `${id}.${name}`) {
      throw new Error(`stack adapter ${id}.capabilities.${name}.id must be ${id}.${name}`);
    }
  }
  if (Object.keys(capabilities).length === 0) {
    throw new Error(`stack adapter ${id} must declare at least one capability`);
  }
  const lifecycle = record.lifecycle as unknown as StackLifecycle;
  return Object.freeze({ schemaVersion: STACK_ADAPTER_SCHEMA_VERSION, id, version: adapterVersion, lifecycle,
    capabilities: Object.freeze(capabilities) });
}

export function executeStackCapability(
  adapter: CapabilityHolder,
  capabilityName: 'teardown',
  operation: 'host',
  input?: unknown,
): boolean;
export function executeStackCapability(
  adapter: CapabilityHolder,
  capabilityName: 'ports',
  operation: 'allocations',
  input?: unknown,
): StackPortBases;
export function executeStackCapability(
  adapter: CapabilityHolder,
  capabilityName: 'ports',
  operation: 'for-run',
  input: unknown,
): StackRunPorts;
export function executeStackCapability(
  adapter: CapabilityHolder,
  capabilityName: string,
  operation: string,
  input?: unknown,
): unknown;
export function executeStackCapability(
  adapter: CapabilityHolder,
  capabilityName: string,
  operation: string,
  input: unknown = {},
): unknown {
  const provider = adapter.capabilities[capabilityName];
  if (!provider) {
    throw new StackCapabilityUnsupportedError(
      `stack adapter ${adapter.id} does not support capability ${capabilityName}`);
  }
  if (!provider.operations.includes(operation)) {
    throw new StackCapabilityUnsupportedError(
      `stack adapter ${adapter.id} capability ${capabilityName} does not support operation ${operation}`);
  }
  return provider.execute(operation, input);
}

export function createStackAdapterRegistry(adapters: unknown): StackAdapterRegistry {
  if (!Array.isArray(adapters)) throw new Error('stack adapter registry requires an array');
  const entries = new Map<string, Readonly<StackAdapter>>();
  for (const source of adapters) {
    const adapter = defineStackAdapter(source);
    for (const [capability, operations] of Object.entries(STACK_ENGINE_REQUIREMENTS)) {
      const provider = adapter.capabilities[capability];
      if (!provider) throw new Error(`stack adapter ${adapter.id} is missing required capability ${capability}`);
      for (const operation of operations) {
        if (!provider.operations.includes(operation)) {
          throw new Error(`stack adapter ${adapter.id} capability ${capability} is missing required operation ${operation}`);
        }
      }
    }
    if (entries.has(adapter.id)) throw new Error(`duplicate stack adapter ${adapter.id}`);
    entries.set(adapter.id, adapter);
  }
  const ids = Object.freeze([...entries.keys()].sort());
  return Object.freeze({
    ids,
    get(id: string): Readonly<StackAdapter> {
      const adapter = entries.get(id);
      if (!adapter) throw new Error(`unknown stack adapter ${JSON.stringify(id)}`);
      return adapter;
    },
  });
}
