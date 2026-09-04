import { randomBytes } from 'node:crypto';
import { readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { z } from 'zod';

import { normalizeClaudeUsage, priceClaudeUsage } from '../src/evidence/claude-usage-cost.js';
import type { ClaudeUsage } from '../src/evidence/claude-usage-cost.js';
import { validatePricingRates as validateSharedPricingRates } from '../src/evidence/pricing-authority.js';
import { formatZodError } from '../src/zod-error.js';

export const BROKER_LEDGER_SCHEMA_VERSION = 4;
// Why a billable request was charged its cost ceiling instead of priced from
// the provider's usage: a 2xx response without complete usage (an aborted or
// errored stream, an oversized body), a response that broke off, or an
// upstream connection that failed. The ceiling makes spend an upper bound.
export const ESTIMATE_REASONS = ['no-usage', 'response-aborted', 'upstream-error'] as const;
export type EstimateReason = typeof ESTIMATE_REASONS[number];
export type EstimateCounts = Record<EstimateReason, number>;
export const noEstimates = (): EstimateCounts => ({ 'no-usage': 0, 'response-aborted': 0, 'upstream-error': 0 });
export const MAX_BROKER_OUTPUT_TOKENS = 128_000;
export const CLAUDE_USAGE_FIELDS = ['input', 'output', 'cacheRead', 'cacheWrite5m', 'cacheWrite1h'] as const;
const COST_TOLERANCE_USD = 0.0001;

type JsonRecord = Record<string, unknown>;
export type BrokerMode = 'api-key' | 'subscription-token';
export type PricingRates = ReturnType<typeof validateSharedPricingRates>;

export type BrokerConfig = {
  mode: BrokerMode;
  credential: string;
  sessionToken: string;
  readyPath?: string;
  parentPid?: number;
  expiresAt?: number;
  listenHost?: '127.0.0.1' | '0.0.0.0';
  ledgerPath?: string;
  model: string;
  maxOutputTokens: number;
  maxBudgetUsd?: number | null;
  pricingRates?: PricingRates;
};

export type BrokerLedger = {
  schemaVersion: number;
  model: string;
  maxBudgetUsd: number | null;
  acceptedRequests: number;
  billableRequests: number;
  completedBillableRequests: number;
  estimatedBillableRequests: number;
  estimatedByReason: EstimateCounts;
  spentUsd: number;
  reservedUsd: number;
  usage: ClaudeUsage;
  complete: boolean;
  updatedAt: string;
};

// `costUsd` is what the broker charged: exact provider usage priced at the
// receipt's rates, plus the cost ceiling of every estimated request. With
// `exact` false it is an upper bound and `calculatedCostUsd`, priced from the
// exact usage alone, a lower bound.
export interface CredentialBrokerReceipt {
  schemaVersion: 3;
  source: 'credential-broker';
  model: string;
  maxBudgetUsd: number;
  costUsd: number;
  cliCostUsd: number | null;
  calculatedCostUsd: number | null;
  usage: ClaudeUsage | null;
  pricingRates: PricingRates | null;
  exact: boolean;
  estimatedRequests: number;
  estimatedByReason: EstimateCounts;
  complete: boolean;
  reconciled: boolean;
  error: string | null;
}

export interface CredentialBrokerResult extends JsonRecord {
  total_cost_usd: number;
  usage?: ReturnType<typeof rawUsage>;
  stack_bench_cost_receipt: CredentialBrokerReceipt;
}

const positiveFinite = z.number().finite().positive();
const nonNegativeFinite = z.number().finite().nonnegative();
const nonNegativeSafeInteger = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
const usageSchema = z.strictObject({
  input: nonNegativeSafeInteger,
  output: nonNegativeSafeInteger,
  cacheRead: nonNegativeSafeInteger,
  cacheWrite5m: nonNegativeSafeInteger,
  cacheWrite1h: nonNegativeSafeInteger,
});
const brokerConfigSchema = z.strictObject({
  mode: z.enum(['api-key', 'subscription-token']),
  credential: z.string().min(16),
  sessionToken: z.string().min(16),
  readyPath: z.string().min(1).optional(),
  parentPid: z.number().int().positive().max(Number.MAX_SAFE_INTEGER).optional(),
  expiresAt: positiveFinite.optional(),
  listenHost: z.enum(['127.0.0.1', '0.0.0.0']).optional(),
  ledgerPath: z.string().min(1).optional(),
  model: z.string().min(1),
  maxOutputTokens: z.number().int().min(1).max(MAX_BROKER_OUTPUT_TOKENS),
  maxBudgetUsd: positiveFinite.nullable().optional(),
  pricingRates: z.unknown().optional(),
}).superRefine((value, context) => {
  if (value.expiresAt !== undefined && value.expiresAt <= Date.now()) {
    context.addIssue({ code: 'custom', path: ['expiresAt'], message: 'must be in the future' });
  }
});
const brokerLedgerSchema = z.strictObject({
  schemaVersion: z.literal(BROKER_LEDGER_SCHEMA_VERSION),
  model: z.string().min(1),
  maxBudgetUsd: positiveFinite.nullable(),
  acceptedRequests: nonNegativeSafeInteger,
  billableRequests: nonNegativeSafeInteger,
  completedBillableRequests: nonNegativeSafeInteger,
  estimatedBillableRequests: nonNegativeSafeInteger,
  estimatedByReason: z.strictObject({
    'no-usage': nonNegativeSafeInteger,
    'response-aborted': nonNegativeSafeInteger,
    'upstream-error': nonNegativeSafeInteger,
  }),
  spentUsd: nonNegativeFinite,
  reservedUsd: nonNegativeFinite,
  usage: usageSchema,
  complete: z.boolean(),
  updatedAt: z.string().refine(value => !Number.isNaN(Date.parse(value)), 'must be a timestamp'),
});

function isRecord(value: unknown): value is JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isNumber(value: unknown): value is number {
  return typeof value === 'number';
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function fail(message: string): never {
  throw new Error(`credential broker: ${message}`);
}

export function validatePricingRates(value: unknown): PricingRates {
  try { return validateSharedPricingRates(value, { at: 'pricingRates' }); }
  catch (error) { return fail(errorMessage(error)); }
}

export function priceNormalizedClaudeUsage(usage: ClaudeUsage, rates: PricingRates): number {
  return priceClaudeUsage({
    input_tokens: usage.input,
    output_tokens: usage.output,
    cache_read_input_tokens: usage.cacheRead,
    cache_creation: {
      ephemeral_5m_input_tokens: usage.cacheWrite5m,
      ephemeral_1h_input_tokens: usage.cacheWrite1h,
    },
  }, rates);
}

function rawUsage(usage: ClaudeUsage): JsonRecord {
  return {
    input_tokens: usage.input,
    output_tokens: usage.output,
    cache_read_input_tokens: usage.cacheRead,
    cache_creation_input_tokens: usage.cacheWrite5m + usage.cacheWrite1h,
    cache_creation: {
      ephemeral_5m_input_tokens: usage.cacheWrite5m,
      ephemeral_1h_input_tokens: usage.cacheWrite1h,
    },
  };
}

function brokerCoversCliUsage(broker: ClaudeUsage, cli: ClaudeUsage): boolean {
  return broker.input >= cli.input
    && broker.output >= cli.output
    && broker.cacheRead >= cli.cacheRead
    && broker.cacheWrite5m + broker.cacheWrite1h >= cli.cacheWrite5m + cli.cacheWrite1h;
}

export function validateBrokerConfig(value: unknown): BrokerConfig {
  const parsed = brokerConfigSchema.safeParse(value);
  if (!parsed.success) fail(formatZodError(parsed.error, 'configuration'));
  const { pricingRates, ...config } = parsed.data;
  return config.maxBudgetUsd === null || config.maxBudgetUsd === undefined
    ? config
    : { ...config, pricingRates: validatePricingRates(pricingRates) };
}

function validateLedger(value: unknown,
  { model = null, maxBudgetUsd = undefined }: { model?: string | null; maxBudgetUsd?: number | null } = {}): BrokerLedger {
  const parsed = brokerLedgerSchema.safeParse(value);
  if (!parsed.success) fail(formatZodError(parsed.error, 'spend ledger'));
  const ledger = parsed.data;
  if (model !== null && ledger.model !== model) fail('spend ledger model does not match');
  if (maxBudgetUsd !== undefined && ledger.maxBudgetUsd !== maxBudgetUsd) {
    fail('spend ledger budget does not match');
  }
  if (ledger.completedBillableRequests > ledger.billableRequests) {
    fail('spend ledger completed request count is invalid');
  }
  if (ledger.estimatedBillableRequests > ledger.completedBillableRequests) {
    fail('spend ledger estimated request count is invalid');
  }
  const reasons = ESTIMATE_REASONS.reduce((sum, reason) => sum + ledger.estimatedByReason[reason], 0);
  if (reasons !== ledger.estimatedBillableRequests) fail('spend ledger estimate reasons do not add up');
  const complete = ledger.reservedUsd === 0
    && ledger.completedBillableRequests === ledger.billableRequests;
  if (ledger.complete !== complete) fail('spend ledger completion state is invalid');
  return ledger;
}

export function writeCredentialBrokerLedger(path: string | undefined, value: unknown): void {
  if (!path) return;
  const ledger = validateLedger(value);
  const temporary = `${path}.${process.pid}.${randomBytes(8).toString('hex')}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(ledger)}\n`, { flag: 'wx', mode: 0o600 });
  try { renameSync(temporary, path); }
  catch (error) { rmSync(temporary, { force: true }); throw error; }
}

export function readCredentialBrokerLedger(path: string,
  expected: { model?: string | null; maxBudgetUsd?: number | null } = {}): BrokerLedger {
  return validateLedger(JSON.parse(readFileSync(path, 'utf8')), expected);
}

export function reconcileCredentialBrokerReceipt({ ledger, cliResult, model, maxBudgetUsd,
  pricingRates, brokerDiagnostics = null, toleranceUsd = COST_TOLERANCE_USD }: {
  ledger: unknown; cliResult: unknown; model: unknown; maxBudgetUsd: unknown; pricingRates: unknown;
  brokerDiagnostics?: unknown; toleranceUsd?: number;
}): { ok: boolean; result: CredentialBrokerResult; receipt: CredentialBrokerReceipt } {
  if (typeof model !== 'string' || !model) fail('receipt model is invalid');
  if (!isNumber(maxBudgetUsd) || !Number.isFinite(maxBudgetUsd) || maxBudgetUsd <= 0) fail('receipt budget is invalid');
  if (!Number.isFinite(toleranceUsd) || toleranceUsd < 0) fail('receipt tolerance is invalid');
  const receiptBudget = maxBudgetUsd;
  let verifiedLedger: BrokerLedger | null = null;
  let verifiedRates: PricingRates | null = null;
  let usage: ClaudeUsage | null = null;
  let cliUsage: ClaudeUsage | null = null;
  let calculatedCostUsd: number | null = null;
  let issue: string | null = null;
  try { verifiedLedger = validateLedger(ledger, { model, maxBudgetUsd: receiptBudget }); }
  catch (error) { issue = errorMessage(error); }
  if (!issue && verifiedLedger?.complete !== true) issue = 'credential broker spend ledger is incomplete';
  const estimatedRequests = verifiedLedger?.estimatedBillableRequests ?? 0;
  const exact = verifiedLedger !== null && estimatedRequests === 0;
  try { verifiedRates = validatePricingRates(pricingRates); }
  catch (error) { if (!issue) issue = errorMessage(error); }
  try { cliUsage = normalizeClaudeUsage(isRecord(cliResult) ? cliResult.usage : undefined); }
  catch (error) { if (!issue) issue = errorMessage(error); }
  if (verifiedLedger) usage = structuredClone(verifiedLedger.usage);
  if (!issue && cliUsage && usage && !brokerCoversCliUsage(usage, cliUsage)) {
    issue = 'credential broker usage is lower than CLI usage totals';
  }
  try { if (verifiedRates && usage) calculatedCostUsd = priceNormalizedClaudeUsage(usage, verifiedRates); }
  catch (error) { if (!issue) issue = errorMessage(error); }
  const brokerCost = verifiedLedger
    ? Math.min(receiptBudget, verifiedLedger.spentUsd + verifiedLedger.reservedUsd) : receiptBudget;
  const cliCost = Number(isRecord(cliResult) ? cliResult.total_cost_usd : undefined);
  if (!issue && (!Number.isFinite(cliCost) || cliCost < 0)) {
    issue = 'coding session did not return a usable cost receipt';
  }
  // Exact spend must price back to the broker's figure. Estimated requests
  // add their ceilings on top of the priced usage, so the priced usage can
  // only fall below the broker's figure, never above it.
  if (!issue && calculatedCostUsd !== null && exact && Math.abs(calculatedCostUsd - brokerCost) > toleranceUsd) {
    issue = `usage-priced spend $${calculatedCostUsd.toFixed(6)} does not match credential broker spend $${brokerCost.toFixed(6)}`;
  }
  if (!issue && calculatedCostUsd !== null && !exact && calculatedCostUsd - brokerCost > toleranceUsd) {
    issue = `usage-priced spend $${calculatedCostUsd.toFixed(6)} exceeds credential broker spend $${brokerCost.toFixed(6)}`;
  }
  const receipt: CredentialBrokerReceipt = {
    schemaVersion: 3,
    source: 'credential-broker',
    model,
    maxBudgetUsd: receiptBudget,
    costUsd: Number(brokerCost.toFixed(6)),
    cliCostUsd: Number.isFinite(cliCost) && cliCost >= 0 ? Number(cliCost.toFixed(6)) : null,
    calculatedCostUsd: calculatedCostUsd === null ? null : Number(calculatedCostUsd.toFixed(6)),
    usage,
    pricingRates: verifiedRates,
    exact,
    estimatedRequests,
    estimatedByReason: verifiedLedger ? structuredClone(verifiedLedger.estimatedByReason) : noEstimates(),
    complete: verifiedLedger?.complete === true,
    reconciled: issue === null,
    error: issue,
  };
  const result: CredentialBrokerResult = {
    ...(isRecord(cliResult)
      ? structuredClone(cliResult) : { type: 'result', is_error: true, result: '' }),
    total_cost_usd: receipt.costUsd,
    stack_bench_cost_receipt: receipt,
  };
  if (usage) result.usage = rawUsage(usage);
  if (brokerDiagnostics) result.stack_bench_credential_broker = structuredClone(brokerDiagnostics);
  if (issue) {
    result.is_error = true;
    result.terminal_reason = 'cost_receipt_error';
    result.result = [typeof result.result === 'string' ? result.result.trim() : '', issue]
      .filter(Boolean).join('\n');
  }
  return { ok: issue === null, result, receipt };
}
