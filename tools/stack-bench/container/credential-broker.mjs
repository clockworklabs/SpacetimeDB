#!/usr/bin/env node
import { randomBytes } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { request as httpsRequest } from 'node:https';
import { chmodSync, existsSync, mkdtempSync, readFileSync, renameSync, rmSync,
  writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { brotliDecompressSync, gunzipSync, inflateSync } from 'node:zlib';

import { containerAuthSecret } from './container-auth.mjs';
import { killTree } from '../src/runtime/platform.mjs';
import { priceClaudeUsage } from '../src/evidence/claude-usage-cost.mjs';

const MAX_REQUEST_BYTES = 32 * 1024 * 1024;
const MAX_REQUESTS = 512;
const MAX_OUTPUT_TOKENS = 128_000;
const ALLOWED_PATHS = new Set(['/v1/messages', '/v1/messages/count_tokens']);
const PRICE_FIELDS = ['input', 'output', 'cacheWrite5m', 'cacheWrite1h', 'cacheRead'];
const LEDGER_SCHEMA_VERSION = 1;
const COST_TOLERANCE_USD = 0.0001;

function fail(message) {
  throw new Error(`credential broker: ${message}`);
}

function validateConfig(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail('configuration must be an object');
  if (!['api-key', 'subscription-token'].includes(value.mode)) fail('credential mode is invalid');
  for (const field of ['credential', 'sessionToken']) {
    if (typeof value[field] !== 'string' || value[field].length < 16) fail(`${field} is invalid`);
  }
  if (value.readyPath !== undefined && (typeof value.readyPath !== 'string' || !value.readyPath)) {
    fail('readyPath is invalid');
  }
  if (value.parentPid !== undefined && (!Number.isInteger(value.parentPid) || value.parentPid < 1)) {
    fail('parentPid is invalid');
  }
  if (value.expiresAt !== undefined && (!Number.isFinite(value.expiresAt) || value.expiresAt <= Date.now())) {
    fail('expiresAt is invalid');
  }
  if (value.listenHost !== undefined && !['127.0.0.1', '0.0.0.0'].includes(value.listenHost)) {
    fail('listenHost is invalid');
  }
  if (value.ledgerPath !== undefined && (typeof value.ledgerPath !== 'string'
    || !value.ledgerPath)) fail('ledgerPath is invalid');
  if (typeof value.model !== 'string' || !value.model) fail('model is invalid');
  if (!Number.isInteger(value.maxOutputTokens) || value.maxOutputTokens < 1
    || value.maxOutputTokens > MAX_OUTPUT_TOKENS) fail('maxOutputTokens is invalid');
  if (value.maxBudgetUsd !== null && value.maxBudgetUsd !== undefined) {
    if (!Number.isFinite(value.maxBudgetUsd) || value.maxBudgetUsd <= 0) {
      fail('maxBudgetUsd is invalid');
    }
    if (!value.pricingRates || typeof value.pricingRates !== 'object'
      || Array.isArray(value.pricingRates)) fail('pricingRates are required with maxBudgetUsd');
    for (const field of PRICE_FIELDS) {
      if (!Number.isFinite(value.pricingRates[field]) || value.pricingRates[field] < 0) {
        fail(`pricingRates.${field} is invalid`);
      }
    }
  }
  return value;
}

function validateLedger(value, { model = null, maxBudgetUsd = undefined } = {}) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail('spend ledger is invalid');
  const fields = new Set(['schemaVersion', 'model', 'maxBudgetUsd', 'acceptedRequests',
    'billableRequests', 'completedBillableRequests', 'spentUsd', 'reservedUsd',
    'complete', 'updatedAt']);
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`spend ledger.${key} is unknown`);
  if (value.schemaVersion !== LEDGER_SCHEMA_VERSION) fail('spend ledger schema is invalid');
  if (typeof value.model !== 'string' || !value.model) fail('spend ledger model is invalid');
  if (model !== null && value.model !== model) fail('spend ledger model does not match');
  if (value.maxBudgetUsd !== null
    && (!Number.isFinite(value.maxBudgetUsd) || value.maxBudgetUsd <= 0)) {
    fail('spend ledger budget is invalid');
  }
  if (maxBudgetUsd !== undefined && value.maxBudgetUsd !== maxBudgetUsd) {
    fail('spend ledger budget does not match');
  }
  for (const field of ['acceptedRequests', 'billableRequests', 'completedBillableRequests']) {
    if (!Number.isSafeInteger(value[field]) || value[field] < 0) {
      fail(`spend ledger.${field} is invalid`);
    }
  }
  if (value.completedBillableRequests > value.billableRequests) {
    fail('spend ledger completed request count is invalid');
  }
  for (const field of ['spentUsd', 'reservedUsd']) {
    if (!Number.isFinite(value[field]) || value[field] < 0) fail(`spend ledger.${field} is invalid`);
  }
  const complete = value.reservedUsd === 0
    && value.completedBillableRequests === value.billableRequests;
  if (value.complete !== complete) fail('spend ledger completion state is invalid');
  if (typeof value.updatedAt !== 'string' || Number.isNaN(Date.parse(value.updatedAt))) {
    fail('spend ledger timestamp is invalid');
  }
  return value;
}

