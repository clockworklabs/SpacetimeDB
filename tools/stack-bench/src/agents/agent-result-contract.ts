import { pricingRatesEqual, validatePricingAuthority }
  from '../evidence/pricing-authority.js';
import type { PricingRates } from '../evidence/pricing-authority.js';
import type { AgentMode, AgentRequest } from './agent-adapter-contract.js';

export const AGENT_COST_RECEIPT_TOLERANCE_USD = 0.0001;

type UnknownRecord = Record<string, unknown>;

interface ReceiptUsage {
  input: number;
  output: number;
  cacheWrite5m: number;
  cacheWrite1h: number;
  cacheRead: number;
}

export interface AgentCostReceipt {
  schemaVersion: 2;
  source: 'credential-broker';
  model: string;
  maxBudgetUsd: number;
  costUsd: number;
  cliCostUsd: number | null;
  calculatedCostUsd: number | null;
  usage: ReceiptUsage | null;
  pricingRates: PricingRates | null;
  complete: boolean;
  reconciled: boolean;
  error: string | null;
}

export interface AgentCostReceiptEntry {
  invocation: number;
  receipt: AgentCostReceipt;
}

export interface AgentSessionFailure {
  kind: 'provider_failure' | 'harness_failure';
  phase: 'coding-session';
  reason: string;
  provider: unknown;
  appFailures: [];
  inconclusive: [];
  harnessFailures: [];
}

export interface AgentUsage {
  input: number;
  output: number;
  cacheWrite: number;
  cacheRead: number;
}

export interface AgentTranscriptIdentity {
  kind: string;
  id: string;
}

export interface AgentThinking {
  blocks?: number | null;
  signatureBytes?: number | null;
}

export interface AgentSetup extends UnknownRecord {
  session?: string;
  isolation?: UnknownRecord & { imageId?: string | null };
  providerThrottle?: { waits?: number | null; waitedMs?: number | null };
}

export interface ValidatedAgentResult {
  appDir: string;
  mode: AgentMode;
  level: number;
  backend: string;
  track: string;
  model: string;
  guidance: unknown;
  stack?: unknown;
  ok: boolean;
  sessionId: string | null;
  setup: AgentSetup;
  costUsd: number;
  tokens: number;
  outputTokens: number;
  turns: number;
  promptBytes: number;
  tokensPerTurn?: number | null;
  durationMs: number;
  usage: AgentUsage;
  costReceipts: AgentCostReceiptEntry[];
  costComplete: boolean;
  transcript: AgentTranscriptIdentity | null;
  thinking?: AgentThinking | null;
  provenance?: UnknownRecord | null;
  providerMetadata?: UnknownRecord | null;
}

const RESULT_FIELDS = new Set(['appDir', 'mode', 'level', 'track', 'backend', 'model', 'guidance',
  'stack', 'setup', 'costUsd', 'tokens', 'outputTokens', 'usage', 'provenance', 'turns',
  'promptBytes', 'tokensPerTurn', 'thinking', 'durationMs', 'sessionId', 'ok',
  'providerMetadata', 'transcript', 'costReceipts']);
const RECEIPT_FIELDS = new Set(['schemaVersion', 'source', 'model', 'maxBudgetUsd', 'costUsd',
  'cliCostUsd', 'calculatedCostUsd', 'usage', 'pricingRates', 'complete', 'reconciled', 'error']);
const RECEIPT_USAGE_FIELDS = ['input', 'output', 'cacheWrite5m', 'cacheWrite1h', 'cacheRead'];
const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const nonNegative = (value: unknown): value is number => typeof value === 'number'
  && Number.isFinite(value) && value >= 0;

export function validateAgentCostReceipt(value: unknown, model: string, at: string): AgentCostReceipt {
  if (!object(value)
    || Object.keys(value).some(key => !RECEIPT_FIELDS.has(key))
    || value.schemaVersion !== 2 || value.source !== 'credential-broker' || value.model !== model
    || !nonNegative(value.maxBudgetUsd) || value.maxBudgetUsd === 0
    || !nonNegative(value.costUsd)
    || ![value.cliCostUsd, value.calculatedCostUsd]
      .every(cost => cost === null || nonNegative(cost))
    || typeof value.complete !== 'boolean' || typeof value.reconciled !== 'boolean'
    || (value.error !== null && (typeof value.error !== 'string' || !value.error))
    || value.reconciled !== (value.error === null)) {
    throw new Error(`${at} is invalid`);
  }
  for (const field of ['usage', 'pricingRates'] as const) {
    const values = value[field];
    if (values === null) continue;
    if (!object(values) || Object.keys(values).some(key => !RECEIPT_USAGE_FIELDS.includes(key))
      || RECEIPT_USAGE_FIELDS.some(key => !nonNegative(values[key]))) {
      throw new Error(`${at}.${field} is invalid`);
    }
  }
  if (value.reconciled && (!value.complete || value.error !== null
    || value.usage === null || value.pricingRates === null
    || value.cliCostUsd === null || value.calculatedCostUsd === null)) {
    throw new Error(`${at} is incomplete`);
  }
  return value as unknown as AgentCostReceipt;
}

