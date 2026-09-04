import { evidenceNowMs } from '../evidence/evidence-timing.js';
import { isFinding } from './action-findings.js';
import type { Finding } from './action-findings.js';

type UnknownRecord = Record<string, unknown>;
type ActionStatus = 'passed' | 'failed' | 'inconclusive' | 'harness_failure';
type TimerHandle = ReturnType<typeof setTimeout>;

export type ActionCategory =
  | 'application-process'
  | 'browser-interaction'
  | 'browser-observation'
  | 'concurrency'
  | 'database'
  | 'lifecycle'
  | 'timing'
  | 'transport';

interface ActionImplementationArguments {
  readonly input: unknown;
  readonly capabilities: Readonly<Record<string, unknown>>;
  readonly signal: AbortSignal;
}

export type ActionImplementation = (
  arguments_: ActionImplementationArguments,
) => unknown | Promise<unknown>;

export function actionImplementation<Input, Capabilities>(
  implementation: (arguments_: {
    readonly input: Input;
    readonly capabilities: Capabilities;
    readonly signal: AbortSignal;
  }) => unknown | Promise<unknown>,
): ActionImplementation {
  // Compilation and capability selection validate this static dispatch boundary.
  return implementation as unknown as ActionImplementation;
}

export interface ActionPlugin {
  readonly id: string;
  readonly version: string;
  readonly category: ActionCategory;
  readonly compile: (input: unknown) => unknown;
  readonly capabilities: readonly string[];
  readonly timeoutMs: number;
  readonly sensitivity: readonly string[];
  readonly execute: ActionImplementation;
}

export interface ActionRegistry {
  readonly ids: readonly string[];
  get(id: string): ActionPlugin;
}

interface ActionRunContext {
  readonly capabilities: Readonly<Record<string, unknown>>;
  readonly signal: AbortSignal | null;
}

type ActionRunContextInput = Partial<ActionRunContext>;

export interface ActionEvidence {
  readonly schemaVersion: number;
  readonly action: { readonly id: string; readonly version: string };
  readonly status: ActionStatus;
  readonly type: string;
  readonly code: string;
  readonly phase: 'execute';
  readonly summary: string | null;
  // The typed finding behind a failed or inconclusive action; null when the
  // action passed or the harness itself failed.
  readonly finding: Finding | null;
  readonly observation: unknown;
  readonly expected: unknown;
  readonly retryable: boolean;
  readonly timing: {
    readonly startedAtMs: number;
    readonly completedAtMs: number;
    readonly durationMs: number;
    readonly deadlineMs: number;
  };
  readonly attachments: readonly unknown[];
  readonly sensitivity: readonly string[];
}

export const ACTION_EVIDENCE_SCHEMA_VERSION = 2;

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
// Every runtime capability the grader can hand an action. A stack declares
// which of these it provides; an action declares which it needs.
export const GRADING_CAPABILITY_IDS = Object.freeze([
  'actors',
  'application-files',
  'application-lifecycle',
  'backend-lifecycle',
  'browser-interaction',
  'browser-observation',
  'clock',
  'concurrency',
  'database-write',
  'named-actions',
  'subprocess',
  'transport-observation',
] as const);

export type GradingCapabilityId = (typeof GRADING_CAPABILITY_IDS)[number];

