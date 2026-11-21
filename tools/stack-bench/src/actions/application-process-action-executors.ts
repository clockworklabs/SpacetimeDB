import { execFile } from 'node:child_process';
import { existsSync, realpathSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';

import { actionImplementation } from './action-contract.js';
import { fail, inconclusive } from './actor-action-runtime.js';
import type { ActionImplementation } from './action-contract.js';
import { harnessProcessFailure } from '../evidence/harness-errors.js';

type UnknownRecord = Record<string, unknown>;

function field(error: unknown, name: string): unknown {
  return typeof error === 'object' && error !== null
    ? (error as UnknownRecord)[name]
    : undefined;
}

function inside(root: string, path: string): boolean {
  const fromRoot = relative(root, path);
  return Boolean(fromRoot) && fromRoot !== '..'
    && !fromRoot.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`)
    && !isAbsolute(fromRoot);
}

function executeNodeScript(path: string, args: readonly string[], root: string,
  timeout: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolvePromise, reject) => {
    execFile(process.execPath, [path, ...args], {
      cwd: root,
      encoding: 'utf8',
      signal,
      timeout,
    }, (error, stdout, stderr) => {
      if (!error) return resolvePromise();
      Object.assign(error, { stdout, stderr });
      reject(error);
    });
  });
}

interface ApplicationProcessCapabilities {
  readonly 'application-files': {
    readonly root?: string;
    expand(value: string): string;
  };
  readonly subprocess: {
    sleep(milliseconds: number, signal: AbortSignal): Promise<void>;
  };
}

interface RunScriptInput {
  readonly args?: readonly string[];
  readonly script: string;
  readonly settleMs?: number;
  readonly timeoutMs?: number;
}

async function runScript({
  capabilities,
  input,
  signal,
}: {
  readonly capabilities: ApplicationProcessCapabilities;
  readonly input: RunScriptInput;
  readonly signal: AbortSignal;
}) {
  const files = capabilities['application-files'];
  const subprocess = capabilities.subprocess;
  const action = input;
  if (!files.root) inconclusive('app-directory-unknown', {});

  const root = realpathSync(files.root);
  const unresolved = resolve(root, action.script);
  if (!inside(root, unresolved) || !existsSync(unresolved)) {
    fail('script-invalid', { script: action.script });
  }
  const path = realpathSync(unresolved);
  if (!inside(root, path)) fail('script-invalid', { script: action.script });

  try {
    await executeNodeScript(path, (action.args ?? []).map(files.expand), root,
      action.timeoutMs ?? 60_000, signal);
  } catch (error) {
    if (signal.aborted) throw error;
    const timedOut = field(error, 'killed') === true || field(error, 'code') === 'ETIMEDOUT';
    if (!timedOut && harnessProcessFailure(error)) throw error;
    const output = String(field(error, 'stdout') ?? '') + String(field(error, 'stderr') ?? '');
    fail('script-failed', { script: action.script,
      detail: output.trim().slice(-200) || String(field(error, 'message') ?? error) });
  }
  await subprocess.sleep(action.settleMs ?? 3000, signal);
  return { script: action.script, completed: true };
}

export const RUN_SCRIPT_ACTION_IMPLEMENTATION: ActionImplementation = actionImplementation(runScript);
