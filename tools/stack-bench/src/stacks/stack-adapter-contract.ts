import type { BackendLease } from '../runtime/backend-lease.js';
import type { TextCommandExecutor } from '../runtime/command-executor.js';
import { isExactSemanticVersion } from '../semantic-version.js';
import type { GradingCapabilityId } from '../actions/action-contract.js';
import type { PlatformAuthPatch } from '../actions/auth-request-patch.js';
import type { NamedAction } from '../composition/tracks.js';
import type { LeasedSpacetimeTarget } from '../runtime/spacetime-target.js';
import type { LeasedDatabase } from './backend-reset-guard.js';
import type { CheckoutState } from './checkout-state.js';
import type { StackDatabaseRuntime } from './process-crash.js';
import type { StackLeaseCapability } from './stack-lease-helpers.js';

// What the grader can measure on a stack: the runtime capabilities the stack
// provides and the transport that carries named application actions.
export interface StackGradingSupport {
  // Names the stack's named-action transport; shared code reads declared capabilities, not this name.
  readonly transport: string;
  readonly capabilities: readonly GradingCapabilityId[];
  // The binding named actions need, when the transport name does not imply it.
  readonly namedActionBinding?: 'path' | 'reducer';
  // The database target observers read and write, from the authenticated lease.
  databaseLease?(lease: BackendLease): LeasedDatabase;
  // URL prefixes outside the page origin where the platform receives the application's own writes.
  writeEndpoints?(lease: BackendLease): readonly string[];
  // How the platform's own password requests are changed, for a platform that serves accounts itself.
  authRequestPatch?(lease: BackendLease): PlatformAuthPatch;
}

export interface NamedActionProbe { ok: boolean; status: number; note: string }

// A stack's operator-reviewed reader for a saved application, bound to its source.
// `lease` is the grading database lease; a stack that declares none ignores it.
export interface SavedCheckoutRead {
  getSavedCheckoutState?(input: { account: string; item: string; app: string; exec: TextCommandExecutor;
    reader: { path: string; sha256: string }; lease: LeasedDatabase; spacetime?: LeasedSpacetimeTarget }):
    { state: CheckoutState; schemaSha256: Record<string, string>; scope: 'orders' };
}

// Optional members any adapter may declare; shared code reads them from every adapter.
export interface StackAdapterOptions {
  readonly grading: StackGradingSupport;
  readonly namedAction: {
    // Account actions checked through browser sign-up and sign-in, not password mutations.
    readonly browserAccounts?: readonly string[];
    // Checks a named action from native function metadata instead of issuing it.
    probe?(action: NamedAction): NamedActionProbe | Promise<NamedActionProbe>;
  };
  readonly runtime?: StackDatabaseRuntime;
}

export interface StackPortBases {
  readonly vite: number;
  readonly express?: number;
  readonly db?: number;
}

// A pinned image a release must ship for a stack, under its release role.
export interface StackReleaseImage { readonly role: string; readonly reference: string; readonly description: string }

// What shared code needs about a stack without loading its adapter. Each stack
// declares its own in its identity module; stack-identities.ts registers it.
export interface StackIdentity {
  readonly version: string;
  readonly applicationInterface: string;
  readonly ports: StackPortBases;
  readonly lease: StackLeaseCapability;
  // Memory admission holds during an attempt's first minute, when it differs from the default.
  readonly startupMemoryBytes?: number;
  readonly releaseImages?: readonly StackReleaseImage[];
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
  ports: StackRunPorts;
  leasePath: string;
  leaseToken: string;
  lease: BackendLease;
  // The application directory, for platforms whose own services serve application files.
  app?: string;
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