function validateCostReceipts(value: unknown, model: string): AgentCostReceiptEntry[] {
  if (!Array.isArray(value)) throw new Error('agent result costReceipts must be an array');
  return value.map((entry, index) => {
    if (!object(entry) || !Number.isSafeInteger(entry.invocation) || entry.invocation !== index + 1) {
      throw new Error(`agent result costReceipts[${index}] is invalid`);
    }
    return { invocation: entry.invocation as number,
      receipt: validateAgentCostReceipt(entry.receipt, model,
        `agent result costReceipts[${index}].receipt`) };
  });
}

const finite = (value: unknown, at: string): number => {
  if (!nonNegative(value)) throw new Error(`agent result ${at} must be a non-negative number`);
  return value;
};

const optionalFinite = (value: unknown, at: string): number | null | undefined => {
  if (value === undefined || value === null) return value;
  return finite(value, at);
};

export function validateAgentResult(value: unknown, request: AgentRequest): ValidatedAgentResult {
  if (!object(value)) throw new Error('agent result must be an object');
  for (const key of Object.keys(value)) {
    if (!RESULT_FIELDS.has(key)) throw new Error(`agent result ${key} is unknown`);
  }
  if (value.appDir !== request.app) throw new Error('agent result appDir does not match the request');
  if (value.mode !== request.mode) throw new Error('agent result mode does not match the request');
  if (value.level !== request.level) throw new Error('agent result level does not match the request');
  if (value.backend !== undefined && value.backend !== request.backend) {
    throw new Error('agent result backend does not match the request');
  }
  if (value.track !== undefined && value.track !== request.track) {
    throw new Error('agent result track does not match the request');
  }
  if (value.model !== undefined && value.model !== request.model) {
    throw new Error('agent result model does not match the request');
  }
  if (typeof value.ok !== 'boolean') throw new Error('agent result ok must be boolean');
  if (value.sessionId !== null && (typeof value.sessionId !== 'string' || !value.sessionId)) {
    throw new Error('agent result sessionId must be a non-empty string or null');
  }
  if (!object(value.usage)) throw new Error('agent result usage must be an object');
  const resultUsage = value.usage;
  for (const key of Object.keys(resultUsage)) {
    if (!['input', 'output', 'cacheWrite', 'cacheRead'].includes(key)) {
      throw new Error(`agent result usage.${key} is unknown`);
    }
  }
  if (!object(value.setup)) throw new Error('agent result setup must be an object');
  const setup: AgentSetup = value.setup;
  if (setup.session !== undefined && (typeof setup.session !== 'string' || !setup.session)) {
    throw new Error('agent result setup.session must be a non-empty string when present');
  }
  if (setup.isolation !== undefined) {
    if (!object(setup.isolation)) throw new Error('agent result setup.isolation must be an object');
    if (setup.isolation.imageId !== undefined && setup.isolation.imageId !== null
      && (typeof setup.isolation.imageId !== 'string' || !setup.isolation.imageId)) {
      throw new Error('agent result setup.isolation.imageId must be a non-empty string or null');
    }
  }
  if (setup.providerThrottle !== undefined) {
    if (!object(setup.providerThrottle)) {
      throw new Error('agent result setup.providerThrottle must be an object');
    }
    for (const key of ['waits', 'waitedMs'] as const) {
      const metric = setup.providerThrottle[key];
      if (metric !== undefined && metric !== null && !nonNegative(metric)) {
        throw new Error(`agent result setup.providerThrottle.${key} must be non-negative`);
      }
    }
  }
  let thinking: AgentThinking | null | undefined;
  if (value.thinking === null || value.thinking === undefined) {
    thinking = value.thinking;
  } else {
    if (!object(value.thinking)) throw new Error('agent result thinking must be an object or null');
    for (const key of ['blocks', 'signatureBytes'] as const) {
      const metric = value.thinking[key];
      if (metric !== undefined && metric !== null && !nonNegative(metric)) {
        throw new Error(`agent result thinking.${key} must be non-negative`);
      }
    }
    thinking = {
      blocks: optionalFinite(value.thinking.blocks, 'thinking.blocks'),
      signatureBytes: optionalFinite(value.thinking.signatureBytes, 'thinking.signatureBytes'),
    };
  }
  if (value.provenance !== undefined && value.provenance !== null && !object(value.provenance)) {
    throw new Error('agent result provenance must be an object or null');
  }
  if (value.providerMetadata !== undefined && value.providerMetadata !== null
    && !object(value.providerMetadata)) throw new Error('agent result providerMetadata must be an object or null');
  if (value.transcript !== undefined && value.transcript !== null) {
    if (!object(value.transcript)) throw new Error('agent result transcript must be an object or null');
    for (const key of Object.keys(value.transcript)) {
      if (!['kind', 'id'].includes(key)) throw new Error(`agent result transcript.${key} is unknown`);
    }
    if (typeof value.transcript.kind !== 'string' || !value.transcript.kind
      || typeof value.transcript.id !== 'string' || !value.transcript.id) {
      throw new Error('agent result transcript requires non-empty kind and id');
    }
  }
  const usage: AgentUsage = {
    input: finite(resultUsage.input, 'usage.input'),
    output: finite(resultUsage.output, 'usage.output'),
    cacheWrite: finite(resultUsage.cacheWrite, 'usage.cacheWrite'),
    cacheRead: finite(resultUsage.cacheRead, 'usage.cacheRead'),
  };
  const costUsd = finite(value.costUsd, 'costUsd');
  const costReceipts = validateCostReceipts(
    value.costReceipts === undefined ? [] : value.costReceipts, request.model);
  const pricing = request.pricing == null ? null
    : validatePricingAuthority(request.pricing, { at: 'agent request pricing' });
  const cappedNative = request.adapterCostLimit === 'native' && request.maxBudgetUsd != null;
  if (cappedNative && pricing === null) {
    throw new Error('agent request pricing is required for a native cost limit');
  }
  const receiptCostUsd = costReceipts.reduce((sum, { receipt }) => sum + receipt.costUsd, 0);
  const costComplete = request.adapterCostLimit === 'non-billable'
    || (cappedNative && costReceipts.length > 0
      && costReceipts.every(({ receipt }) => receipt.complete && receipt.reconciled
        && receipt.error === null && pricingRatesEqual(receipt.pricingRates, pricing?.rates))
      && Math.abs(receiptCostUsd - costUsd) <= AGENT_COST_RECEIPT_TOLERANCE_USD);
  if (value.ok && cappedNative && !costComplete) {
    throw new Error('successful agent result requires complete reconciled broker cost proof');
  }
  if (request.maxBudgetUsd != null && request.adapterCostLimit === 'unsupported') {
    throw new Error('agent result came from an adapter that cannot enforce a cost limit');
  }
  const transcript = value.transcript === null || value.transcript === undefined
    ? (value.sessionId ? { kind: 'provider-session', id: value.sessionId as string } : null)
    : value.transcript as unknown as AgentTranscriptIdentity;
  return {
    appDir: request.app,
    mode: request.mode,
    level: request.level,
    backend: request.backend,
    track: request.track,
    model: request.model,
    guidance: value.guidance ?? request.guidance,
    stack: value.stack,
    ok: value.ok,
    sessionId: value.sessionId,
    setup,
    thinking,
    costUsd,
    tokens: finite(value.tokens, 'tokens'),
    outputTokens: finite(value.outputTokens, 'outputTokens'),
    turns: finite(value.turns, 'turns'),
    promptBytes: finite(value.promptBytes, 'promptBytes'),
    tokensPerTurn: optionalFinite(value.tokensPerTurn, 'tokensPerTurn'),
    durationMs: finite(value.durationMs, 'durationMs'),
    usage,
    costReceipts,
    costComplete,
    transcript,
    provenance: value.provenance as UnknownRecord | null | undefined,
    providerMetadata: value.providerMetadata as UnknownRecord | null | undefined,
  };
}

export function agentSessionFailure(value: unknown): AgentSessionFailure | null {
  if (!object(value)) throw new Error('agent session result must be an object');
  const result = value;
  if (result.ok === true && result.sessionId) return null;
  const providerMetadata = object(result.providerMetadata) ? result.providerMetadata : null;
  const failureCode = providerMetadata?.failureCode;
  const kind = typeof failureCode === 'string' && failureCode.startsWith('provider-')
    ? 'provider_failure' : 'harness_failure';
  return { kind, phase: 'coding-session',
    reason: typeof failureCode === 'string' && failureCode ? failureCode
      : result.sessionId ? 'coding session reported failure' : 'coding session did not run',
    provider: providerMetadata?.failure ?? null,
    appFailures: [], inconclusive: [], harnessFailures: [] };
}
