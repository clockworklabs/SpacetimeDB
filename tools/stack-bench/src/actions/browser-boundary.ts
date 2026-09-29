import { stripVTControlCharacters } from 'node:util';
import { ActionApplicationFailure, ActionInconclusive } from './action-contract.js';
import { finding, renderFinding } from './action-findings.js';
import { harnessBrowserFailure } from '../evidence/harness-errors.js';

// How a browser action's exception becomes an application finding, a harness
// error or an unresolved input. Kept apart so capture code can use it too.
function errorField(error: unknown, field: string): unknown {
  return typeof error === 'object' && error !== null
    ? (error as Record<string, unknown>)[field]
    : undefined;
}

function isExpectedBrowserFailure(error: unknown): boolean {
  if (error instanceof ActionApplicationFailure) return true;
  if (errorField(error, 'name') === 'TimeoutError') return true;
  const message = String(errorField(error, 'message') ?? error ?? '');
  // A Playwright stack frame does not prove an app defect. Only observed
  // control failures belong here; selector, script and protocol errors stay errors.
  return /^locator\.[^:]+: (?:Error: )?(?:strict mode violation:|Element is not (?:an? |editable)|Input of type \S+ cannot be filled|Element is outside of the viewport)/i.test(message);
}

// The one place raw browser text is read: a Playwright error becomes a
// finding by its shape. The text itself travels only as human detail.
export function pageFailure(message: string, scope?: string): ActionApplicationFailure {
  // Alternative/intersection locators do not establish a parent-child scope.
  const controls = [...new Set([...message.matchAll(/data-(?:testid|role)="([a-zA-Z0-9_-]+)"/g)]
    .map(match => match[1]!))];
  const combined = /\.(?:or|and)\(/.test(message);
  const control = combined ? undefined : controls.at(-1);
  scope ??= !combined && controls.length > 1 ? controls.at(-2) : undefined;
  const named = { ...(control ? { control } : {}), ...(scope ? { scope } : {}) };
  const value = /Page crashed/i.test(message) ? finding('page-crashed', { detail: message })
    : /intercepts pointer events/i.test(message) ? finding('control-blocked', { ...named, detail: message })
    : /timeout/i.test(message) ? finding('page-timeout', { ...named,
      ...(/\.or\(/.test(message) && controls.length ? { alternatives: controls } : {}), detail: message })
    : finding('page-error', { ...named, detail: message });
  return new ActionApplicationFailure(renderFinding(value), { finding: value });
}

function clickMayHaveDispatched(message: string): boolean {
  let pending = false;
  for (const line of stripVTControlCharacters(message).split('\n')) {
    if (/^\s*- click action done\s*$/.test(line)) return true;
    if (/^\s*- performing click action\s*$/.test(line)) {
      if (pending) return true;
      pending = true;
    } else if (/intercepts pointer events\s*$/.test(line)) {
      // Playwright's first-event hit-target interceptor blocks this attempt.
      pending = false;
    } else if (/retrying click action/.test(line) && pending) {
      // A later blocked attempt cannot disprove an earlier unresolved delivery.
      return true;
    }
  }
  return pending;
}

export function browserApplicationBoundary<Arguments, Result>(
  implementation: (arguments_: Arguments) => Result | Promise<Result>,
  scopeOf?: (arguments_: Arguments) => string | undefined,
): (arguments_: Arguments) => Promise<Result> {
  return async (args: Arguments): Promise<Result> => {
    try {
      return await implementation(args);
    } catch (error) {
      if (errorField(error, 'classification') || harnessBrowserFailure(error)) throw error;
      const message = String(errorField(error, 'message') ?? error);
      // Playwright can time out after delivering the input (for example, while
      // a close handler removes the target). Only an explicit interception
      // disproves delivery. Do not score or repeat an unresolved input.
      if (errorField(error, 'name') === 'TimeoutError' && /^locator\.click:/.test(message)
        && clickMayHaveDispatched(message)) {
        throw new ActionInconclusive('browser click timed out after input dispatch began', {
          observation: { detail: message }, expected: 'confirmed completion of the browser click',
        });
      }
      if (isExpectedBrowserFailure(error)) throw pageFailure(message, scopeOf?.(args));
      throw error;
    }
  };
}
