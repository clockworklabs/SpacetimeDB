#!/usr/bin/env node
import { randomBytes } from 'node:crypto';
import { spawn } from 'node:child_process';
import type { ChildProcess } from 'node:child_process';
import { createServer } from 'node:http';
import type { ClientRequest, IncomingMessage, OutgoingHttpHeaders, ServerResponse } from 'node:http';
import { request as httpsRequest } from 'node:https';
import type { RequestOptions } from 'node:https';
import type { Socket } from 'node:net';
import { chmodSync, existsSync, mkdtempSync, readFileSync, renameSync, rmSync,
  writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { AddressInfo } from 'node:net';
import { brotliDecompressSync, gunzipSync, inflateSync } from 'node:zlib';

import { containerAuthSecret } from './container-auth.js';
import { killTree } from '../src/runtime/platform.js';
import { compiledEntrypoint } from '../src/package-root.js';
import { normalizeClaudeUsage, priceClaudeUsage } from '../src/evidence/claude-usage-cost.js';
import { validatePricingRates as validateSharedPricingRates } from '../src/evidence/pricing-authority.js';

const MAX_REQUEST_BYTES = 32 * 1024 * 1024;
const MAX_REQUESTS = 512;
const MAX_OUTPUT_TOKENS = 128_000;
const ALLOWED_PATHS = new Set(['/v1/messages', '/v1/messages/count_tokens']);
const LEDGER_SCHEMA_VERSION = 3;
const COST_TOLERANCE_USD = 0.0001;
const BROKER_DRAIN_TIMEOUT_MS = 30_000;
const BROKER_DRAIN_POLL_MS = 100;
const BROKER_STDERR_LIMIT_BYTES = 16 * 1024;
const BROKER_STOP_GRACE_MS = 2_000;
const BROKER_STOP_FORCE_MS = 2_000;
const BROKER_SERVER_CLOSE_GRACE_MS = 1_000;
const USAGE_FIELDS = ['input', 'output', 'cacheRead', 'cacheWrite5m', 'cacheWrite1h'] as const;

export type ClaudeUsage = { input: number; output: number; cacheRead: number;
  cacheWrite5m: number; cacheWrite1h: number };
export type PricingRates = ReturnType<typeof validateSharedPricingRates>;
type JsonRecord = Record<string, unknown>;
export type BrokerMode = 'api-key' | 'subscription-token';
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
export type BrokerProcessState = {
  exitCode: number | null;
  signal: NodeJS.Signals | null;
  exitedAt: string | null;
  stderrTail: string;
  stderrPending: string;
  stderrTruncated: boolean;
};
export type BrokerError = { type: string; phase: string; message: string };
export interface BrokerDrainDiagnostics {
  timeoutMs: number;
  elapsedMs: number;
  timedOut: boolean;
  reason: string | null;
  terminationRequested: boolean;
}

export interface BrokerTerminationDiagnostics {
  gracefulRequested: boolean;
  forceRequested: boolean;
  exited: boolean;
  gracefulTimeoutMs: number;
  forceTimeoutMs: number;
}

export type BrokerDiagnostics = {
  schemaVersion: number;
  endpointKind: string;
  child: { pid: number | undefined | null; exitCode: number | null; signal: NodeJS.Signals | null;
    exitedAt: string | null; stderrTail: string | null; stderrTruncated: boolean };
  drain: BrokerDrainDiagnostics | null;
  termination: BrokerTerminationDiagnostics | null;
  ledger: BrokerLedger | null;
  errors: BrokerError[];
};
export interface BrokerStats {
  acceptedRequests: number;
  billableRequests: number;
  completedBillableRequests: number;
  estimatedBillableRequests: number;
  spentUsd: number;
  reservedUsd: number;
}

export interface CreatedCredentialBroker {
  server: ReturnType<typeof createServer>;
  stats: () => BrokerStats;
}

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
export interface CredentialBrokerChild {
  pid?: number;
  exitCode?: number | null;
  signalCode?: NodeJS.Signals | null;
  kill?: (signal?: NodeJS.Signals | number) => boolean;
}

export interface CredentialBrokerHandle {
  child: CredentialBrokerChild;
  root: string;
  ledgerPath: string;
  model: string;
  maxBudgetUsd: number | null;
  sessionToken?: string;
  baseUrl?: string;
  listenHost?: string;
  endpointKind?: string;
  processState?: Partial<BrokerProcessState>;
  diagnosticSecrets?: string[];
  finalDiagnostics?: BrokerDiagnostics | null;
  finalLedger?: BrokerLedger | null;
}

export interface CredentialBroker extends CredentialBrokerHandle {
  child: ChildProcess;
  sessionToken: string;
  baseUrl: string;
  listenHost: string;
  endpointKind: string;
  processState: BrokerProcessState;
  diagnosticSecrets: string[];
  finalDiagnostics: BrokerDiagnostics | null;
  finalLedger: BrokerLedger | null;
}
type UpstreamRequest = (options: RequestOptions,
  callback: (response: IncomingMessage) => void) => ClientRequest;
type Alive = (pid: number | undefined) => boolean;

function isRecord(value: unknown): value is JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isNumber(value: unknown): value is number {
  return typeof value === 'number';
}

function isString(value: unknown): value is string {
  return typeof value === 'string';
}

const wait = (ms: number): Promise<void> => new Promise(resolve => setTimeout(resolve, ms));
const roundUsd = (value: number): number => Number(value.toFixed(6));
const reserveUsd = (value: number): number => Math.ceil(value * 1e6) / 1e6;

function priceNormalizedUsage(usage: ClaudeUsage, rates: PricingRates): number {
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
    && broker.cacheWrite5m + broker.cacheWrite1h
      >= cli.cacheWrite5m + cli.cacheWrite1h;
}

function fail(message: string): never {
  throw new Error(`credential broker: ${message}`);
}

function validatePricingRates(value: unknown): PricingRates {
  try { return validateSharedPricingRates(value, { at: 'pricingRates' }); }
  catch (error) { return fail(errorMessage(error)); }
}

function validateConfig(value: unknown): BrokerConfig {
  if (!isRecord(value)) fail('configuration must be an object');
  if (value.mode !== 'api-key' && value.mode !== 'subscription-token') fail('credential mode is invalid');
  for (const field of ['credential', 'sessionToken']) {
    if (!isString(value[field]) || value[field].length < 16) fail(`${field} is invalid`);
  }
  if (value.readyPath !== undefined && (typeof value.readyPath !== 'string' || !value.readyPath)) {
    fail('readyPath is invalid');
  }
  if (value.parentPid !== undefined && (!isNumber(value.parentPid)
    || !Number.isInteger(value.parentPid) || value.parentPid < 1)) {
    fail('parentPid is invalid');
  }
  if (value.expiresAt !== undefined && (!isNumber(value.expiresAt)
    || !Number.isFinite(value.expiresAt) || value.expiresAt <= Date.now())) {
    fail('expiresAt is invalid');
  }
  if (value.listenHost !== undefined && value.listenHost !== '127.0.0.1' && value.listenHost !== '0.0.0.0') {
    fail('listenHost is invalid');
  }
  if (value.ledgerPath !== undefined && (typeof value.ledgerPath !== 'string'
    || !value.ledgerPath)) fail('ledgerPath is invalid');
  if (typeof value.model !== 'string' || !value.model) fail('model is invalid');
  if (!isNumber(value.maxOutputTokens) || !Number.isInteger(value.maxOutputTokens) || value.maxOutputTokens < 1
    || value.maxOutputTokens > MAX_OUTPUT_TOKENS) fail('maxOutputTokens is invalid');
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
    'spentUsd', 'reservedUsd',
    'usage', 'complete', 'updatedAt']);
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`spend ledger.${key} is unknown`);
  if (value.schemaVersion !== LEDGER_SCHEMA_VERSION) fail('spend ledger schema is invalid');
  if (typeof value.model !== 'string' || !value.model) fail('spend ledger model is invalid');
  if (model !== null && value.model !== model) fail('spend ledger model does not match');
  if (value.maxBudgetUsd !== null
    && (!isNumber(value.maxBudgetUsd) || !Number.isFinite(value.maxBudgetUsd) || value.maxBudgetUsd <= 0)) {
    fail('spend ledger budget is invalid');
  }
  if (maxBudgetUsd !== undefined && value.maxBudgetUsd !== maxBudgetUsd) {
    fail('spend ledger budget does not match');
  }
  for (const field of ['acceptedRequests', 'billableRequests', 'completedBillableRequests',
    'estimatedBillableRequests']) {
    if (!isNumber(value[field]) || !Number.isSafeInteger(value[field]) || value[field] < 0) {
      fail(`spend ledger.${field} is invalid`);
    }
  }
  const completedBillableRequests = value.completedBillableRequests as number;
  const billableRequests = value.billableRequests as number;
  const estimatedBillableRequests = value.estimatedBillableRequests as number;
  if (completedBillableRequests > billableRequests) {
    fail('spend ledger completed request count is invalid');
  }
  if (estimatedBillableRequests > completedBillableRequests) {
    fail('spend ledger estimated request count is invalid');
  }
  for (const field of ['spentUsd', 'reservedUsd']) {
    if (!isNumber(value[field]) || !Number.isFinite(value[field]) || value[field] < 0) {
      fail(`spend ledger.${field} is invalid`);
    }
  }
  if (!isRecord(value.usage)
    || Object.keys(value.usage).some(field => !(USAGE_FIELDS as readonly string[]).includes(field))) {
    fail('spend ledger.usage is invalid');
  }
  const usageSource = value.usage;
  for (const field of USAGE_FIELDS) {
    if (!isNumber(usageSource[field]) || !Number.isSafeInteger(usageSource[field]) || usageSource[field] < 0) {
      fail(`spend ledger.usage.${field} is invalid`);
    }
  }
  const complete = value.reservedUsd === 0 && completedBillableRequests === billableRequests;
  if (value.complete !== complete) fail('spend ledger completion state is invalid');
  if (typeof value.updatedAt !== 'string' || Number.isNaN(Date.parse(value.updatedAt))) {
    fail('spend ledger timestamp is invalid');
  }
  const usage: ClaudeUsage = {
    input: usageSource.input as number,
    output: usageSource.output as number,
    cacheRead: usageSource.cacheRead as number,
    cacheWrite5m: usageSource.cacheWrite5m as number,
    cacheWrite1h: usageSource.cacheWrite1h as number,
  };
  return { schemaVersion: value.schemaVersion as number, model: value.model as string,
    maxBudgetUsd: value.maxBudgetUsd as number | null,
    acceptedRequests: value.acceptedRequests as number, billableRequests: value.billableRequests as number,
    completedBillableRequests: value.completedBillableRequests as number,
    estimatedBillableRequests: value.estimatedBillableRequests as number,
    spentUsd: value.spentUsd as number, reservedUsd: value.reservedUsd as number, usage,
    complete: value.complete as boolean, updatedAt: value.updatedAt as string };
}