function writeLedger(path, value) {
  if (!path) return;
  const ledger = validateLedger(value);
  const temporary = `${path}.${process.pid}.${randomBytes(8).toString('hex')}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(ledger)}\n`, { flag: 'wx', mode: 0o600 });
  try { renameSync(temporary, path); }
  catch (error) { rmSync(temporary, { force: true }); throw error; }
}

export function readCredentialBrokerLedger(path, expected = {}) {
  return validateLedger(JSON.parse(readFileSync(path, 'utf8')), expected);
}

export function reconcileCredentialBrokerReceipt({ ledger, cliResult, model, maxBudgetUsd,
  toleranceUsd = COST_TOLERANCE_USD } = {}) {
  if (typeof model !== 'string' || !model) fail('receipt model is invalid');
  if (!Number.isFinite(maxBudgetUsd) || maxBudgetUsd <= 0) fail('receipt budget is invalid');
  if (!Number.isFinite(toleranceUsd) || toleranceUsd < 0) fail('receipt tolerance is invalid');
  let verifiedLedger = null;
  let issue = null;
  try { verifiedLedger = validateLedger(ledger, { model, maxBudgetUsd }); }
  catch (error) { issue = error.message; }
  const brokerCost = verifiedLedger
    ? Math.min(maxBudgetUsd, verifiedLedger.spentUsd + verifiedLedger.reservedUsd)
    : maxBudgetUsd;
  const cliCost = Number(cliResult?.total_cost_usd);
  if (!issue && verifiedLedger.complete !== true) issue = 'credential broker spend ledger is incomplete';
  if (!issue && (!Number.isFinite(cliCost) || cliCost < 0)) {
    issue = 'coding session did not return a usable cost receipt';
  }
  if (!issue && Math.abs(cliCost - brokerCost) > toleranceUsd) {
    issue = `credential broker spend $${brokerCost.toFixed(6)} does not match CLI receipt $${cliCost.toFixed(6)}`;
  }
  const receipt = {
    schemaVersion: 1,
    source: 'credential-broker',
    model,
    costUsd: Number(brokerCost.toFixed(6)),
    cliCostUsd: Number.isFinite(cliCost) && cliCost >= 0 ? Number(cliCost.toFixed(6)) : null,
    complete: verifiedLedger?.complete === true,
    reconciled: issue === null,
    error: issue,
  };
  const result = cliResult && typeof cliResult === 'object' && !Array.isArray(cliResult)
    ? structuredClone(cliResult) : { type: 'result', is_error: true, result: '' };
  result.total_cost_usd = receipt.costUsd;
  result.stack_bench_cost_receipt = receipt;
  if (issue) {
    result.is_error = true;
    result.terminal_reason = 'cost_receipt_error';
    result.result = [typeof result.result === 'string' ? result.result.trim() : '', issue]
      .filter(Boolean).join('\n');
  }
  return { ok: issue === null, result, receipt };
}