export function createActionRegistry(
  plugins: readonly ActionPlugin[],
  { expectedIds = null }: { expectedIds?: readonly string[] | null } = {},
): ActionRegistry {
  const entries = new Map<string, ActionPlugin>();
  const known = new Set<string>(GRADING_CAPABILITY_IDS);
  for (const plugin of plugins) {
    if (entries.has(plugin.id)) throw new Error(`duplicate action registration ${plugin.id}`);
    const unknown = plugin.capabilities.filter(capability => !known.has(capability));
    if (unknown.length) {
      throw new Error(`action ${plugin.id} needs unknown capabilities ${unknown.join(', ')}`);
    }
    entries.set(plugin.id, plugin);
  }
  if (expectedIds !== null) {
    const expected = new Set(expectedIds);
    const missing = [...expected].filter(id => !entries.has(id));
    const unknown = [...entries.keys()].filter(id => !expected.has(id));
    if (missing.length || unknown.length) {
      throw new Error(`action registry mismatch${missing.length ? `; missing ${missing.join(', ')}` : ''}`
        + `${unknown.length ? `; unknown ${unknown.join(', ')}` : ''}`);
    }
  }
  const ids = Object.freeze([...entries.keys()].sort());
  return Object.freeze({
    ids,
    get(id: string): ActionPlugin {
      const plugin = entries.get(id);
      if (!plugin) throw new Error(`unknown registered action ${JSON.stringify(id)}`);
      return plugin;
    },
  });
}

function createActionRunContext(value: ActionRunContextInput = {}): ActionRunContext {
  const capabilities = value.capabilities ?? {};
  return Object.freeze({ capabilities: Object.freeze({ ...capabilities }),
    signal: value.signal ?? null });
}

class ClassifiedActionError extends Error {
  readonly classification: 'application_failure' | 'inconclusive';
  readonly details: UnknownRecord;

  constructor(
    classification: 'application_failure' | 'inconclusive',
    message: string,
    details: UnknownRecord = {},
  ) {
    super(message);
    this.name = this.constructor.name;
    this.classification = classification;
    this.details = details;
  }
}

export class ActionApplicationFailure extends ClassifiedActionError {
  constructor(message: string, details: UnknownRecord = {}) {
    super('application_failure', message, details);
  }
}

export class ActionInconclusive extends ClassifiedActionError {
  constructor(message: string, details: UnknownRecord = {}) {
    super('inconclusive', message, details);
  }
}

function evidence(
  plugin: ActionPlugin,
  startedAtMs: number,
  completedAtMs: number,
  status: ActionStatus,
  code: string,
  summary: unknown,
  details: UnknownRecord = {},
): ActionEvidence {
  const safeCompletedAtMs = Math.max(startedAtMs, completedAtMs);
  return {
    schemaVersion: ACTION_EVIDENCE_SCHEMA_VERSION,
    action: { id: plugin.id, version: plugin.version },
    status,
    type: `${plugin.category}-evidence`,
    code,
    phase: 'execute',
    summary: summary == null ? null : String(summary).slice(0, 2_000),
    finding: isFinding(details.finding) ? details.finding : null,
    observation: details.observation ?? null,
    expected: details.expected ?? null,
    retryable: details.retryable === true,
    timing: { startedAtMs, completedAtMs: safeCompletedAtMs,
      durationMs: safeCompletedAtMs - startedAtMs,
      deadlineMs: plugin.timeoutMs },
    attachments: [],
    sensitivity: [...plugin.sensitivity],
  };
}

