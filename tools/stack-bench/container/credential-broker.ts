#!/usr/bin/env node
import { createServer } from 'node:http';
import type { ClientRequest, IncomingMessage, OutgoingHttpHeaders, ServerResponse } from 'node:http';
import { request as httpsRequest } from 'node:https';
import type { RequestOptions } from 'node:https';
import type { Socket } from 'node:net';
import { readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs as parseNodeArgs } from 'node:util';
import type { AddressInfo } from 'node:net';
import { classifyProviderFailure } from '../src/agents/provider-failure.js';
import type { ProviderFailure } from '../src/agents/provider-failure.js';
import { brokerProtocol } from './broker-protocols.js';

import { normalizeClaudeUsage } from '../src/evidence/claude-usage-cost.js';
import type { ClaudeUsage } from '../src/evidence/claude-usage-cost.js';
import { BROKER_LEDGER_SCHEMA_VERSION, CLAUDE_USAGE_FIELDS, noEstimates, priceNormalizedClaudeUsage,
  validateBrokerConfig,
  writeCredentialBrokerLedger } from './credential-broker-accounting.js';
import type { BrokerConfig, EstimateReason, PricingRates }
  from './credential-broker-accounting.js';

const MAX_REQUEST_BYTES = 32 * 1024 * 1024;
const BROKER_SERVER_CLOSE_GRACE_MS = 1_000;
export type { ClaudeUsage } from '../src/evidence/claude-usage-cost.js';
type JsonRecord = Record<string, unknown>;
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

type UpstreamRequest = (options: RequestOptions,
  callback: (response: IncomingMessage) => void) => ClientRequest;

const roundUsd = (value: number): number => Number(value.toFixed(6));
const reserveUsd = (value: number): number => Math.ceil(value * 1e6) / 1e6;

function fail(message: string): never {
  throw new Error(`credential broker: ${message}`);
}

function clientAuthorized(request: IncomingMessage, sessionToken: string): boolean {
  return request.headers.authorization === `Bearer ${sessionToken}`
    || request.headers['x-api-key'] === sessionToken;
}

function requestPath(value: string | undefined): string | null {
  try { return new URL(value ?? '', 'http://credential-broker.invalid').pathname; }
  catch { return null; }
}

function rejectRequest(request: IncomingMessage, response: ServerResponse,
  status: number, message: string): void {
  request.on('error', () => {});
  response.on('error', () => {});
  try {
    response.shouldKeepAlive = false;
    response.writeHead(status, { 'content-type': 'text/plain', connection: 'close' });
    response.end(message);
  } catch { response.destroy(); }
  request.resume();
}

function requestCostCeiling(bodyBytes: number, maxTokens: number, rates: PricingRates): number {
  const inputRate = Math.max(rates.input, rates.cacheRead, rates.cacheWrite5m, rates.cacheWrite1h);
  return bodyBytes * inputRate / 1e6 + maxTokens * rates.output / 1e6;
}

