import { randomBytes } from 'node:crypto';
import { readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';

import { normalizeClaudeUsage, priceClaudeUsage } from '../src/evidence/claude-usage-cost.js';
import type { ClaudeUsage } from '../src/evidence/claude-usage-cost.js';
import { validatePricingRates as validateSharedPricingRates } from '../src/evidence/pricing-authority.js';

export const BROKER_LEDGER_SCHEMA_VERSION = 3;
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
  spentUsd: number;
  reservedUsd: number;
  usage: ClaudeUsage;
  complete: boolean;
  updatedAt: string;
};

export interface CredentialBrokerReceipt {
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

export interface CredentialBrokerResult extends JsonRecord {
  total_cost_usd: number;
  usage?: ReturnType<typeof rawUsage>;
  stack_bench_cost_receipt: CredentialBrokerReceipt;
}

function isRecord(value: unknown): value is JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isNumber(value: unknown): value is number {
  return typeof value === 'number';
}

function isString(value: unknown): value is string {
  return typeof value === 'string';
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
  if (!isRecord(value)) fail('configuration must be an object');
  if (value.mode !== 'api-key' && value.mode !== 'subscription-token') fail('credential mode is invalid');
  for (const field of ['credential', 'sessionToken']) {
    if (!isString(value[field]) || value[field].length < 16) fail(`${field} is invalid`);
  }
  if (value.readyPath !== undefined && (typeof value.readyPath !== 'string' || !value.readyPath)) {
    fail('readyPath is invalid');
  }
  if (value.parentPid !== undefined && (!isNumber(value.parentPid)
    || !Number.isInteger(value.parentPid) || value.parentPid < 1)) fail('parentPid is invalid');
  if (value.expiresAt !== undefined && (!isNumber(value.expiresAt)
    || !Number.isFinite(value.expiresAt) || value.expiresAt <= Date.now())) fail('expiresAt is invalid');
  if (value.listenHost !== undefined && value.listenHost !== '127.0.0.1' && value.listenHost !== '0.0.0.0') {
    fail('listenHost is invalid');
  }
  if (value.ledgerPath !== undefined && (typeof value.ledgerPath !== 'string' || !value.ledgerPath)) {
    fail('ledgerPath is invalid');
  }
  if (typeof value.model !== 'string' || !value.model) fail('model is invalid');
  if (!isNumber(value.maxOutputTokens) || !Number.isInteger(value.maxOutputTokens)
    || value.maxOutputTokens < 1 || value.maxOutputTokens > MAX_BROKER_OUTPUT_TOKENS) {
    fail('maxOutputTokens is invalid');
  }
  if (value.maxBudgetUsd !== null && value.maxBudgetUsd !== undefined) {
    if (!isNumber(value.maxBudgetUsd) || !Number.isFinite(value.maxBudgetUsd) || value.maxBudgetUsd <= 0) {
      fail('maxBudgetUsd is invalid');
    }
    validatePricingRates(value.pricingRates);
  }
  return {
    mode: value.mode,
    credential: value.credential as string,
    sessionToken: value.sessionToken as string,
    ...(typeof value.readyPath === 'string' ? { readyPath: value.readyPath } : {}),
    ...(typeof value.parentPid === 'number' ? { parentPid: value.parentPid } : {}),
    ...(typeof value.expiresAt === 'number' ? { expiresAt: value.expiresAt } : {}),
    ...(value.listenHost === '127.0.0.1' || value.listenHost === '0.0.0.0'
      ? { listenHost: value.listenHost } : {}),
    ...(typeof value.ledgerPath === 'string' ? { ledgerPath: value.ledgerPath } : {}),
    model: value.model,
    maxOutputTokens: value.maxOutputTokens,
    ...(typeof value.maxBudgetUsd === 'number' ? { maxBudgetUsd: value.maxBudgetUsd }
      : value.maxBudgetUsd === null ? { maxBudgetUsd: null } : {}),
    ...(value.maxBudgetUsd !== null && value.maxBudgetUsd !== undefined
      ? { pricingRates: validatePricingRates(value.pricingRates) } : {}),
  };
}

function validateLedger(value: unknown,
  { model = null, maxBudgetUsd = undefined }: { model?: string | null; maxBudgetUsd?: number | null } = {}): BrokerLedger {
  if (!isRecord(value)) fail('spend ledger is invalid');
  const fields = new Set(['schemaVersion', 'model', 'maxBudgetUsd', 'acceptedRequests',
    'billableRequests', 'completedBillableRequests', 'estimatedBillableRequests',
    'spentUsd', 'reservedUsd', 'usage', 'complete', 'updatedAt']);
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`spend ledger.${key} is unknown`);
  if (value.schemaVersion !== BROKER_LEDGER_SCHEMA_VERSION) fail('spend ledger schema is invalid');
  if (typeof value.model !== 'string' || !value.model) fail('spend ledger model is invalid');
  if (model !== null && value.model !== model) fail('spend ledger model does not match');
  if (value.maxBudgetUsd !== null
    && (!isNumber(value.maxBudgetUsd) || !Number.isFinite(value.maxBudgetUsd) || value.maxBudgetUsd <= 0)) {
    fail('spend ledger budget is invalid');
  }
  if (maxBudgetUsd !== undefined && value.maxBudgetUsd !== maxBudgetUsd) fail('spend ledger budget does not match');
  for (const field of ['acceptedRequests', 'billableRequests', 'completedBillableRequests',
    'estimatedBillableRequests']) {
    if (!isNumber(value[field]) || !Number.isSafeInteger(value[field]) || value[field] < 0) {
      fail(`spend ledger.${field} is invalid`);
    }
  }
  const completedBillableRequests = value.completedBillableRequests as number;
  const billableRequests = value.billableRequests as number;
  const estimatedBillableRequests = value.estimatedBillableRequests as number;
  if (completedBillableRequests > billableRequests) fail('spend ledger completed request count is invalid');
  if (estimatedBillableRequests > completedBillableRequests) fail('spend ledger estimated request count is invalid');
  for (const field of ['spentUsd', 'reservedUsd']) {
    if (!isNumber(value[field]) || !Number.isFinite(value[field]) || value[field] < 0) {
      fail(`spend ledger.${field} is invalid`);
    }
  }
  if (!isRecord(value.usage)
    || Object.keys(value.usage).some(field => !(CLAUDE_USAGE_FIELDS as readonly string[]).includes(field))) {
    fail('spend ledger.usage is invalid');
  }
  const usageSource = value.usage;
  for (const field of CLAUDE_USAGE_FIELDS) {
    if (!isNumber(usageSource[field]) || !Number.isSafeInteger(usageSource[field]) || usageSource[field] < 0) {
      fail(`spend ledger.usage.${field} is invalid`);
    }
  }
  const complete = value.reservedUsd === 0 && completedBillableRequests === billableRequests;
  if (value.complete !== complete) fail('spend ledger completion state is invalid');
  if (typeof value.updatedAt !== 'string' || Number.isNaN(Date.parse(value.updatedAt))) {
    fail('spend ledger timestamp is invalid');
  }
  return {
    schemaVersion: value.schemaVersion as number,
    model: value.model as string,
    maxBudgetUsd: value.maxBudgetUsd as number | null,
    acceptedRequests: value.acceptedRequests as number,
    billableRequests,
    completedBillableRequests,
    estimatedBillableRequests,
    spentUsd: value.spentUsd as number,
    reservedUsd: value.reservedUsd as number,
    usage: {
      input: usageSource.input as number,
      output: usageSource.output as number,
      cacheRead: usageSource.cacheRead as number,
      cacheWrite5m: usageSource.cacheWrite5m as number,
      cacheWrite1h: usageSource.cacheWrite1h as number,
    },
    complete: value.complete as boolean,
    updatedAt: value.updatedAt as string,
  };
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
  if (!issue && verifiedLedger?.estimatedBillableRequests !== 0) {
    issue = 'credential broker contains billable requests without exact provider usage';
  }
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
  if (!issue && calculatedCostUsd !== null && Math.abs(calculatedCostUsd - brokerCost) > toleranceUsd) {
    issue = `usage-priced spend $${calculatedCostUsd.toFixed(6)} does not match credential broker spend $${brokerCost.toFixed(6)}`;
  }
  const receipt: CredentialBrokerReceipt = {
    schemaVersion: 2,
    source: 'credential-broker',
    model,
    maxBudgetUsd: receiptBudget,
    costUsd: Number(brokerCost.toFixed(6)),
    cliCostUsd: Number.isFinite(cliCost) && cliCost >= 0 ? Number(cliCost.toFixed(6)) : null,
    calculatedCostUsd: calculatedCostUsd === null ? null : Number(calculatedCostUsd.toFixed(6)),
    usage,
    pricingRates: verifiedRates,
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