function writeLedger(path: string | undefined, value: unknown): void {
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
  const receiptBudget = maxBudgetUsd;
  if (!Number.isFinite(toleranceUsd) || toleranceUsd < 0) fail('receipt tolerance is invalid');
  let verifiedLedger: BrokerLedger | null = null;
  let verifiedRates: PricingRates | null = null;
  let usage: ClaudeUsage | null = null;
  let cliUsage: ClaudeUsage | null = null;
  let calculatedCostUsd: number | null = null;
  let issue: string | null = null;
  try { verifiedLedger = validateLedger(ledger, { model, maxBudgetUsd: receiptBudget }); }
  catch (error) { issue = errorMessage(error); }
  if (!issue && verifiedLedger?.complete !== true) {
    issue = 'credential broker spend ledger is incomplete';
  }
  if (!issue && verifiedLedger?.estimatedBillableRequests !== 0) {
    issue = 'credential broker contains billable requests without exact provider usage';
  }
  try { verifiedRates = validatePricingRates(pricingRates); }
  catch (error) { if (!issue) issue = errorMessage(error); }
  try {
    cliUsage = normalizeClaudeUsage(isRecord(cliResult) ? cliResult.usage : undefined);
  } catch (error) { if (!issue) issue = errorMessage(error); }
  if (verifiedLedger) usage = structuredClone(verifiedLedger.usage);
  // Broker usage is authoritative; CLI usage is its lower-bound check.
  if (!issue && cliUsage && usage && !brokerCoversCliUsage(usage, cliUsage)) {
    issue = 'credential broker usage is lower than CLI usage totals';
  }
  try {
    if (verifiedRates && usage) calculatedCostUsd = priceNormalizedUsage(usage, verifiedRates);
  } catch (error) { if (!issue) issue = errorMessage(error); }
  const brokerCost = verifiedLedger
    ? Math.min(receiptBudget, verifiedLedger.spentUsd + verifiedLedger.reservedUsd)
    : receiptBudget;
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
    calculatedCostUsd: calculatedCostUsd === null
      ? null : Number(calculatedCostUsd.toFixed(6)),
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
  if (brokerDiagnostics) {
    result.stack_bench_credential_broker = structuredClone(brokerDiagnostics);
  }
  if (issue) {
    result.is_error = true;
    result.terminal_reason = 'cost_receipt_error';
    result.result = [typeof result.result === 'string' ? result.result.trim() : '', issue]
      .filter(Boolean).join('\n');
  }
  return { ok: issue === null, result, receipt };
}