function structuredValue(value: unknown, seen: Set<object> = new Set()): boolean {
  if (value === undefined || value === null || typeof value === 'string' || typeof value === 'boolean') return true;
  if (typeof value === 'number') return Number.isFinite(value);
  if (!object(value) && !Array.isArray(value)) return false;
  if (seen.has(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  if (!Array.isArray(value) && prototype !== Object.prototype && prototype !== null) return false;
  seen.add(value);
  const valid = (Array.isArray(value) ? value : Object.values(value))
    .every(item => structuredValue(item, seen));
  seen.delete(value);
  return valid;
}

export async function executeAction(
  registry: ActionRegistry,
  id: string,
  input: unknown,
  runContext: ActionRunContextInput,
  {
    now = evidenceNowMs,
    setTimer = setTimeout,
    clearTimer = clearTimeout,
  }: {
    now?: () => number;
    setTimer?: (callback: () => void, timeoutMs: number) => TimerHandle;
    clearTimer?: (timer: TimerHandle) => void;
  } = {},
): Promise<ActionEvidence> {
  const plugin = registry.get(id);
  const context = createActionRunContext(runContext);
  const startedAtMs = now();
  let compiled;
  try { compiled = plugin.compile(input); }
  catch (error: unknown) {
    return evidence(plugin, startedAtMs, now(), 'harness_failure', 'invalid_input',
      error instanceof Error ? error.message : String(error));
  }
  const missing = plugin.capabilities.filter(capability => !Object.hasOwn(context.capabilities, capability));
  if (missing.length) {
    return evidence(plugin, startedAtMs, now(), 'harness_failure', 'missing_capability',
      `missing action capabilities: ${missing.join(', ')}`, { retryable: false });
  }
  const capabilities = Object.freeze(Object.fromEntries(
    plugin.capabilities.map(capability => [capability, context.capabilities[capability]])));
  const controller = new AbortController();
  const termination: {
    current: { kind: 'cancelled' | 'deadline_exceeded'; reason: unknown } | null;
  } = { current: null };
  const abort = (kind: 'cancelled' | 'deadline_exceeded', reason: unknown): void => {
    if (termination.current) return;
    termination.current = { kind, reason };
    controller.abort(reason);
  };
  const onCancel = () => abort('cancelled', context.signal?.reason ?? 'action cancelled');
  if (context.signal?.aborted) onCancel();
  else context.signal?.addEventListener('abort', onCancel, { once: true });
  const timer = setTimer(() => abort('deadline_exceeded',
    `action exceeded ${plugin.timeoutMs}ms deadline`), plugin.timeoutMs);
  const stopped = new Promise<never>((_resolve, reject) => {
    if (termination.current) reject(new Error(String(termination.current.reason)));
    else controller.signal.addEventListener('abort', () => reject(new Error(
      String(termination.current?.reason ?? controller.signal.reason))),
      { once: true });
  });
  const execution = Promise.resolve().then(() => {
    if (controller.signal.aborted) throw controller.signal.reason;
    return plugin.execute({
      input: compiled,
      capabilities,
      signal: controller.signal,
    });
  });
  try {
    const observation = await Promise.race([execution, stopped]);
    if (!structuredValue(observation)) {
      return evidence(plugin, startedAtMs, now(), 'harness_failure', 'invalid_evidence',
        `${id} returned non-serializable evidence`);
    }
    return evidence(plugin, startedAtMs, now(), 'passed', 'completed', null, { observation });
  } catch (error: unknown) {
    // Do not start the next action while a timed-out implementation still runs.
    // Every implementation has a bounded operation or observes this signal.
    if (termination.current) await execution.catch(() => undefined);
    if (termination.current?.kind === 'cancelled') {
      return evidence(plugin, startedAtMs, now(), 'inconclusive', 'cancelled',
        String(termination.current.reason), { retryable: true });
    }
    if (termination.current?.kind === 'deadline_exceeded') {
      return evidence(plugin, startedAtMs, now(), 'harness_failure', 'deadline_exceeded',
        String(termination.current.reason), { retryable: true });
    }
    if (error instanceof ActionApplicationFailure) {
      if (!structuredValue(error.details?.observation) || !structuredValue(error.details?.expected)) {
        return evidence(plugin, startedAtMs, now(), 'harness_failure', 'invalid_evidence',
          `${id} returned non-serializable failure evidence`);
      }
      return evidence(plugin, startedAtMs, now(), 'failed', 'application_failure', error.message, error.details);
    }
    if (error instanceof ActionInconclusive) {
      if (!structuredValue(error.details?.observation) || !structuredValue(error.details?.expected)) {
        return evidence(plugin, startedAtMs, now(), 'harness_failure', 'invalid_evidence',
          `${id} returned non-serializable inconclusive evidence`);
      }
      return evidence(plugin, startedAtMs, now(), 'inconclusive', 'inconclusive',
        error.message, error.details);
    }
    return evidence(plugin, startedAtMs, now(), 'harness_failure', 'unclassified_exception',
      error instanceof Error ? error.message : String(error), { retryable: false });
  } finally {
    clearTimer(timer);
    context.signal?.removeEventListener('abort', onCancel);
  }
}
