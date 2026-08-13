import { evidenceNowMs } from './evidence-timing.mjs';

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

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const nonEmpty = value => typeof value === 'string' && value.trim().length > 0;

function strict(value, fields, at) {
  if (!object(value)) throw new Error(`${at} must be an object`);
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) throw new Error(`${at}.${key} is unknown`);
  }
}

function stringList(value, at) {
  if (!Array.isArray(value) || !value.every(nonEmpty)) throw new Error(`${at} must be a string array`);
  const unique = new Set(value);
  if (unique.size !== value.length) throw new Error(`${at} contains duplicates`);
  return Object.freeze([...value]);
}

export function defineActionPlugin(value) {
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
  if (!Number.isInteger(value.deadline.timeoutMs) || value.deadline.timeoutMs <= 0) {
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
    ...value,
    input: Object.freeze({ ...value.input }),
    capabilities: stringList(value.capabilities, `${value.id}.capabilities`),
    deadline: Object.freeze({ ...value.deadline }),
    evidence: Object.freeze({ ...value.evidence }),
    redaction: Object.freeze({
      sensitivity: stringList(value.redaction.sensitivity, `${value.id}.redaction.sensitivity`),
      fields: stringList(value.redaction.fields, `${value.id}.redaction.fields`),
    }),
    renderer: Object.freeze({ ...value.renderer }),
  });
}

export function createActionRegistry(plugins, { expectedIds = null } = {}) {
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
    get(id) {
      const plugin = entries.get(id);
      if (!plugin) throw new Error(`unknown registered action ${JSON.stringify(id)}`);
      return plugin;
    },
  });
}

export function createActionRunContext(value = {}) {
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
  return Object.freeze({ capabilities: Object.freeze({ ...capabilities }),
    implementations: Object.freeze({ ...implementations }), signal: value.signal ?? null,
    attempt: value.attempt == null ? null : Object.freeze({ ...value.attempt }) });
}

class ClassifiedActionError extends Error {
  constructor(classification, message, details = {}) {
    super(message);
    this.name = this.constructor.name;
    this.classification = classification;
    this.details = details;
  }
}

export class ActionApplicationFailure extends ClassifiedActionError {
  constructor(message, details) { super('application_failure', message, details); }
}

export class ActionInconclusive extends ClassifiedActionError {
  constructor(message, details) { super('inconclusive', message, details); }
}

function evidence(plugin, startedAtMs, completedAtMs, status, code, summary, details = {}) {
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
    retryable: details.retryable ?? false,
    timing: { startedAtMs, completedAtMs: safeCompletedAtMs,
      durationMs: safeCompletedAtMs - startedAtMs,
      deadlineMs: plugin.deadline.timeoutMs },
    attachments: [],
    sensitivity: [...plugin.redaction.sensitivity],
  };
}

function structuredValue(value, seen = new Set()) {
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

export async function executeAction(registry, id, input, runContext, {
  now = evidenceNowMs, setTimer = setTimeout, clearTimer = clearTimeout,
} = {}) {
  const plugin = registry.get(id);
  const context = createActionRunContext(runContext);
  const startedAtMs = now();
  let compiled;
  try { compiled = plugin.input.compile(input); }
  catch (error) {
    return evidence(plugin, startedAtMs, now(), 'harness_failure', 'invalid_input', error.message);
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
  let termination = null;
  const abort = (kind, reason) => {
    if (termination) return;
    termination = { kind, reason };
    controller.abort(reason);
  };
  const onCancel = () => abort('cancelled', context.signal?.reason ?? 'action cancelled');
  if (context.signal?.aborted) onCancel();
  else context.signal?.addEventListener('abort', onCancel, { once: true });
  const timer = setTimer(() => abort('deadline_exceeded',
    `action exceeded ${plugin.deadline.timeoutMs}ms deadline`), plugin.deadline.timeoutMs);
  const stopped = new Promise((_, reject) => {
    if (termination) reject(new Error(String(termination.reason)));
    else controller.signal.addEventListener('abort', () => reject(new Error(String(termination.reason))),
      { once: true });
  });
  try {
    const observation = await Promise.race([
      plugin.execute({ input: compiled, capabilities, signal: controller.signal,
        implementation, attempt: context.attempt }),
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
  } catch (error) {
    if (termination?.kind === 'cancelled') {
      return evidence(plugin, startedAtMs, now(), 'inconclusive', 'cancelled',
        String(termination.reason), { retryable: true });
    }
    if (termination?.kind === 'deadline_exceeded') {
      return evidence(plugin, startedAtMs, now(), 'harness_failure', 'deadline_exceeded',
        String(termination.reason), { retryable: true });
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
      String(error?.message ?? error), { retryable: false });
  } finally {
    clearTimer(timer);
    context.signal?.removeEventListener('abort', onCancel);
  }
}