function clientAuthorized(request: IncomingMessage, sessionToken: string): boolean {
  return request.headers.authorization === `Bearer ${sessionToken}`
    || request.headers['x-api-key'] === sessionToken;
}

function upstreamHeaders(request: IncomingMessage, config: BrokerConfig): OutgoingHttpHeaders {
  const headers: OutgoingHttpHeaders = { ...request.headers };
  delete headers.host;
  // Request identity encoding so accounting and the client read the same bytes.
  delete headers['accept-encoding'];
  delete headers.authorization;
  delete headers['proxy-authorization'];
  delete headers['x-api-key'];
  if (config.mode === 'api-key') headers['x-api-key'] = config.credential;
  else headers.authorization = `Bearer ${config.credential}`;
  return headers;
}

function requestPath(value: string | undefined): string | null {
  try { return new URL(value ?? '', 'http://credential-broker.invalid').pathname; }
  catch { return null; }
}

function rejectRequest(request: IncomingMessage, response: ServerResponse,
  status: number, message: string): void {
  const send = () => {
    if (response.destroyed || response.writableEnded) return;
    try {
      if (!response.headersSent) response.writeHead(status, { 'content-type': 'text/plain' });
      response.end(message);
    } catch { response.destroy(); }
  };
  request.on('error', () => {});
  response.on('error', () => {});
  if (request.complete) send();
  else {
    request.once('end', send);
    request.resume();
  }
}

function parseProviderRequest(body: Buffer, path: string, config: BrokerConfig): JsonRecord {
  let payload: unknown;
  try { payload = JSON.parse(body.toString('utf8')); }
  catch { fail('request body must be valid JSON'); }
  if (!isRecord(payload)) {
    fail('request body must be an object');
  }
  if (payload.model !== config.model) fail('request model does not match the selected model');
  if (path === '/v1/messages'
    && (!isNumber(payload.max_tokens) || !Number.isInteger(payload.max_tokens) || payload.max_tokens < 1
      || payload.max_tokens > config.maxOutputTokens)) {
    fail(`max_tokens must be from 1 through ${config.maxOutputTokens}`);
  }
  return payload;
}