function clientAuthorized(request, sessionToken) {
  return request.headers.authorization === `Bearer ${sessionToken}`
    || request.headers['x-api-key'] === sessionToken;
}

function upstreamHeaders(request, config) {
  const headers = { ...request.headers };
  delete headers.host;
  // Usage accounting reads a copy of the provider response. Ask for an
  // identity response so the accounting path and the client see the same
  // bytes. The decoder below still handles an encoded response if an upstream
  // proxy adds one.
  delete headers['accept-encoding'];
  delete headers.authorization;
  delete headers['proxy-authorization'];
  delete headers['x-api-key'];
  if (config.mode === 'api-key') headers['x-api-key'] = config.credential;
  else headers.authorization = `Bearer ${config.credential}`;
  return headers;
}

function requestPath(value) {
  try { return new URL(value, 'http://credential-broker.invalid').pathname; }
  catch { return null; }
}

function rejectRequest(request, response, status, message) {
  const send = () => {
    if (response.writableEnded) return;
    response.writeHead(status, { 'content-type': 'text/plain' });
    response.end(message);
  };
  request.on('error', () => {});
  if (request.complete) send();
  else {
    request.once('end', send);
    request.resume();
  }
}

function parseProviderRequest(body, path, config) {
  let payload;
  try { payload = JSON.parse(body.toString('utf8')); }
  catch { fail('request body must be valid JSON'); }
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) {
    fail('request body must be an object');
  }
  if (payload.model !== config.model) fail('request model does not match the selected model');
  if (path === '/v1/messages'
    && (!Number.isInteger(payload.max_tokens) || payload.max_tokens < 1
      || payload.max_tokens > config.maxOutputTokens)) {
    fail(`max_tokens must be from 1 through ${config.maxOutputTokens}`);
  }
  return payload;
}

function requestCostCeiling(bodyBytes, maxTokens, rates) {
  const inputRate = Math.max(rates.input, rates.cacheWrite5m, rates.cacheWrite1h);
  return bodyBytes * inputRate / 1e6 + maxTokens * rates.output / 1e6;
}

