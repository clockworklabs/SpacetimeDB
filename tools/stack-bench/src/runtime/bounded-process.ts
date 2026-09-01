import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { closeSync, constants, mkdirSync, openSync, rmSync, writeSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

import { killTree } from './platform.js';

export interface CapturedProcessLog {
  path: string;
  sha256: string;
  bytes: number;
  retainedBytes: number;
  truncated: boolean;
}

export interface BoundedProcessResult {
  ok: boolean;
  code: number | null;
  signal: NodeJS.Signals | null;
  timedOut: boolean;
  cancelled: boolean;
  error: Error | null;
  logs: Record<string, CapturedProcessLog> | null;
  stdoutTail: string;
  stderrTail: string;
}

export interface RunBoundedOptions {
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  stdio?: 'inherit' | 'ignore';
  timeoutMs: number;
  logs?: { stdout: string; stderr: string; maxBytes?: number } | null;
  signal?: AbortSignal | null;
}

type StreamName = 'stdout' | 'stderr';

interface CaptureState {
  path: string;
  fd: number;
  bytes: number;
  retainedBytes: number;
  hash: ReturnType<typeof createHash>;
  tail: string;
}

function openCapture(path: string): CaptureState {
  mkdirSync(dirname(path), { recursive: true });
  const flags = constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY
    | (constants.O_NOFOLLOW ?? 0);
  return { path, fd: openSync(path, flags, 0o600), bytes: 0, retainedBytes: 0,
    hash: createHash('sha256'), tail: '' };
}

export function runBounded(command: string, argv: readonly string[],
  { cwd = process.cwd(), env = process.env, stdio = 'inherit', timeoutMs,
    terminate = killTree, logs = null, signal = null, gracefulCancellationMs = 0 }:
    RunBoundedOptions & {
      terminate?: (pid: number) => void; gracefulCancellationMs?: number;
    }): Promise<BoundedProcessResult> {
  return new Promise(resolveRun => {
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs <= 0) {
      throw new Error('runBounded timeoutMs must be a positive safe integer');
    }
    if (!Number.isSafeInteger(gracefulCancellationMs) || gracefulCancellationMs < 0) {
      throw new Error('runBounded gracefulCancellationMs must be a non-negative safe integer');
    }
    const maximum = logs?.maxBytes ?? 4 * 1024 * 1024;
    if (logs && (!Number.isInteger(maximum) || maximum <= 0)) {
      throw new Error('runBounded logs.maxBytes must be a positive integer');
    }
    if (logs) {
      for (const name of ['stdout', 'stderr'] as const) {
        if (typeof logs[name] !== 'string' || !logs[name]) {
          throw new Error(`runBounded logs.${name} must be a path`);
        }
      }
      if (resolve(logs.stdout) === resolve(logs.stderr)) {
        throw new Error('runBounded stdout and stderr logs must use different paths');
      }
    }
    let streams: Record<StreamName, CaptureState> | null = null;
    if (logs) {
      const stdout = openCapture(resolve(logs.stdout));
      try {
        streams = { stdout, stderr: openCapture(resolve(logs.stderr)) };
      } catch (error) {
        closeSync(stdout.fd);
        throw error;
      }
    }
    let child: ReturnType<typeof spawn>;
    try {
      child = spawn(command, argv, { cwd, env,
        stdio: streams ? ['inherit', 'pipe', 'pipe'] : stdio });
    } catch (error) {
      if (streams) for (const stream of Object.values(streams)) {
        closeSync(stream.fd);
        rmSync(stream.path, { force: true });
      }
      throw error;
    }
    let timedOut = false;
    let cancelled = false;
    let spawnError: Error | null = null;
    let captureError: Error | null = null;
    let forceTimer: NodeJS.Timeout | null = null;
    const stop = (): void => {
      if (child.pid !== undefined) terminate(child.pid);
      try { child.kill('SIGKILL'); } catch { /* already exited */ }
    };
    const capture = (name: StreamName, destination: NodeJS.WriteStream) =>
      (chunk: Buffer | string): void => {
      if (!streams || captureError) return;
      try {
        const state = streams[name];
        const data = Buffer.from(chunk);
        state.bytes += data.length;
        const remaining = maximum - state.retainedBytes;
        if (remaining > 0) {
          const retained = data.subarray(0, remaining);
          writeSync(state.fd, retained);
          state.hash.update(retained);
          state.retainedBytes += retained.length;
        }
        state.tail = `${state.tail}${data.toString('utf8')}`.slice(-2000);
        destination.write(data);
      } catch (error) {
        captureError = error instanceof Error ? error : new Error(String(error));
        stop();
      }
    };
    if (streams) {
      child.stdout?.on('data', capture('stdout', process.stdout));
      child.stderr?.on('data', capture('stderr', process.stderr));
    }
    const cancel = (): void => {
      cancelled = true;
      if (gracefulCancellationMs > 0) {
        try { child.kill('SIGTERM'); } catch { /* already exited */ }
        forceTimer = setTimeout(stop, gracefulCancellationMs);
        forceTimer.unref();
      } else stop();
    };
    if (signal) {
      if (signal.aborted) cancel();
      else signal.addEventListener('abort', cancel, { once: true });
    }
    const timer = setTimeout(() => {
      timedOut = true;
      stop();
    }, timeoutMs);
    child.once('error', error => { spawnError = error; });
    child.once('close', (code, childSignal) => {
      clearTimeout(timer);
      if (forceTimer) clearTimeout(forceTimer);
      signal?.removeEventListener('abort', cancel);
      const captured = streams ? Object.fromEntries(Object.entries(streams).map(([name, state]) => {
        closeSync(state.fd);
        return [name, { path: state.path, sha256: state.hash.digest('hex'), bytes: state.bytes,
          retainedBytes: state.retainedBytes, truncated: state.bytes > state.retainedBytes }];
      })) : null;
      const error = captureError ?? spawnError;
      resolveRun({ ok: !timedOut && !cancelled && !error && code === 0,
        code, signal: childSignal, timedOut, cancelled,
        error, logs: captured,
        stdoutTail: streams?.stdout.tail.trim() ?? '',
        stderrTail: streams?.stderr.tail.trim() ?? '' });
    });
  });
}