function requestCostCeiling(bodyBytes: number, maxTokens: number, rates: PricingRates): number {
  const inputRate = Math.max(rates.input, rates.cacheWrite5m, rates.cacheWrite1h);
  return bodyBytes * inputRate / 1e6 + maxTokens * rates.output / 1e6;
}

function decodedResponseBody(body: Buffer, contentEncoding: string | string[] | undefined): Buffer {
  const encodings = String(contentEncoding ?? '')
    .split(',').map(value => value.trim().toLowerCase()).filter(Boolean);
  let decoded = body;
  for (const encoding of encodings.reverse()) {
    if (encoding === 'identity') continue;
    const options = { maxOutputLength: MAX_REQUEST_BYTES };
    if (encoding === 'gzip' || encoding === 'x-gzip') decoded = gunzipSync(decoded, options);
    else if (encoding === 'deflate') decoded = inflateSync(decoded, options);
    else if (encoding === 'br') decoded = brotliDecompressSync(decoded, options);
    else throw new Error(`unsupported response encoding ${encoding}`);
  }
  return decoded;
}

function responseUsage(body: Buffer, contentEncoding: string | string[] | undefined = undefined): JsonRecord | null {
  const values: JsonRecord[] = [];
  const add = (value: unknown): void => {
    if (!isRecord(value)) return;
    if (isRecord(value.usage)) values.push(value.usage);
    if (isRecord(value.message) && isRecord(value.message.usage)) values.push(value.message.usage);
  };
  let text: string;
  try { text = decodedResponseBody(body, contentEncoding).toString('utf8'); }
  catch { return null; }
  try {
    add(JSON.parse(text));
  } catch {
    let sawError = false;
    let sawFinalUsage = false;
    let sawMessageStop = false;
    for (const line of text.split(/\r?\n/)) {
      if (!line.startsWith('data:')) continue;
      const data = line.slice(5).trim();
      if (!data || data === '[DONE]') continue;
      try {
        const event = JSON.parse(data);
        if (isRecord(event) && event.type === 'error') sawError = true;
        if (isRecord(event) && event.type === 'message_delta' && isRecord(event.usage)) sawFinalUsage = true;
        if (isRecord(event) && event.type === 'message_stop') sawMessageStop = true;
        add(event);
      } catch { /* Ignore non-JSON event data. */ }
    }
    if (sawError || !sawFinalUsage || !sawMessageStop) return null;
  }
  if (values.length === 0) return null;
  const number = (field: string): number => Math.max(0, ...values.map(value => Number(value[field]) || 0));
  const cacheWrite = (field: string): number => Math.max(0, ...values.map(value =>
    isRecord(value.cache_creation) ? Number(value.cache_creation[field]) || 0 : 0));
  const cacheWrite5m = cacheWrite('ephemeral_5m_input_tokens');
  const cacheWrite1h = cacheWrite('ephemeral_1h_input_tokens');
  const flatCacheWrite = number('cache_creation_input_tokens');
  return {
    input_tokens: number('input_tokens'),
    output_tokens: number('output_tokens'),
    cache_read_input_tokens: number('cache_read_input_tokens'),
    cache_creation: {
      ephemeral_5m_input_tokens: cacheWrite5m + cacheWrite1h > 0 ? cacheWrite5m : flatCacheWrite,
      ephemeral_1h_input_tokens: cacheWrite1h,
    },
  };
}

