#!/usr/bin/env node
import { randomBytes } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { request as httpsRequest } from 'node:https';
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { containerAuthSecret } from './container-auth.mjs';
import { killTree } from '../src/runtime/platform.mjs';

const MAX_REQUEST_BYTES = 32 * 1024 * 1024;
const MAX_REQUESTS = 512;

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
  return value;
}

function clientAuthorized(request, sessionToken) {
  return request.headers.authorization === `Bearer ${sessionToken}`
    || request.headers['x-api-key'] === sessionToken;
}

function upstreamHeaders(request, config) {
  const headers = { ...request.headers };
  delete headers.host;
  delete headers.authorization;
  delete headers['proxy-authorization'];
  delete headers['x-api-key'];
  if (config.mode === 'api-key') headers['x-api-key'] = config.credential;
  else headers.authorization = `Bearer ${config.credential}`;
  return headers;
}

export function createCredentialBroker(configInput, {
  requestUpstream = httpsRequest,
  upstream = { protocol: 'https:', hostname: 'api.anthropic.com', port: 443 },
  maxRequests = MAX_REQUESTS,
  maxRequestBytes = MAX_REQUEST_BYTES,
} = {}) {
  const config = validateConfig(configInput);
  let acceptedRequests = 0;
  const server = createServer((request, response) => {
    if (!clientAuthorized(request, config.sessionToken)) {
      response.writeHead(401, { 'content-type': 'text/plain' });
      response.end('unauthorized');
      return;
    }
    if (request.method !== 'POST' || !request.url?.startsWith('/v1/')) {
      response.writeHead(404, { 'content-type': 'text/plain' });
      response.end('not found');
      return;
    }
    acceptedRequests += 1;
    if (acceptedRequests > maxRequests) {
      response.writeHead(429, { 'content-type': 'text/plain' });
      response.end('session request limit reached');
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
        upstreamResponse.pipe(response);
      });
      upstreamRequest.on('error', () => {
        if (!response.headersSent) response.writeHead(502, { 'content-type': 'text/plain' });
        response.end('upstream request failed');
      });
      upstreamRequest.end(Buffer.concat(chunks));
    });
  });
  return { server, stats: () => ({ acceptedRequests }) };
}

export function startCredentialBroker(selectedAuth, { networkMode, deadlineMs,
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
    const sessionToken = randomBytes(32).toString('hex');
    writeFileSync(configPath, `${JSON.stringify({ schemaVersion: 1,
      mode: selectedAuth.mode, credential, sessionToken, readyPath, parentPid: process.pid,
      expiresAt: Date.now() + deadlineMs + 60_000 })}\n`, { flag: 'wx', mode: 0o600 });
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
    const host = networkMode === 'host' ? '127.0.0.1' : 'host.docker.internal';
    return { child, root, sessionToken, baseUrl: `http://${host}:${ready.port}` };
  } catch (error) {
    if (child?.pid) killTree(child.pid);
    rmSync(root, { recursive: true, force: true });
    throw error;
  }
}

export function stopCredentialBroker(broker) {
  if (!broker) return;
  killTree(broker.child.pid);
  rmSync(broker.root, { recursive: true, force: true });
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
  server.listen(0, '0.0.0.0', () => {
    const address = server.address();
    if (!address || typeof address === 'string') fail('listener address is unavailable');
    writeFileSync(config.readyPath, `${JSON.stringify({ port: address.port })}\n`,
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
