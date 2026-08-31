import { evidenceNowMs } from '../evidence/evidence-timing.js';

type UnknownRecord = Record<string, unknown>;
type ActionStatus = 'passed' | 'failed' | 'inconclusive' | 'harness_failure';
type TimerHandle = ReturnType<typeof setTimeout>;

export interface ActionAttempt {
  readonly id: string;
  readonly parentId?: string | null;
}

export interface ActionImplementationArguments {
  readonly input: unknown;
  readonly capabilities: Readonly<Record<string, unknown>>;
  readonly signal: AbortSignal;
  readonly attempt: ActionAttempt | null;
}

export type ActionImplementation = (
  arguments_: ActionImplementationArguments,
) => unknown | Promise<unknown>;

export interface ActionExecutionArguments extends ActionImplementationArguments {
  readonly implementation: ActionImplementation;
}

export interface ActionPlugin {
  readonly schemaVersion: number;
  readonly id: string;
  readonly version: string;
  readonly input: {
    readonly schemaVersion: number;
    readonly compile: (input: unknown) => unknown;
  };
  readonly capabilities: readonly string[];
  readonly deadline: { readonly timeoutMs: number };
  readonly evidence: {
    readonly schemaVersion: number;
    readonly type: string;
    readonly validate: (value: unknown) => boolean;
  };
  readonly redaction: {
    readonly sensitivity: readonly string[];
    readonly fields: readonly string[];
  };
  readonly renderer: { readonly label: string; readonly category: string };
  readonly execute: (arguments_: ActionExecutionArguments) => unknown | Promise<unknown>;
}

export interface ActionRegistry {
  readonly ids: readonly string[];
  get(id: string): ActionPlugin;
}

export interface ActionRunContext {
  readonly capabilities: Readonly<Record<string, unknown>>;
  readonly implementations: Readonly<Record<string, unknown>>;
  readonly signal: AbortSignal | null;
  readonly attempt: Readonly<ActionAttempt> | null;
}