export function createCredentialBroker(configInput: unknown, {
  requestUpstream = httpsRequest as UpstreamRequest,
  upstream = { protocol: 'https:', hostname: 'api.anthropic.com', port: 443 },
  maxRequests = MAX_REQUESTS,
  maxRequestBytes = MAX_REQUEST_BYTES,
}: { requestUpstream?: UpstreamRequest;
  upstream?: { protocol: string; hostname: string; port: number };
  maxRequests?: number; maxRequestBytes?: number } = {}): CreatedCredentialBroker {
  const config = validateConfig(configInput);
  let acceptedRequests = 0;
  let billableRequests = 0;
  let completedBillableRequests = 0;
  let estimatedBillableRequests = 0;
  let spentUsd = 0;
  let reservedUsd = 0;
  const usageTotals: ClaudeUsage = { input: 0, output: 0, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 };
  const recordLedger = () => writeLedger(config.ledgerPath, {
    schemaVersion: LEDGER_SCHEMA_VERSION,
    model: config.model,
    maxBudgetUsd: config.maxBudgetUsd ?? null,
    acceptedRequests,
    billableRequests,
    completedBillableRequests,
    estimatedBillableRequests,
    spentUsd: Number(spentUsd.toFixed(6)),
    reservedUsd: Number(reservedUsd.toFixed(6)),
    usage: usageTotals,
    complete: reservedUsd === 0 && completedBillableRequests === billableRequests,
    updatedAt: new Date().toISOString(),
  });
  recordLedger();
  const server = createServer((request, response) => {
    // A client can disappear while the broker is still draining an upstream
    // response. Socket errors must not terminate the broker and strand a paid
    // request reservation in the ledger.
    request.on('error', () => {});
    request.on('aborted', () => {});
    response.on('error', () => {});
    const responseOpen = (): boolean => !response.destroyed && !response.writableEnded;
    const writeHead = (status: number, headers: OutgoingHttpHeaders): void => {
      if (!responseOpen() || response.headersSent) return;
      try { response.writeHead(status, headers); }
      catch { response.destroy(); }
    };
    const endResponse = (body?: string | Buffer): void => {
      if (!responseOpen()) return;
      try { response.end(body); }
      catch { response.destroy(); }
    };
    if (!clientAuthorized(request, config.sessionToken)) {
      rejectRequest(request, response, 401, 'unauthorized');
      return;
    }
    const path = requestPath(request.url);
    if (request.method !== 'POST' || path === null || !ALLOWED_PATHS.has(path)) {
      rejectRequest(request, response, 404, 'not found');
      return;
    }
    acceptedRequests += 1;
    recordLedger();
    if (acceptedRequests > maxRequests) {
      rejectRequest(request, response, 429, 'session request limit reached');
      return;
    }

    const chunks: Buffer[] = [];
    let received = 0;
    let tooLarge = false;
    request.on('data', (chunk: Buffer) => {
      if (tooLarge) return;
      received += chunk.length;
      if (received > maxRequestBytes) {
        tooLarge = true;
        writeHead(413, { 'content-type': 'text/plain' });
        endResponse('request is too large');
        return;
      }
      chunks.push(chunk);
    });
    request.on('end', () => {
      if (tooLarge) return;
      const body = Buffer.concat(chunks);
      let payload: JsonRecord;
      try { payload = parseProviderRequest(body, path, config); }
      catch (error) {
        writeHead(400, { 'content-type': 'text/plain' });
        endResponse(errorMessage(error));
        return;
      }
      const billable = path === '/v1/messages' && config.maxBudgetUsd != null;
      const costCeiling = billable
        ? reserveUsd(requestCostCeiling(received, payload.max_tokens as number,
          config.pricingRates as PricingRates)) : 0;
      const budget = config.maxBudgetUsd;
      if (billable && budget !== null && budget !== undefined
        && spentUsd + reservedUsd + costCeiling > budget) {
        writeHead(402, { 'content-type': 'text/plain' });
        endResponse('session cost limit reached');
        return;
      }
      if (billable) billableRequests += 1;
      reservedUsd = roundUsd(reservedUsd + costCeiling);
      recordLedger();
      let billableSettled = !billable;
      const settleBillable = ({ usage = null, estimated = false }:
        { usage?: ClaudeUsage | null; estimated?: boolean } = {}): void => {
        if (billableSettled) return;
        billableSettled = true;
        reservedUsd = roundUsd(reservedUsd - costCeiling);
        completedBillableRequests += 1;
        if (estimated) {
          estimatedBillableRequests += 1;
          spentUsd = roundUsd(spentUsd + costCeiling);
        } else if (usage) {
          spentUsd = roundUsd(spentUsd + priceNormalizedUsage(usage, config.pricingRates as PricingRates));
          for (const field of USAGE_FIELDS) usageTotals[field] += usage[field];
        }
        recordLedger();
      };
      const headers = upstreamHeaders(request, config);
      for (const name of ['connection', 'keep-alive', 'proxy-connection', 'te', 'trailer',
        'transfer-encoding', 'upgrade']) delete headers[name];
      headers['content-length'] = String(received);
      const upstreamRequest = requestUpstream({
        protocol: upstream.protocol,
        hostname: upstream.hostname,
        port: upstream.port,
        method: request.method,
        path: request.url,
        headers,
      }, upstreamResponse => {
        writeHead(upstreamResponse.statusCode ?? 502, upstreamResponse.headers);
        const responseChunks: Buffer[] = [];
        let responseBytes = 0;
        upstreamResponse.on('data', (chunk: Buffer) => {
          responseBytes += chunk.length;
          if (responseBytes <= maxRequestBytes) responseChunks.push(chunk);
          if (responseOpen()) {
            try { response.write(chunk); }
            catch { response.destroy(); }
          }
        });
        upstreamResponse.on('end', () => {
          endResponse();
          if (!billable) return;
          if ((upstreamResponse.statusCode ?? 502) >= 200
            && (upstreamResponse.statusCode ?? 502) < 300) {
            const usage = responseBytes <= maxRequestBytes
              ? responseUsage(Buffer.concat(responseChunks), upstreamResponse.headers['content-encoding'])
              : null;
            if (!usage) settleBillable({ estimated: true });
            else try { settleBillable({ usage: normalizeClaudeUsage(usage) }); }
            catch { settleBillable({ estimated: true }); }
          } else {
            settleBillable();
          }
        });
        const settleAbortedResponse = () => {
          settleBillable({ estimated: true });
          if (responseOpen()) response.destroy();
        };
        upstreamResponse.once('aborted', settleAbortedResponse);
        upstreamResponse.once('error', settleAbortedResponse);
      });
      upstreamRequest.on('error', () => {
        settleBillable({ estimated: true });
        writeHead(502, { 'content-type': 'text/plain' });
        endResponse('upstream request failed');
      });
      upstreamRequest.end(body);
    });
  });
  server.on('clientError', (_error: Error, socket: Socket) => {
    socket.on('error', () => {});
    if (socket.writable) socket.end('HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n');
    else socket.destroy();
  });
  return { server, stats: () => ({ acceptedRequests,
    billableRequests, completedBillableRequests, estimatedBillableRequests,
    spentUsd: Number(spentUsd.toFixed(6)), reservedUsd: Number(reservedUsd.toFixed(6)) }) };
}

