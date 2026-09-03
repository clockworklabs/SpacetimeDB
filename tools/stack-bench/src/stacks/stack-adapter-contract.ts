import type { BackendLease } from '../runtime/backend-lease.js';
import type { TextCommandExecutor } from '../runtime/command-executor.js';
import { isExactSemanticVersion } from '../semantic-version.js';
import type { GradingCapabilityId } from '../actions/action-contract.js';

// What the grader can measure on a stack: the runtime capabilities the stack
// provides and the transport that carries named application actions.
export interface StackGradingSupport {
  readonly transport: 'http' | 'reducer';
  readonly capabilities: readonly GradingCapabilityId[];
}

export interface StackPortBases {
  readonly vite: number;
  readonly express?: number;
  readonly db?: number;
}

export interface StackRunPorts {
  readonly vite: number;
  readonly express: number | null;
  readonly dbPort: number | null;
}

export type RuntimeControlMode = 'start' | 'stop' | 'restart';

export interface StackLifecycleInput {
  adapterId: string;
  lease: BackendLease;
  app: string;
  port: number;
  probe: string;
  mode: RuntimeControlMode;
  signal?: AbortSignal | null;
  exec?: TextCommandExecutor;
}

interface StackActivationInput {
  leasePath: string;
  leaseToken: string;
  lease: BackendLease;
  cli?: string;
}

export interface StackLifecycle {
  activate(input: StackActivationInput): void;
  control?(input: StackLifecycleInput): Promise<void>;
  applicationEnvironment?(lease: BackendLease): Record<string, string>;
}

export interface StackAdapterIdentity {
  readonly id: string;
  readonly version: string;
  readonly lifecycle: StackLifecycle;
}

export function createStackAdapterRegistry<const T extends readonly StackAdapterIdentity[]>(adapters: T) {
  const entries = new Map<string, T[number]>();
  for (const adapter of adapters) {
    if (!/^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*$/.test(adapter.id)) {
      throw new Error(`stack adapter id ${JSON.stringify(adapter.id)} is invalid`);
    }
    if (!isExactSemanticVersion(adapter.version)) {
      throw new Error(`stack adapter ${adapter.id}.version is invalid`);
    }
    if (entries.has(adapter.id)) throw new Error(`duplicate stack adapter ${adapter.id}`);
    entries.set(adapter.id, adapter);
  }
  const ids = Object.freeze([...entries.keys()].sort());
  function get<I extends T[number]['id']>(id: I): Extract<T[number], { id: I }>;
  function get(id: string): T[number];
  function get(id: string): T[number] {
    const adapter = entries.get(id);
    if (!adapter) throw new Error(`unknown stack adapter ${JSON.stringify(id)}`);
    return adapter;
  }
  return Object.freeze({ ids, get });
}