export function createCredentialBroker(configInput: unknown, {
  requestUpstream = httpsRequest as UpstreamRequest,
  upstream,
  maxRequestBytes = MAX_REQUEST_BYTES,
}: { requestUpstream?: UpstreamRequest;
  upstream?: { protocol: string; hostname: string; port: number };
  maxRequestBytes?: number } = {}): CreatedCredentialBroker {
  const config = validateBrokerConfig(configInput);
  const protocol = brokerProtocol(config);
  const destination = upstream ?? { protocol: 'https:', hostname: protocol.hostname, port: 443 };
  let acceptedRequests = 0;
  let lastResponseRequest = 0;
  let providerFailure: ProviderFailure | null = null;
  const recordFailure = (request: number, failure: ProviderFailure | null): void => {
    if (request >= lastResponseRequest) { lastResponseRequest = request; providerFailure = failure; }
  };
  let billableRequests = 0;
  let completedBillableRequests = 0;
  let estimatedBillableRequests = 0;
  const estimatedByReason = noEstimates();
  let spentUsd = 0;
  let providerReportedCostUsd = 0;
  let providerIntegrityError: string | undefined;
  const upstreamProviders = new Set<string>();
  let reservedUsd = 0;
  const usageTotals: ClaudeUsage = { input: 0, output: 0, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 };
  const recordLedger = () => writeCredentialBrokerLedger(config.ledgerPath, {
    schemaVersion: BROKER_LEDGER_SCHEMA_VERSION,
    ...(config.provider === 'openrouter' ? { provider: config.provider, providerRoute: config.providerRoute,
      providerReportedCostUsd: roundUsd(providerReportedCostUsd), upstreamProviders: [...upstreamProviders],
      ...(providerIntegrityError ? { providerIntegrityError } : {}) } : {}),
    providerFailure,
    model: config.model,
    maxBudgetUsd: config.maxBudgetUsd ?? null,
    acceptedRequests,
    billableRequests,
    completedBillableRequests,
    estimatedBillableRequests,
    estimatedByReason,
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
    if (providerIntegrityError) {
      rejectRequest(request, response, 502, 'provider accounting or routing validation failed');
      return;
    }
    const path = requestPath(request.url);
    if (request.method !== 'POST' || path === null || !protocol.allowedPaths.has(path)) {
      recordFailure(acceptedRequests + 1, { category: 'request', status: 404, code: 'broker-path' });
      recordLedger();
      rejectRequest(request, response, 404, 'not found');
      return;
    }
    acceptedRequests += 1;
    const requestOrdinal = acceptedRequests;
    recordLedger();

    const chunks: Buffer[] = [];
    let received = 0;
    let tooLarge = false;
    request.on('data', (chunk: Buffer) => {
      if (tooLarge) return;
      received += chunk.length;
      if (received > maxRequestBytes) {
        tooLarge = true;
        recordFailure(requestOrdinal, { category: 'request', status: 413, code: 'broker-body-limit' });
        recordLedger();
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
      try { payload = protocol.parseRequest(body, path); }
      catch {
        recordFailure(requestOrdinal, { category: 'request', status: 400, code: 'broker-request-invalid' });
        recordLedger();
        writeHead(400, { 'content-type': 'text/plain' });
        endResponse('invalid provider request');
        return;
      }
      const billable = protocol.billable(path) && config.maxBudgetUsd != null;
      const costCeiling = billable
        ? reserveUsd(requestCostCeiling(received + (protocol.inputTokenAdjustment?.(payload) ?? 0), protocol.outputLimit(payload),
          config.pricingRates as PricingRates)) : 0;
      const budget = config.maxBudgetUsd;
      if (billable && budget !== null && budget !== undefined
        && spentUsd + reservedUsd + costCeiling > budget) {
        const measuredSpend = config.provider === 'openrouter' ? providerReportedCostUsd
          : priceNormalizedClaudeUsage(usageTotals, config.pricingRates as PricingRates);
        recordFailure(requestOrdinal, { category: 'broker-budget', status: 402, code: 'reservation-exceeds-budget',
          budget: { maxBudgetUsd: budget, spentUsd, reservedUsd, requestCeilingUsd: costCeiling,
            estimatedSpendUsd: roundUsd(Math.max(0, spentUsd - measuredSpend)) } });
        recordLedger();
        writeHead(402, { 'content-type': 'text/plain' });
        endResponse('session budget cannot cover the next request reservation');
        return;
      }
      if (billable) billableRequests += 1;
      reservedUsd = roundUsd(reservedUsd + costCeiling);
      recordLedger();
      let billableSettled = !billable;
      const settleBillable = ({ usage = null, estimated = null, reportedCost = null }:
        { usage?: ClaudeUsage | null; estimated?: EstimateReason | null; reportedCost?: number | null } = {}): void => {
        if (billableSettled) return;
        billableSettled = true;
        reservedUsd = roundUsd(reservedUsd - costCeiling);
        completedBillableRequests += 1;
        if (estimated) {
          estimatedBillableRequests += 1;
          estimatedByReason[estimated] += 1;
          spentUsd = roundUsd(spentUsd + costCeiling);
        } else if (usage) {
          spentUsd = roundUsd(spentUsd + (reportedCost ?? priceNormalizedClaudeUsage(usage, config.pricingRates as PricingRates)));
          if (reportedCost !== null) {
            providerReportedCostUsd = roundUsd(providerReportedCostUsd + reportedCost);
            if (reportedCost > costCeiling + 0.000001) {
              providerIntegrityError = 'OpenRouter reported cost exceeds the request reservation';
            }
          }
          for (const field of CLAUDE_USAGE_FIELDS) usageTotals[field] += usage[field];
        }
        recordLedger();
      };
      const headers = protocol.headers(request);
      for (const name of ['connection', 'keep-alive', 'proxy-connection', 'te', 'trailer',
        'transfer-encoding', 'upgrade']) delete headers[name];
      const forwardedBody = Buffer.from(JSON.stringify(payload));
      headers['content-length'] = String(forwardedBody.length);
      const upstreamRequest = requestUpstream({
        protocol: destination.protocol,
        hostname: destination.hostname,
        port: destination.port,
        method: request.method,
        path: protocol.upstreamPath(path + new URL(request.url ?? '', 'http://credential-broker.invalid').search),
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
          const status = upstreamResponse.statusCode ?? 502;
          recordFailure(requestOrdinal, status >= 200 && status < 300 ? null
            : classifyProviderFailure(status, Buffer.concat(responseChunks)));
          recordLedger();
          endResponse();
          if (!billable) return;
          if ((upstreamResponse.statusCode ?? 502) >= 200
            && (upstreamResponse.statusCode ?? 502) < 300) {
            const usage = responseBytes <= maxRequestBytes
              ? protocol.responseUsage(Buffer.concat(responseChunks), upstreamResponse.headers['content-encoding'])
              : null;
            if (!usage) {
              if (config.provider === 'openrouter') providerIntegrityError = 'OpenRouter response lacks verified cost and routing metadata';
              recordFailure(requestOrdinal, { category: 'transport', status, code: 'incomplete-response' });
              settleBillable({ estimated: 'no-usage' });
            }
            else try {
              if (typeof usage.upstream_provider === 'string') upstreamProviders.add(usage.upstream_provider);
              settleBillable({ usage: normalizeClaudeUsage(usage),
                reportedCost: typeof usage.provider_reported_cost_usd === 'number' ? usage.provider_reported_cost_usd : null });
            }
            catch {
              recordFailure(requestOrdinal, { category: 'transport', status, code: 'invalid-usage' });
              settleBillable({ estimated: 'no-usage' });
            }
          } else {
            settleBillable();
          }
        });
        const settleAbortedResponse = () => {
          recordFailure(requestOrdinal, { category: 'transport', status: null, code: null });
          settleBillable({ estimated: 'response-aborted' });
          if (responseOpen()) response.destroy();
        };
        upstreamResponse.once('aborted', settleAbortedResponse);
        upstreamResponse.once('error', settleAbortedResponse);
      });
      upstreamRequest.on('error', () => {
        recordFailure(requestOrdinal, { category: 'transport', status: null, code: null });
        settleBillable({ estimated: 'upstream-error' });
        writeHead(502, { 'content-type': 'text/plain' });
        endResponse('upstream request failed');
      });
      upstreamRequest.end(forwardedBody);
    });
  });
  server.on('clientError', (_error: Error, socket: Socket) => {
    socket.on('error', () => {});
    if (socket.writable) socket.end('HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n');
    else socket.destroy();
  });
  return { server, stats: () => ({ acceptedRequests,
    billableRequests, completedBillableRequests, estimatedBillableRequests,
    estimatedByReason: { ...estimatedByReason },
    spentUsd: Number(spentUsd.toFixed(6)), reservedUsd: Number(reservedUsd.toFixed(6)) }) };
}

function parseArgs(argv: string[]): string {
  const { values } = parseNodeArgs({ args: argv, options: { config: { type: 'string' } } });
  const configPath = values.config;
  if (!configPath || argv.length !== 2) fail('use --config <private-file>');
  return resolve(configPath);
}

async function main() {
  const configPath = parseArgs(process.argv.slice(2));
  let config: BrokerConfig;
  try { config = validateBrokerConfig(JSON.parse(readFileSync(configPath, 'utf8'))); }
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