export function startCredentialBroker(selectedAuth: Parameters<typeof containerAuthSecret>[0], { networkMode, deadlineMs,
  model, maxOutputTokens = MAX_OUTPUT_TOKENS, maxBudgetUsd = null, pricingRates = null,
  env = process.env }: { networkMode: 'bridge' | 'host'; deadlineMs: number; model: string;
  maxOutputTokens?: number; maxBudgetUsd?: number | null; pricingRates?: PricingRates | null;
  env?: NodeJS.ProcessEnv }): CredentialBroker {
  if (!['bridge', 'host'].includes(networkMode)) fail('network mode is invalid');
  if (!Number.isFinite(deadlineMs) || deadlineMs <= 0) fail('deadline is invalid');
  const credential = containerAuthSecret(selectedAuth);
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-credential-broker-'));
  let child: ChildProcess | null = null;
  const processState: BrokerProcessState = { exitCode: null, signal: null, exitedAt: null,
    stderrTail: '', stderrPending: '', stderrTruncated: false };
  try {
    chmodSync(root, 0o700);
    const configPath = join(root, 'config.json');
    const readyPath = join(root, 'ready.json');
    const ledgerPath = join(root, 'spend-ledger.json');
    const sessionToken = randomBytes(32).toString('hex');
    const listenHost = networkMode === 'host' ? '127.0.0.1' : '0.0.0.0';
    const config = validateConfig({ schemaVersion: 1,
      mode: selectedAuth.mode, credential, sessionToken, readyPath, parentPid: process.pid,
      expiresAt: Date.now() + deadlineMs + 60_000, listenHost, ledgerPath, model, maxOutputTokens,
      maxBudgetUsd, pricingRates });
    writeFileSync(configPath, `${JSON.stringify(config)}\n`, { flag: 'wx', mode: 0o600 });
    child = spawn(process.execPath, [compiledEntrypoint('container', 'credential-broker.js'),
      '--config', configPath], {
      stdio: ['ignore', 'ignore', 'pipe'],
      windowsHide: true,
      env: Object.fromEntries(['PATH', 'Path', 'SystemRoot', 'WINDIR', 'SSL_CERT_FILE',
        'NODE_EXTRA_CA_CERTS', 'HTTPS_PROXY', 'HTTP_PROXY']
        .filter(name => env[name] !== undefined).map(name => [name, env[name]])),
    });
    const diagnosticSecrets = [credential, sessionToken];
    child.stderr?.on('data', (chunk: Buffer) => appendDiagnosticStderr(
      processState, chunk.toString('utf8'), diagnosticSecrets));
    child.once('exit', (code: number | null, signal: NodeJS.Signals | null) => {
      appendDiagnosticStderr(processState, '', diagnosticSecrets, true);
      processState.exitCode = code;
      processState.signal = signal;
      processState.exitedAt = new Date().toISOString();
    });
    let spawnError: Error | null = null;
    child.once('error', (error: Error) => { spawnError = error; });
    const readyDeadline = Date.now() + 10_000;
    while (!spawnError && child.exitCode === null && !existsSync(readyPath)
      && Date.now() < readyDeadline) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100);
    }
    if (spawnError) throw spawnError;
    if (!existsSync(readyPath)) throw new Error('credential broker did not become ready');
    const ready: unknown = JSON.parse(readFileSync(readyPath, 'utf8'));
    if (!isRecord(ready) || !isNumber(ready.port) || !Number.isInteger(ready.port) || ready.port < 1 || ready.port > 65535) {
      throw new Error('credential broker returned an invalid port');
    }
    if (ready.host !== listenHost) throw new Error('credential broker returned an invalid host');
    const readyPort = ready.port;
    const readyHost = ready.host as string;
    const host = networkMode === 'host' ? '127.0.0.1' : 'host.docker.internal';
    return { child, root, ledgerPath, model, maxBudgetUsd: maxBudgetUsd ?? null,
      sessionToken, baseUrl: `http://${host}:${readyPort}`, listenHost: readyHost,
      endpointKind: 'local-credential-broker', processState,
      diagnosticSecrets, finalDiagnostics: null, finalLedger: null };
  } catch (error) {
    if (child?.pid) killTree(child.pid);
    rmSync(root, { recursive: true, force: true });
    throw error;
  }
}

function ledgerDiagnostics(ledger: BrokerLedger | null): BrokerLedger | null {
  return ledger ? structuredClone(ledger) : null;
}

function redactDiagnosticText(value: unknown,
  broker: CredentialBrokerHandle | string[] | null | undefined): string {
  let result = String(value ?? '');
  const secrets = Array.isArray(broker) ? broker : broker?.diagnosticSecrets ?? [];
  for (const secret of secrets) {
    if (typeof secret === 'string' && secret) result = result.replaceAll(secret, '[REDACTED]');
  }
  return result;
}