function decodedResponseBody(body, contentEncoding) {
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

function responseUsage(body, contentEncoding = null) {
  const values = [];
  const add = value => {
    if (!value || typeof value !== 'object') return;
    if (value.usage && typeof value.usage === 'object') values.push(value.usage);
    if (value.message?.usage && typeof value.message.usage === 'object') values.push(value.message.usage);
  };
  let text;
  try { text = decodedResponseBody(body, contentEncoding).toString('utf8'); }
  catch { return null; }
  try { add(JSON.parse(text)); }
  catch {
    for (const line of text.split(/\r?\n/)) {
      if (!line.startsWith('data:')) continue;
      const data = line.slice(5).trim();
      if (!data || data === '[DONE]') continue;
      try { add(JSON.parse(data)); } catch { /* Ignore non-JSON event data. */ }
    }
  }
  if (values.length === 0) return null;
  const number = field => Math.max(0, ...values.map(value => Number(value[field]) || 0));
  const cacheWrite = field => Math.max(0, ...values.map(value =>
    Number(value.cache_creation?.[field]) || 0));
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

export function createCredentialBroker(configInput, {
  requestUpstream = httpsRequest,
  upstream = { protocol: 'https:', hostname: 'api.anthropic.com', port: 443 },
  maxRequests = MAX_REQUESTS,
  maxRequestBytes = MAX_REQUEST_BYTES,
} = {}) {
  const config = validateConfig(configInput);
  let acceptedRequests = 0;
  let billableRequests = 0;
  let completedBillableRequests = 0;
  let spentUsd = 0;
  let reservedUsd = 0;
  const recordLedger = () => writeLedger(config.ledgerPath, {
    schemaVersion: LEDGER_SCHEMA_VERSION,
    model: config.model,
    maxBudgetUsd: config.maxBudgetUsd ?? null,
    acceptedRequests,
    billableRequests,
    completedBillableRequests,
    spentUsd: Number(spentUsd.toFixed(6)),
    reservedUsd: Number(reservedUsd.toFixed(6)),
    complete: reservedUsd === 0 && completedBillableRequests === billableRequests,
    updatedAt: new Date().toISOString(),
  });
  recordLedger();
  const server = createServer((request, response) => {
    if (!clientAuthorized(request, config.sessionToken)) {
      rejectRequest(request, response, 401, 'unauthorized');
      return;
    }
    const path = requestPath(request.url);
    if (request.method !== 'POST' || !ALLOWED_PATHS.has(path)) {
      rejectRequest(request, response, 404, 'not found');
      return;
    }
    acceptedRequests += 1;
    recordLedger();
    if (acceptedRequests > maxRequests) {
      rejectRequest(request, response, 429, 'session request limit reached');
      return;
    }

    const chunks = [];
    let received = 0;
    let tooLarge = false;
    request.on('data', chunk => {
      if (tooLarge) return;
      received += chunk.length;
      if (received > maxRequestBytes) {
        tooLarge = true;
        if (!response.headersSent) response.writeHead(413, { 'content-type': 'text/plain' });
        response.end('request is too large');
        return;
      }
      chunks.push(chunk);
    });
    request.on('end', () => {
      if (tooLarge) return;
      const body = Buffer.concat(chunks);
      let payload;
      try { payload = parseProviderRequest(body, path, config); }
      catch (error) {
        response.writeHead(400, { 'content-type': 'text/plain' });
        response.end(error.message);
        return;
      }
      const billable = path === '/v1/messages' && config.maxBudgetUsd != null;
      const costCeiling = billable
        ? requestCostCeiling(received, payload.max_tokens, config.pricingRates) : 0;
      if (billable && spentUsd + reservedUsd + costCeiling > config.maxBudgetUsd) {
        response.writeHead(402, { 'content-type': 'text/plain' });
        response.end('session cost limit reached');
        return;
      }
      if (billable) billableRequests += 1;
      reservedUsd += costCeiling;
      recordLedger();
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
        response.writeHead(upstreamResponse.statusCode ?? 502, upstreamResponse.headers);
        const responseChunks = [];
        let responseBytes = 0;
        upstreamResponse.on('data', chunk => {
          responseBytes += chunk.length;
          if (responseBytes <= maxRequestBytes) responseChunks.push(chunk);
          response.write(chunk);
        });
        upstreamResponse.on('end', () => {
          response.end();
          if (!billable) return;
          reservedUsd -= costCeiling;
          completedBillableRequests += 1;
          if ((upstreamResponse.statusCode ?? 502) >= 200
            && (upstreamResponse.statusCode ?? 502) < 300) {
            const usage = responseBytes <= maxRequestBytes
              ? responseUsage(Buffer.concat(responseChunks), upstreamResponse.headers['content-encoding'])
              : null;
            if (!usage) spentUsd += costCeiling;
            else {
              try { spentUsd += priceClaudeUsage(usage, config.pricingRates); }
              catch { spentUsd += costCeiling; }
            }
          }
          recordLedger();
        });
      });
      upstreamRequest.on('error', () => {
        if (billable) {
          reservedUsd -= costCeiling;
          spentUsd += costCeiling;
          completedBillableRequests += 1;
          recordLedger();
        }
        if (!response.headersSent) response.writeHead(502, { 'content-type': 'text/plain' });
        response.end('upstream request failed');
      });
      upstreamRequest.end(body);
    });
  });
  return { server, stats: () => ({ acceptedRequests,
    billableRequests, completedBillableRequests,
    spentUsd: Number(spentUsd.toFixed(6)), reservedUsd: Number(reservedUsd.toFixed(6)) }) };
}

