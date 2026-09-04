import { spawn } from 'node:child_process';
import type { ChildProcess } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as wait } from 'node:timers/promises';

import type { ContainerAuth } from './container-auth.js';
import { MAX_BROKER_OUTPUT_TOKENS, readCredentialBrokerLedger, validateBrokerConfig }
  from './credential-broker-accounting.js';
import type { BrokerLedger, PricingRates } from './credential-broker-accounting.js';
import { compiledEntrypoint } from '../src/package-root.js';
import { killTree } from '../src/runtime/platform.js';

const BROKER_DRAIN_TIMEOUT_MS = 30_000;
const BROKER_DRAIN_POLL_MS = 100;
const BROKER_STDERR_LIMIT_BYTES = 16 * 1024;
const BROKER_STOP_GRACE_MS = 2_000;
const BROKER_STOP_FORCE_MS = 2_000;

type JsonRecord = Record<string, unknown>;
type Alive = (pid: number | undefined) => boolean;

export type BrokerProcessState = {
  exitCode: number | null;
  signal: NodeJS.Signals | null;
  exitedAt: string | null;
  stderrTail: string;
  stderrPending: string;
  stderrTruncated: boolean;
};

export type BrokerError = { type: string; phase: string; message: string };

export type BrokerDiagnostics = {
  schemaVersion: number;
  endpointKind: string;
  child: { pid: number | undefined | null; exitCode: number | null; signal: NodeJS.Signals | null;
    exitedAt: string | null; stderrTail: string | null; stderrTruncated: boolean };
  drain: { timeoutMs: number; elapsedMs: number; timedOut: boolean; reason: string | null;
    terminationRequested: boolean } | null;
  termination: { gracefulRequested: boolean; forceRequested: boolean; exited: boolean;
    gracefulTimeoutMs: number; forceTimeoutMs: number } | null;
  ledger: BrokerLedger | null;
  errors: BrokerError[];
};

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

function isRecord(value: unknown): value is JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function fail(message: string): never {
  throw new Error(`credential broker: ${message}`);
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
      for (let length = Math.min(secret.length - 1, redacted.length); length > pendingLength; length -= 1) {
        if (redacted.endsWith(secret.slice(0, length))) { pendingLength = length; break; }
      }
    }
  } else if (raw && secrets.some(secret => secret.startsWith(raw))) redacted = '[REDACTED]';
  state.stderrPending = pendingLength ? redacted.slice(-pendingLength) : '';
  const safe = pendingLength ? redacted.slice(0, -pendingLength) : redacted;
  const next = state.stderrTail + safe;
  if (Buffer.byteLength(next) > BROKER_STDERR_LIMIT_BYTES) {
    state.stderrTruncated = true;
    state.stderrTail = Buffer.from(next).subarray(-BROKER_STDERR_LIMIT_BYTES).toString('utf8');
  } else state.stderrTail = next;
}

export async function startCredentialBroker(selectedAuth: ContainerAuth, { networkMode, deadlineMs,
  model, maxOutputTokens = MAX_BROKER_OUTPUT_TOKENS, maxBudgetUsd = null, pricingRates = null,
  env = process.env }: { networkMode: 'bridge' | 'host'; deadlineMs: number; model: string;
  maxOutputTokens?: number; maxBudgetUsd?: number | null; pricingRates?: PricingRates | null;
  env?: NodeJS.ProcessEnv }): Promise<CredentialBroker> {
  if (!['bridge', 'host'].includes(networkMode)) fail('network mode is invalid');
  if (!Number.isFinite(deadlineMs) || deadlineMs <= 0) fail('deadline is invalid');
  const credential = selectedAuth.credential.trim();
  if (!credential) fail('selected authentication has no broker credential');
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
    const config = validateBrokerConfig({ mode: selectedAuth.mode, credential, sessionToken, readyPath,
      parentPid: process.pid, expiresAt: Date.now() + deadlineMs + 60_000, listenHost, ledgerPath,
      model, maxOutputTokens, maxBudgetUsd, pricingRates });
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
    while (!spawnError && child.exitCode === null && !existsSync(readyPath) && Date.now() < readyDeadline) {
      await wait(100);
    }
    if (spawnError) throw spawnError;
    if (!existsSync(readyPath)) throw new Error('credential broker did not become ready');
    const ready: unknown = JSON.parse(readFileSync(readyPath, 'utf8'));
    if (!isRecord(ready) || typeof ready.port !== 'number' || !Number.isInteger(ready.port)
      || ready.port < 1 || ready.port > 65_535) throw new Error('credential broker returned an invalid port');
    if (ready.host !== listenHost) throw new Error('credential broker returned an invalid host');
    const host = networkMode === 'host' ? '127.0.0.1' : 'host.docker.internal';
    return { child, root, ledgerPath, model, maxBudgetUsd: maxBudgetUsd ?? null,
      sessionToken, baseUrl: `http://${host}:${ready.port}`, listenHost,
      endpointKind: 'local-credential-broker', processState,
      diagnosticSecrets, finalDiagnostics: null, finalLedger: null };
  } catch (error) {
    if (child?.pid) killTree(child.pid);
    rmSync(root, { recursive: true, force: true });
    throw error;
  }
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
    child: { pid: broker.child?.pid ?? null,
      exitCode: state.exitCode ?? broker.child?.exitCode ?? null,
      signal: state.signal ?? broker.child?.signalCode ?? null,
      exitedAt: state.exitedAt ?? null,
      stderrTail: state.stderrTail ? redactDiagnosticText(state.stderrTail, broker) : null,
      stderrTruncated: state.stderrTruncated === true },
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
  const errorKeys = new Set<string>();
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
      child: { pid: broker.child?.pid ?? null,
        exitCode: state.exitCode ?? broker.child?.exitCode ?? null,
        signal: state.signal ?? broker.child?.signalCode ?? null,
        exitedAt: state.exitedAt ?? null,
        stderrTail: state.stderrTail ? redactDiagnosticText(state.stderrTail, broker) : null,
        stderrTruncated: state.stderrTruncated === true },
      drain: { timeoutMs: drainTimeoutMs, elapsedMs: Math.max(0, now() - startedAt),
        timedOut: drainTimedOut, reason: drainReason, terminationRequested: gracefulRequested },
      termination: { gracefulRequested, forceRequested, exited, gracefulTimeoutMs, forceTimeoutMs },
      ledger: ledger ? structuredClone(ledger) : null,
      errors,
    };
  }
  return ledger;
}