export interface ActionEvidence {
  readonly schemaVersion: number;
  readonly action: { readonly id: string; readonly version: string };
  readonly status: ActionStatus;
  readonly type: string;
  readonly code: string;
  readonly phase: 'execute';
  readonly summary: string | null;
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

export const ACTION_PLUGIN_SCHEMA_VERSION = 1;
export const ACTION_INPUT_SCHEMA_VERSION = 1;
export const ACTION_EVIDENCE_SCHEMA_VERSION = 1;

const PLUGIN_FIELDS = new Set([
  'schemaVersion', 'id', 'version', 'input', 'capabilities', 'deadline', 'evidence',
  'redaction', 'renderer', 'execute',
]);
const INPUT_FIELDS = new Set(['schemaVersion', 'compile']);
const DEADLINE_FIELDS = new Set(['timeoutMs']);
const EVIDENCE_FIELDS = new Set(['schemaVersion', 'type', 'validate']);
const REDACTION_FIELDS = new Set(['sensitivity', 'fields']);
const RENDERER_FIELDS = new Set(['label', 'category']);
const CONTEXT_FIELDS = new Set(['capabilities', 'implementations', 'signal', 'attempt']);
const ATTEMPT_FIELDS = new Set(['id', 'parentId']);
const ID = /^[a-z][a-zA-Z0-9]*(?:[.:-][a-zA-Z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const nonEmpty = (value: unknown): value is string =>
  typeof value === 'string' && value.trim().length > 0;
const positiveInteger = (value: unknown): value is number =>
  typeof value === 'number' && Number.isInteger(value) && value > 0;

function strict(value: unknown, fields: ReadonlySet<string>, at: string): asserts value is UnknownRecord {
  if (!object(value)) throw new Error(`${at} must be an object`);
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) throw new Error(`${at}.${key} is unknown`);
  }
}

function stringList(value: unknown, at: string): readonly string[] {
  if (!Array.isArray(value) || !value.every(nonEmpty)) throw new Error(`${at} must be a string array`);
  const unique = new Set(value);
  if (unique.size !== value.length) throw new Error(`${at} contains duplicates`);
  return Object.freeze([...value]);
}

export function defineActionPlugin(value: unknown): ActionPlugin {
  strict(value, PLUGIN_FIELDS, 'action plugin');
  if (value.schemaVersion !== ACTION_PLUGIN_SCHEMA_VERSION) {
    throw new Error(`action plugin ${value.id ?? '<unknown>'} uses unsupported schema ${value.schemaVersion}`);
  }
  if (!nonEmpty(value.id) || !ID.test(value.id)) throw new Error('action plugin id is invalid');
  if (!nonEmpty(value.version) || !VERSION.test(value.version)) {
    throw new Error(`action plugin ${value.id} version is invalid`);
  }
  strict(value.input, INPUT_FIELDS, `${value.id}.input`);
  if (value.input.schemaVersion !== ACTION_INPUT_SCHEMA_VERSION
    || typeof value.input.compile !== 'function') {
    throw new Error(`${value.id}.input must declare schema ${ACTION_INPUT_SCHEMA_VERSION} and compile()`);
  }
  strict(value.deadline, DEADLINE_FIELDS, `${value.id}.deadline`);
  if (!positiveInteger(value.deadline.timeoutMs)) {
    throw new Error(`${value.id}.deadline.timeoutMs must be a positive integer`);
  }
  strict(value.evidence, EVIDENCE_FIELDS, `${value.id}.evidence`);
  if (value.evidence.schemaVersion !== ACTION_EVIDENCE_SCHEMA_VERSION || !nonEmpty(value.evidence.type)
    || typeof value.evidence.validate !== 'function') {
    throw new Error(`${value.id}.evidence must declare schema ${ACTION_EVIDENCE_SCHEMA_VERSION}, type, and validate()`);
  }
  strict(value.redaction, REDACTION_FIELDS, `${value.id}.redaction`);
  strict(value.renderer, RENDERER_FIELDS, `${value.id}.renderer`);
  if (!nonEmpty(value.renderer.label) || !nonEmpty(value.renderer.category)) {
    throw new Error(`${value.id}.renderer requires label and category`);
  }
  if (typeof value.execute !== 'function') throw new Error(`${value.id}.execute must be a function`);
  return Object.freeze({
    schemaVersion: ACTION_PLUGIN_SCHEMA_VERSION,
    id: value.id,
    version: value.version,
    input: Object.freeze({
      schemaVersion: ACTION_INPUT_SCHEMA_VERSION,
      compile: value.input.compile as ActionPlugin['input']['compile'],
    }),
    capabilities: stringList(value.capabilities, `${value.id}.capabilities`),
    deadline: Object.freeze({ timeoutMs: value.deadline.timeoutMs }),
    evidence: Object.freeze({
      schemaVersion: ACTION_EVIDENCE_SCHEMA_VERSION,
      type: value.evidence.type,
      validate: value.evidence.validate as ActionPlugin['evidence']['validate'],
    }),
    redaction: Object.freeze({
      sensitivity: stringList(value.redaction.sensitivity, `${value.id}.redaction.sensitivity`),
      fields: stringList(value.redaction.fields, `${value.id}.redaction.fields`),
    }),
    renderer: Object.freeze({ label: value.renderer.label, category: value.renderer.category }),
    execute: value.execute as ActionPlugin['execute'],
  });
}

export function createActionRegistry(
  plugins: readonly unknown[],
  { expectedIds = null }: { expectedIds?: readonly string[] | null } = {},
): ActionRegistry {
  if (!Array.isArray(plugins)) throw new Error('action registry plugins must be an array');
  const entries = new Map();
  for (const source of plugins) {
    const plugin = defineActionPlugin(source);
    if (entries.has(plugin.id)) throw new Error(`duplicate action registration ${plugin.id}`);
    entries.set(plugin.id, plugin);
  }
  if (expectedIds !== null) {
    const expected = new Set(stringList(expectedIds, 'action registry expectedIds'));
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

export function createActionRunContext(value: unknown = {}): ActionRunContext {
  strict(value, CONTEXT_FIELDS, 'action run context');
  const capabilities = value.capabilities ?? {};
  const implementations = value.implementations ?? {};
  if (!object(capabilities)) throw new Error('action run context capabilities must be an object');
  if (!object(implementations)) throw new Error('action run context implementations must be an object');
  if (value.signal != null && !(value.signal instanceof AbortSignal)) {
    throw new Error('action run context signal must be an AbortSignal');
  }
  if (value.attempt !== undefined && value.attempt !== null && !object(value.attempt)) {
    throw new Error('action run context attempt must be an object or null');
  }
  if (value.attempt) {
    strict(value.attempt, ATTEMPT_FIELDS, 'action run context attempt');
    if (!nonEmpty(value.attempt.id)) throw new Error('action run context attempt.id is required');
    if (value.attempt.parentId != null && !nonEmpty(value.attempt.parentId)) {
      throw new Error('action run context attempt.parentId must be a string or null');
    }
  }
  const attempt = value.attempt == null ? null : Object.freeze({
    id: value.attempt.id as string,
    ...(value.attempt.parentId === undefined ? {} : { parentId: value.attempt.parentId as string | null }),
  });
  return Object.freeze({ capabilities: Object.freeze({ ...capabilities }),
    implementations: Object.freeze({ ...implementations }), signal: value.signal ?? null,
    attempt });
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
    type: plugin.evidence.type,
    code,
    phase: 'execute',
    summary: summary == null ? null : String(summary).slice(0, 2_000),
    observation: details.observation ?? null,
    expected: details.expected ?? null,
    retryable: details.retryable === true,
    timing: { startedAtMs, completedAtMs: safeCompletedAtMs,
      durationMs: safeCompletedAtMs - startedAtMs,
      deadlineMs: plugin.deadline.timeoutMs },
    attachments: [],
    sensitivity: [...plugin.redaction.sensitivity],
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
  runContext: unknown,
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
  try { compiled = plugin.input.compile(input); }
  catch (error: unknown) {
    return evidence(plugin, startedAtMs, now(), 'harness_failure', 'invalid_input',
      error instanceof Error ? error.message : String(error));
  }
  const missing = plugin.capabilities.filter(capability => !Object.hasOwn(context.capabilities, capability));
  if (missing.length) {
    return evidence(plugin, startedAtMs, now(), 'inconclusive', 'missing_capability',
      `missing action capabilities: ${missing.join(', ')}`, { retryable: false });
  }
  const implementation = context.implementations[id];
  if (typeof implementation !== 'function') {
    return evidence(plugin, startedAtMs, now(), 'harness_failure', 'missing_implementation',
      `no implementation registered for ${id}`);
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
    `action exceeded ${plugin.deadline.timeoutMs}ms deadline`), plugin.deadline.timeoutMs);
  const stopped = new Promise<never>((_resolve, reject) => {
    if (termination.current) reject(new Error(String(termination.current.reason)));
    else controller.signal.addEventListener('abort', () => reject(new Error(
      String(termination.current?.reason ?? controller.signal.reason))),
      { once: true });
  });
  try {
    const observation = await Promise.race([
      plugin.execute({ input: compiled, capabilities, signal: controller.signal,
        implementation: implementation as ActionImplementation, attempt: context.attempt }),
      stopped,
    ]);
    let valid = structuredValue(observation);
    try { valid = valid && plugin.evidence.validate(observation) === true; }
    catch { valid = false; }
    if (!valid) {
      return evidence(plugin, startedAtMs, now(), 'harness_failure', 'invalid_evidence',
        `${id} returned evidence that does not match ${plugin.evidence.type}`);
    }
    return evidence(plugin, startedAtMs, now(), 'passed', 'completed', null, { observation });
  } catch (error: unknown) {
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
