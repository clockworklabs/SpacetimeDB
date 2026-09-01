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
import { brotliDecompressSync, gunzipSync, inflateSync } from 'node:zlib';

import { normalizeClaudeUsage } from '../src/evidence/claude-usage-cost.js';
import type { ClaudeUsage } from '../src/evidence/claude-usage-cost.js';
import { BROKER_LEDGER_SCHEMA_VERSION, CLAUDE_USAGE_FIELDS, priceNormalizedClaudeUsage,
  validateBrokerConfig,
  writeCredentialBrokerLedger } from './credential-broker-accounting.js';
import type { BrokerConfig, PricingRates }
  from './credential-broker-accounting.js';

const MAX_REQUEST_BYTES = 32 * 1024 * 1024;
const MAX_REQUESTS = 512;
const ALLOWED_PATHS = new Set(['/v1/messages', '/v1/messages/count_tokens']);
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

function isRecord(value: unknown): value is JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isNumber(value: unknown): value is number {
  return typeof value === 'number';
}

const roundUsd = (value: number): number => Number(value.toFixed(6));
const reserveUsd = (value: number): number => Math.ceil(value * 1e6) / 1e6;

function fail(message: string): never {
  throw new Error(`credential broker: ${message}`);
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
  request.on('error', () => {});
  response.on('error', () => {});
  try {
    response.shouldKeepAlive = false;
    response.writeHead(status, { 'content-type': 'text/plain', connection: 'close' });
    response.end(message);
  } catch { response.destroy(); }
  request.resume();
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
  const config = validateBrokerConfig(configInput);
  let acceptedRequests = 0;
  let billableRequests = 0;
  let completedBillableRequests = 0;
  let estimatedBillableRequests = 0;
  let spentUsd = 0;
  let reservedUsd = 0;
  const usageTotals: ClaudeUsage = { input: 0, output: 0, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 };
  const recordLedger = () => writeCredentialBrokerLedger(config.ledgerPath, {
    schemaVersion: BROKER_LEDGER_SCHEMA_VERSION,
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
      catch {
        writeHead(400, { 'content-type': 'text/plain' });
        endResponse('invalid provider request');
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
          spentUsd = roundUsd(spentUsd + priceNormalizedClaudeUsage(usage, config.pricingRates as PricingRates));
          for (const field of CLAUDE_USAGE_FIELDS) usageTotals[field] += usage[field];
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
