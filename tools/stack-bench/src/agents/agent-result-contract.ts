import { pricingRatesEqual, validatePricingAuthority }
  from '../evidence/pricing-authority.js';
import type { PricingRates } from '../evidence/pricing-authority.js';
import type { ClaudeUsage } from '../evidence/claude-usage-cost.js';
import { formatZodError } from '../zod-error.js';
import type { AgentMode, AgentRequest } from './agent-adapter-contract.js';
import { z } from 'zod';

export const AGENT_COST_RECEIPT_TOLERANCE_USD = 0.0001;

type UnknownRecord = Record<string, unknown>;

export interface AgentCostReceipt {
  schemaVersion: 2;
  source: 'credential-broker';
  model: string;
  maxBudgetUsd: number;
  costUsd: number;
  cliCostUsd: number | null;
  calculatedCostUsd: number | null;
  usage: ClaudeUsage | null;
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
  resources?: {
    buildContainerMemory: {
      currentBytes: number | null;
      peakBytes: number | null;
      limitBytes: number | null;
    } | null;
    memoryProbeError: string | null;
  } | null;
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

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const nonNegativeNumber = z.number().finite().nonnegative();
const receiptUsageSchema = z.strictObject({
  input: nonNegativeNumber,
  output: nonNegativeNumber,
  cacheWrite5m: nonNegativeNumber,
  cacheWrite1h: nonNegativeNumber,
  cacheRead: nonNegativeNumber,
});
const receiptSchema = z.strictObject({
  schemaVersion: z.literal(2),
  source: z.literal('credential-broker'),
  model: z.string().min(1),
  maxBudgetUsd: z.number().finite().positive(),
  costUsd: nonNegativeNumber,
  cliCostUsd: nonNegativeNumber.nullable(),
  calculatedCostUsd: nonNegativeNumber.nullable(),
  usage: receiptUsageSchema.nullable(),
  pricingRates: receiptUsageSchema.nullable(),
  complete: z.boolean(),
  reconciled: z.boolean(),
  error: z.string().min(1).nullable(),
});
const resultSchema = z.strictObject({
  appDir: z.string(),
  mode: z.enum(['build', 'upgrade', 'resume', 'fix']),
  level: z.number().int(),
  track: z.string().optional(),
  backend: z.string().optional(),
  model: z.string().optional(),
  guidance: z.unknown().optional(),
  stack: z.unknown().optional(),
  setup: z.looseObject({
    session: z.string().min(1).optional(),
    isolation: z.looseObject({ imageId: z.string().min(1).nullable().optional() }).optional(),
    providerThrottle: z.looseObject({
      waits: nonNegativeNumber.nullable().optional(),
      waitedMs: nonNegativeNumber.nullable().optional(),
    }).optional(),
    resources: z.strictObject({
      buildContainerMemory: z.strictObject({
        currentBytes: nonNegativeNumber.nullable(),
        peakBytes: nonNegativeNumber.nullable(),
        limitBytes: nonNegativeNumber.nullable(),
      }).nullable(),
      memoryProbeError: z.string().min(1).nullable(),
    }).nullable().optional(),
  }),
  costUsd: nonNegativeNumber,
  tokens: nonNegativeNumber,
  outputTokens: nonNegativeNumber,
  usage: z.strictObject({
    input: nonNegativeNumber,
    output: nonNegativeNumber,
    cacheWrite: nonNegativeNumber,
    cacheRead: nonNegativeNumber,
  }),
  provenance: z.record(z.string(), z.unknown()).nullable().optional(),
  turns: nonNegativeNumber,
  promptBytes: nonNegativeNumber,
  tokensPerTurn: nonNegativeNumber.nullable().optional(),
  thinking: z.looseObject({
    blocks: nonNegativeNumber.nullable().optional(),
    signatureBytes: nonNegativeNumber.nullable().optional(),
  }).nullable().optional(),
  durationMs: nonNegativeNumber,
  sessionId: z.string().min(1).nullable(),
  ok: z.boolean(),
  providerMetadata: z.record(z.string(), z.unknown()).nullable().optional(),
  transcript: z.strictObject({ kind: z.string().min(1), id: z.string().min(1) }).nullable().optional(),
  costReceipts: z.array(z.strictObject({ invocation: z.number().int().positive(),
    receipt: z.unknown() })).optional(),
});

export function validateAgentCostReceipt(value: unknown, model: string, at: string): AgentCostReceipt {
  const parsed = receiptSchema.safeParse(value);
  if (!parsed.success) throw new Error(`${at} is invalid: ${formatZodError(parsed.error, at)}`);
  const receipt = parsed.data;
  if (receipt.model !== model || receipt.reconciled !== (receipt.error === null)) {
    throw new Error(`${at} is invalid`);
  }
  if (receipt.reconciled && (!receipt.complete || receipt.usage === null
    || receipt.pricingRates === null || receipt.cliCostUsd === null
    || receipt.calculatedCostUsd === null)) {
    throw new Error(`${at} is incomplete`);
  }
  return receipt;
}

function validateCostReceipts(value: unknown, model: string): AgentCostReceiptEntry[] {
  if (!Array.isArray(value)) throw new Error('agent result costReceipts must be an array');
  return value.map((entry, index) => {
    const parsed = z.strictObject({ invocation: z.number().int().positive(),
      receipt: z.unknown() }).safeParse(entry);
    if (!parsed.success || parsed.data.invocation !== index + 1) {
      throw new Error(`agent result costReceipts[${index}] is invalid`);
    }
    return { invocation: parsed.data.invocation,
      receipt: validateAgentCostReceipt(parsed.data.receipt, model,
        `agent result costReceipts[${index}].receipt`) };
  });
}

export function validateAgentResult(value: unknown, request: AgentRequest): ValidatedAgentResult {
  const parsed = resultSchema.safeParse(value);
  if (!parsed.success) {
    throw new Error(formatZodError(parsed.error, 'agent result'));
  }
  const result = parsed.data;
  if (result.appDir !== request.app) throw new Error('agent result appDir does not match the request');
  if (result.mode !== request.mode) throw new Error('agent result mode does not match the request');
  if (result.level !== request.level) throw new Error('agent result level does not match the request');
  if (result.backend !== undefined && result.backend !== request.backend) {
    throw new Error('agent result backend does not match the request');
  }
  if (result.track !== undefined && result.track !== request.track) {
    throw new Error('agent result track does not match the request');
  }
  if (result.model !== undefined && result.model !== request.model) {
    throw new Error('agent result model does not match the request');
  }
  const setup: AgentSetup = result.setup;
  const thinking = result.thinking;
  const usage: AgentUsage = result.usage;
  const costUsd = result.costUsd;
  const costReceipts = validateCostReceipts(
    result.costReceipts === undefined ? [] : result.costReceipts, request.model);
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
  if (result.ok && cappedNative && !costComplete) {
    throw new Error('successful agent result requires complete reconciled broker cost proof');
  }
  if (request.maxBudgetUsd != null && request.adapterCostLimit === 'unsupported') {
    throw new Error('agent result came from an adapter that cannot enforce a cost limit');
  }
  const transcript = result.transcript === null || result.transcript === undefined
    ? (result.sessionId ? { kind: 'provider-session', id: result.sessionId } : null)
    : result.transcript;
  return {
    appDir: request.app,
    mode: request.mode,
    level: request.level,
    backend: request.backend,
    track: request.track,
    model: request.model,
    guidance: result.guidance ?? request.guidance,
    stack: result.stack,
    ok: result.ok,
    sessionId: result.sessionId,
    setup,
    thinking,
    costUsd,
    tokens: result.tokens,
    outputTokens: result.outputTokens,
    turns: result.turns,
    promptBytes: result.promptBytes,
    tokensPerTurn: result.tokensPerTurn,
    durationMs: result.durationMs,
    usage,
    costReceipts,
    costComplete,
    transcript,
    provenance: result.provenance,
    providerMetadata: result.providerMetadata,
  };
}

export function agentSessionFailure(value: unknown): AgentSessionFailure | null {
  if (!object(value)) throw new Error('agent session result must be an object');
  const result = value;
  if (result.ok === true && result.sessionId) return null;
  const providerMetadata = object(result.providerMetadata) ? result.providerMetadata : null;
  const failureCode = providerMetadata?.failureCode;
  const diagnostic = providerMetadata?.diagnostic;
  const kind = typeof failureCode === 'string' && failureCode.startsWith('provider-')
    ? 'provider_failure' : 'harness_failure';
  return { kind, phase: 'coding-session',
    reason: typeof diagnostic === 'string' && diagnostic ? diagnostic
      : typeof failureCode === 'string' && failureCode ? failureCode
      : result.sessionId ? 'coding session reported failure' : 'coding session did not run',
    provider: providerMetadata?.failure ?? null,
    appFailures: [], inconclusive: [], harnessFailures: [] };
}