export function startCredentialBroker(selectedAuth, { networkMode, deadlineMs,
  model, maxOutputTokens = MAX_OUTPUT_TOKENS, maxBudgetUsd = null, pricingRates = null,
  env = process.env } = {}) {
  if (!['bridge', 'host'].includes(networkMode)) fail('network mode is invalid');
  if (!Number.isFinite(deadlineMs) || deadlineMs <= 0) fail('deadline is invalid');
  const credential = containerAuthSecret(selectedAuth);
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-credential-broker-'));
  let child = null;
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
    child = spawn(process.execPath, [resolve(import.meta.dirname, 'credential-broker.mjs'),
      '--config', configPath], {
      stdio: ['ignore', 'ignore', 'inherit'],
      windowsHide: true,
      env: Object.fromEntries(['PATH', 'Path', 'SystemRoot', 'WINDIR', 'SSL_CERT_FILE',
        'NODE_EXTRA_CA_CERTS', 'HTTPS_PROXY', 'HTTP_PROXY']
        .filter(name => env[name] !== undefined).map(name => [name, env[name]])),
    });
    let spawnError = null;
    child.once('error', error => { spawnError = error; });
    const readyDeadline = Date.now() + 10_000;
    while (!spawnError && child.exitCode === null && !existsSync(readyPath)
      && Date.now() < readyDeadline) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100);
    }
    if (spawnError) throw spawnError;
    if (!existsSync(readyPath)) throw new Error('credential broker did not become ready');
    const ready = JSON.parse(readFileSync(readyPath, 'utf8'));
    if (!Number.isInteger(ready.port) || ready.port < 1 || ready.port > 65535) {
      throw new Error('credential broker returned an invalid port');
    }
    if (ready.host !== listenHost) throw new Error('credential broker returned an invalid host');
    const host = networkMode === 'host' ? '127.0.0.1' : 'host.docker.internal';
    return { child, root, ledgerPath, model, maxBudgetUsd: maxBudgetUsd ?? null,
      sessionToken, baseUrl: `http://${host}:${ready.port}`, listenHost: ready.host };
  } catch (error) {
    if (child?.pid) killTree(child.pid);
    rmSync(root, { recursive: true, force: true });
    throw error;
  }
}

export function stopCredentialBroker(broker) {
  if (!broker) return null;
  killTree(broker.child.pid);
  let ledger = null;
  try {
    ledger = readCredentialBrokerLedger(broker.ledgerPath,
      { model: broker.model, maxBudgetUsd: broker.maxBudgetUsd });
  } catch { /* The caller treats a missing or invalid ledger as a full-cap receipt. */ }
  finally { rmSync(broker.root, { recursive: true, force: true }); }
  return ledger;
}

function parseArgs(argv) {
  const index = argv.indexOf('--config');
  if (index === -1 || !argv[index + 1] || argv.length !== 2) fail('use --config <private-file>');
  return resolve(argv[index + 1]);
}

async function main() {
  const configPath = parseArgs(process.argv.slice(2));
  let config;
  try { config = validateConfig(JSON.parse(readFileSync(configPath, 'utf8'))); }
  finally { rmSync(configPath, { force: true }); }
  const { server } = createCredentialBroker(config);
  server.on('error', error => {
    process.stderr.write(`credential broker: ${error.message}\n`);
    process.exitCode = 1;
  });
  server.listen(0, config.listenHost ?? '127.0.0.1', () => {
    const address = server.address();
    if (!address || typeof address === 'string') fail('listener address is unavailable');
    writeFileSync(config.readyPath, `${JSON.stringify({ host: address.address, port: address.port })}\n`,
      { flag: 'wx', mode: 0o600 });
  });
  let stopping = false;
  const stop = () => {
    if (stopping) return;
    stopping = true;
    server.close(() => process.exit(0));
  };
  if (config.parentPid) {
    setInterval(() => {
      try { process.kill(config.parentPid, 0); }
      catch { stop(); }
    }, 1_000).unref();
  }
  if (config.expiresAt) setTimeout(stop, Math.max(1, config.expiresAt - Date.now())).unref();
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main().catch(error => {
    process.stderr.write(`${error.stack ?? error.message}\n`);
    process.exit(1);
  });
}
