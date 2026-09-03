import { ActionApplicationFailure, ActionInconclusive } from './action-contract.js';
import { finding, renderFinding } from './action-findings.js';
import type { FailedFindingKind, FindingFields, InconclusiveFindingKind } from './action-findings.js';

export type UnknownRecord = Record<string, unknown>;
export type HeaderRecord = Record<string, string>;

export interface Locator {
  click(options?: unknown): Promise<void>;
  count(): Promise<number>;
  fill(value: string): Promise<void>;
  first(): Locator;
  getAttribute(name: string): Promise<string | null>;
  innerText(): Promise<string>;
  isVisible(): Promise<boolean>;
  or(locator: Locator): Locator;
  press(key: string): Promise<void>;
  waitFor(options?: unknown): Promise<void>;
}

export interface BrowserResponse {
  ok(): boolean;
  status(): number;
}

export interface BrowserPage {
  readonly request: {
    fetch(url: string, options: UnknownRecord): Promise<BrowserResponse>;
  };
  evaluate<Result>(callback: () => Result): Promise<Result>;
  getByRole(role: string, options: UnknownRecord): Locator;
  locator(selector: string, options?: UnknownRecord): Locator;
}

export interface CapturedWrite {
  readonly body: UnknownRecord | null;
  readonly headers: HeaderRecord;
  readonly method: string;
  readonly url: string;
}

export interface ReplayResult {
  readonly accepted?: boolean;
  readonly applicationRejected?: boolean;
  readonly inconclusive?: boolean;
  readonly method?: string;
  readonly namedAction?: string;
  readonly reason?: string;
  readonly status?: number;
  readonly url?: string;
}

export interface ForgeResult {
  readonly accepted?: boolean;
  readonly inconclusive?: boolean;
  readonly reason: string;
  readonly status?: number;
  readonly tamperedField?: string;
}

export interface ActionCall {
  readonly accepted: boolean;
  readonly action: string;
  readonly applicationRejected: boolean;
  readonly method: string;
  readonly operation: {
    readonly reducer: string | null;
    readonly path: string | null;
    readonly method: string;
  };
  readonly status: number;
  readonly url: string;
}

export interface Actor {
  readonly context: {
    cookies(): Promise<readonly { readonly name: string; readonly value: string }[]>;
  };
  readonly lastWrite?: CapturedWrite;
  readonly lastWsWrite?: { readonly event: string; readonly body: UnknownRecord };
  readonly name: string;
  readonly page: BrowserPage;
  readonly received: readonly string[];
  readonly writes: readonly CapturedWrite[];
  actionCall?: ActionCall;
  forge?: ForgeResult;
  replay?: ReplayResult;
  loc(testid: string, options?: UnknownRecord): Locator;
  wasSent(value: string): boolean;
}

export interface BrowserCapability {
  readonly defaultWithin: number;
  roomName(value: string): string;
  scopedUser(value: string): string;
  sleep(milliseconds: number, signal: AbortSignal): Promise<void>;
  testId(id: string): string;
}

export interface TransportCapability {
  readonly defaultWithin: number;
  readonly verification: {
    unverified(message: string): void;
    verified(message: string): void;
  };
  expand(value: string): string;
  sleep(milliseconds: number, signal: AbortSignal): Promise<void>;
}

export interface ActorCapabilities {
  readonly actors: { get(name: string): Actor | undefined };
}

export interface BrowserActorCapabilities extends ActorCapabilities {
  readonly 'browser-interaction': BrowserCapability;
}

export interface TransportActorCapabilities extends ActorCapabilities {
  readonly 'transport-observation': TransportCapability;
}

export interface ActorActionArguments<Input, Capabilities extends ActorCapabilities = ActorCapabilities> {
  readonly capabilities: Capabilities;
  readonly input: Input;
  readonly signal: AbortSignal;
}

// The only ways an executor fails: a finding from the catalog. The error
// message is the rendered finding, so every reader shows the same sentence.
export function fail<K extends FailedFindingKind>(kind: K, fields: FindingFields[K]): never {
  const value = finding(kind, fields);
  throw new ActionApplicationFailure(renderFinding(value), { finding: value });
}

export function inconclusive<K extends InconclusiveFindingKind>(kind: K,
  fields: FindingFields[K]): never {
  const value = finding(kind, fields);
  throw new ActionInconclusive(renderFinding(value), { finding: value });
}

export function actorFor<T>(
  capabilities: { readonly actors: { get(name: string): T | undefined } },
  name: string,
): T {
  const actor = capabilities.actors.get(name);
  if (!actor) throw new Error(`harness did not create actor "${name}"`);
  return actor;
}

export const browserFor = (capabilities: BrowserActorCapabilities): BrowserCapability =>
  capabilities['browser-interaction'];

export const transportFor = (capabilities: TransportActorCapabilities): TransportCapability =>
  capabilities['transport-observation'];

export const pad = (index: number, count: number): string =>
  String(index).padStart(String(count).length, '0');
