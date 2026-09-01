import type { BackendLease } from '../runtime/backend-lease.js';
import { isExactSemanticVersion } from '../semantic-version.js';

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