function appendDiagnosticStderr(state: BrokerProcessState, chunk: string, secrets: string[], flush = false): void {
  const raw = state.stderrPending + chunk;
  let redacted = redactDiagnosticText(raw, secrets);
  let pendingLength = 0;
  if (!flush) {
    for (const secret of secrets) {
      for (let length = Math.min(secret.length - 1, redacted.length);
        length > pendingLength; length -= 1) {
        if (redacted.endsWith(secret.slice(0, length))) {
          pendingLength = length;
          break;
        }
      }
    }
  } else if (raw && secrets.some(secret => secret.startsWith(raw))) {
    redacted = '[REDACTED]';
  }
  state.stderrPending = pendingLength ? redacted.slice(-pendingLength) : '';
  const safe = pendingLength ? redacted.slice(0, -pendingLength) : redacted;
  const next = state.stderrTail + safe;
  if (Buffer.byteLength(next) > BROKER_STDERR_LIMIT_BYTES) {
    state.stderrTruncated = true;
    state.stderrTail = Buffer.from(next).subarray(-BROKER_STDERR_LIMIT_BYTES).toString('utf8');
  } else state.stderrTail = next;
}

function processAlive(pid: number | undefined): boolean {
  if (typeof pid !== 'number' || !Number.isInteger(pid) || pid < 1) return false;
  try { process.kill(pid, 0); return true; }
  catch (error) { return !isRecord(error) || error.code !== 'ESRCH'; }
}

function brokerExited(broker: CredentialBrokerHandle | null | undefined, alive: Alive): boolean {
  const state = broker?.processState;
  if (!state) return !alive(broker?.child?.pid);
  if (state.exitedAt || state.exitCode !== null && state.exitCode !== undefined
    || state.signal !== null && state.signal !== undefined
    || broker?.child?.exitCode !== null && broker?.child?.exitCode !== undefined
    || broker?.child?.signalCode !== null && broker?.child?.signalCode !== undefined) return true;
  return !alive(broker?.child?.pid);
}

async function waitForBrokerExit(broker: CredentialBrokerHandle, timeoutMs: number,
  { sleep, now, alive }: { sleep: (ms: number) => Promise<void>; now: () => number; alive: Alive }): Promise<boolean> {
  const deadline = now() + timeoutMs;
  while (!brokerExited(broker, alive) && now() < deadline) await sleep(BROKER_DRAIN_POLL_MS);
  return brokerExited(broker, alive);
}

export function credentialBrokerDiagnostics(broker: CredentialBrokerHandle | null): BrokerDiagnostics | null {
  if (!broker) return null;
  if (broker.finalDiagnostics) return structuredClone(broker.finalDiagnostics);
  const state = broker.processState ?? {};
  return {
    schemaVersion: 1,
    endpointKind: broker.endpointKind ?? 'local-credential-broker',
    child: {
      pid: broker.child?.pid ?? null,
      exitCode: state.exitCode ?? broker.child?.exitCode ?? null,
      signal: state.signal ?? broker.child?.signalCode ?? null,
      exitedAt: state.exitedAt ?? null,
      stderrTail: state.stderrTail ? redactDiagnosticText(state.stderrTail, broker) : null,
      stderrTruncated: state.stderrTruncated === true,
    },
    drain: null,
    termination: null,
    ledger: null,
    errors: [],
  };
}

export async function stopCredentialBroker(broker: CredentialBrokerHandle | null, {
  drainTimeoutMs = BROKER_DRAIN_TIMEOUT_MS,
  pollMs = BROKER_DRAIN_POLL_MS,
  gracefulTimeoutMs = BROKER_STOP_GRACE_MS,
  forceTimeoutMs = BROKER_STOP_FORCE_MS,
  readLedger = readCredentialBrokerLedger,
  terminate = killTree,
  requestStop = (child: CredentialBrokerChild) => child.kill?.('SIGTERM') ?? false,
  alive = processAlive,
  sleep = wait,
  now = Date.now,
}: { drainTimeoutMs?: number; pollMs?: number; gracefulTimeoutMs?: number; forceTimeoutMs?: number;
  readLedger?: typeof readCredentialBrokerLedger; terminate?: typeof killTree;
  requestStop?: (child: CredentialBrokerChild) => boolean | void; alive?: Alive;
  sleep?: (ms: number) => Promise<void>; now?: () => number } = {}): Promise<BrokerLedger | null> {
  if (!broker) return null;
  if (broker.finalDiagnostics) return structuredClone(broker.finalLedger ?? null);
  let ledger: BrokerLedger | null = null;
  const startedAt = now();
  let drainTimedOut = false;
  let drainReason: string | null = null;
  const errors: BrokerError[] = [];
  const errorKeys = new Set();
  const recordError = (type: string, phase: string, error: unknown): void => {
    const message = redactDiagnosticText(error instanceof Error ? error.message : error, broker) || 'unknown error';
    const key = `${type}:${phase}:${message}`;
    if (errorKeys.has(key)) return;
    errorKeys.add(key);
    errors.push({ type, phase, message });
  };
  const read = (phase: string, expected: { model: string; maxBudgetUsd: number | null }): BrokerLedger | null => {
    try { return readLedger(broker.ledgerPath, expected); }
    catch (error) { recordError('ledger-read-error', phase, error); return null; }
  };
  let gracefulRequested = false;
  let forceRequested = false;
  let exited = brokerExited(broker, alive);
  const expected = { model: broker.model, maxBudgetUsd: broker.maxBudgetUsd };
  try {
    const deadline = now() + drainTimeoutMs;
    // Drain the final provider response before closing its cost reservation.
    while (drainReason === null) {
      ledger = read('drain', expected) ?? ledger;
      if (ledger?.complete === true) { drainReason = 'ledger-complete'; break; }
      exited = brokerExited(broker, alive);
      if (exited) { drainReason = 'child-exited'; break; }
      if (now() >= deadline) { drainTimedOut = true; drainReason = 'timeout'; break; }
      await sleep(pollMs);
    }
    exited = brokerExited(broker, alive);
    if (!exited) {
      gracefulRequested = true;
      try { requestStop(broker.child); }
      catch (error) { recordError('termination-error', 'graceful-request', error); }
      exited = await waitForBrokerExit(broker, gracefulTimeoutMs, { sleep, now, alive });
    }
    if (!exited) {
      forceRequested = true;
      try { terminate(broker.child.pid); }
      catch (error) { recordError('termination-error', 'force-request', error); }
      exited = await waitForBrokerExit(broker, forceTimeoutMs, { sleep, now, alive });
    }
    if (!exited) recordError('termination-error', 'exit-verification',
      new Error('credential broker remained alive after forced termination'));
    ledger = read('final', expected) ?? ledger;
  } catch (error) { recordError('broker-stop-error', 'shutdown', error); }
  finally {
    const state = broker.processState ?? {};
    if (exited) {
      try { rmSync(broker.root, { recursive: true, force: true }); }
      catch (error) { recordError('cleanup-error', 'private-root', error); }
    }
    broker.finalLedger = ledger;
    broker.finalDiagnostics = {
      ...(credentialBrokerDiagnostics(broker) as BrokerDiagnostics),
      child: {
        pid: broker.child?.pid ?? null,
        exitCode: state.exitCode ?? broker.child?.exitCode ?? null,
        signal: state.signal ?? broker.child?.signalCode ?? null,
        exitedAt: state.exitedAt ?? null,
        stderrTail: state.stderrTail ? redactDiagnosticText(state.stderrTail, broker) : null,
        stderrTruncated: state.stderrTruncated === true,
      },
      drain: {
        timeoutMs: drainTimeoutMs,
        elapsedMs: Math.max(0, now() - startedAt),
        timedOut: drainTimedOut,
        reason: drainReason,
        terminationRequested: gracefulRequested,
      },
      termination: { gracefulRequested, forceRequested, exited,
        gracefulTimeoutMs, forceTimeoutMs },
      ledger: ledgerDiagnostics(ledger),
      errors,
    };
  }
  return ledger;
}

function parseArgs(argv: string[]): string {
  const index = argv.indexOf('--config');
  const configPath = index === -1 ? undefined : argv[index + 1];
  if (!configPath || argv.length !== 2) fail('use --config <private-file>');
  return resolve(configPath);
}

async function main() {
  const configPath = parseArgs(process.argv.slice(2));
  let config: BrokerConfig;
  try { config = validateConfig(JSON.parse(readFileSync(configPath, 'utf8'))); }
  finally { rmSync(configPath, { force: true }); }
  if (!config.readyPath) fail('readyPath is invalid');
  const { server } = createCredentialBroker(config);
  const sockets = new Set<Socket>();
  server.on('connection', (socket: Socket) => {
    sockets.add(socket);
    socket.once('close', () => sockets.delete(socket));
  });
  server.on('error', (error: Error) => {
    process.stderr.write(`credential broker: ${error.message}\n`);
    process.exitCode = 1;
  });
  const readyPath = config.readyPath;
  server.listen(0, config.listenHost ?? '127.0.0.1', () => {
    const address: string | AddressInfo | null = server.address();
    if (!address || typeof address === 'string') fail('listener address is unavailable');
    writeFileSync(readyPath, `${JSON.stringify({ host: address.address, port: address.port })}\n`,
      { flag: 'wx', mode: 0o600 });
  });
  let stopping = false;
  const stop = () => {
    if (stopping) return;
    stopping = true;
    const force = setTimeout(() => {
      for (const socket of sockets) socket.destroy();
      server.closeAllConnections?.();
      process.exit(0);
    }, BROKER_SERVER_CLOSE_GRACE_MS);
    force.unref();
    server.close(() => {
      clearTimeout(force);
      process.exit(0);
    });
    server.closeIdleConnections?.();
  };
  const parentPid = config.parentPid;
  if (parentPid) {
    setInterval(() => {
      try { process.kill(parentPid, 0); }
      catch { stop(); }
    }, 1_000).unref();
  }
  const expiresAt = config.expiresAt;
  if (expiresAt) setTimeout(stop, Math.max(1, expiresAt - Date.now())).unref();
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error: unknown) => {
    process.stderr.write(`${error instanceof Error ? error.stack ?? error.message : String(error)}\n`);
    process.exit(1);
  });
}
