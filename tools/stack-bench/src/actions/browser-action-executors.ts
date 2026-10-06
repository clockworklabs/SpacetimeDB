import { actionImplementation, ActionApplicationFailure } from './action-contract.js';
import type {
  ActionImplementation,
} from './action-contract.js';
import { actorFor, fail, inconclusive, pad } from './actor-action-runtime.js';
import { finding, findingText, renderFinding } from './action-findings.js';
import { settledLocatorCount } from '../evidence/browser-evidence.js';
import { harnessBrowserFailure } from '../evidence/harness-errors.js';
import { runApplicationNavigation } from './browser-navigation.js';
import { withWriteCompletion } from './auth-request-patch.js';
import { browserApplicationBoundary, pageFailure } from './browser-boundary.js';

export { browserApplicationBoundary, pageFailure };
import type { Page as PlaywrightPage } from 'playwright';


interface ScrollTarget {
  readonly tagName: string;
  readonly value?: string;
  readonly innerText: string;
  readonly type?: string;
  readonly options?: ArrayLike<{ value: string; label: string; selected?: boolean }>;
  readonly parentElement: ScrollTarget | null;
  closest(selector: 'details'): (ScrollTarget & { open: boolean }) | null;
  querySelectorAll(selector: 'details'): ArrayLike<{ open: boolean }>;
  scrollIntoView(options: { block: 'nearest'; inline: 'nearest'; behavior: 'instant' }): void;
  getBoundingClientRect(): { x: number; y: number };
  readonly ownerDocument: { readonly defaultView: { readonly IntersectionObserver: new (
    callback: (entries: Array<{ isIntersecting: boolean; intersectionRatio: number }>) => void,
  ) => { observe(element: unknown): void; disconnect(): void } } };
}

interface Locator {
  click(options?: unknown): Promise<void>;
  count(): Promise<number>;
  evaluate<Result>(callback: (element: ScrollTarget, displayedStatus?: boolean) => Result,
    arg?: boolean, options?: { timeout?: number }): Promise<Result>;
  evaluateAll<Result>(callback: (elements: ScrollTarget[]) => Result): Promise<Result>;
  fill(value: string): Promise<void>;
  filter(options: unknown): Locator;
  first(): Locator;
  getAttribute(name: string): Promise<string | null>;
  getByRole(role: string, options: unknown): Locator;
  innerText(): Promise<string>;
  isDisabled(): Promise<boolean>;
  isVisible(): Promise<boolean>;
  locator(selector: string, options?: unknown): Locator;
  or(locator: Locator): Locator;
  allInnerTexts(): Promise<string[]>;
  press(key: string): Promise<void>;
  selectOption(value: string): Promise<unknown>;
  type(text: string, options?: unknown): Promise<void>;
  waitFor(options?: unknown): Promise<void>;
}

interface Page extends Partial<Pick<PlaywrightPage, 'on' | 'off'>> {
  readonly keyboard: { press(key: string): Promise<void> };
  locator(selector: string, options?: unknown): Locator;
  reload(options?: unknown): Promise<unknown>;
  goto(url: string, options?: unknown): Promise<unknown>;
}

interface LocatorScope {
  readonly testid: string;
  readonly contains?: string;
  readonly containsAll?: readonly string[];
}

interface BrowserActor {
  readonly page: Page;
  prepareNavigation?(within: number, signal: AbortSignal): Promise<void>;
  loc(testid: string, options?: {
    readonly contains?: string;
    readonly scope?: { readonly testid: string; readonly contains?: string | RegExp };
    readonly editable?: boolean;
  }): Locator;
}

interface BrowserCapability {
  readonly authReadEndpoints?: readonly string[];
  readonly sequenceScopeFallback?: { readonly testid: string; readonly from: string; readonly to: string };
  readonly applicationUrl?: string;
  readonly defaultWithin: number;
  readonly recorded: {
    get(key: string): number | undefined;
    set(key: string, value: number): void;
  };
  expand(value: string | undefined): string | undefined;
  sleep(milliseconds: number, signal: AbortSignal): Promise<void>;
  testId(id: string): string;
}

interface BrowserCapabilities {
  readonly actors: { get(name: string): BrowserActor | undefined };
  readonly 'browser-interaction': BrowserCapability;
  readonly 'browser-observation': BrowserCapability;
  readonly clock: { sleep(milliseconds: number, signal: AbortSignal): Promise<void> };
}

interface CommonInput {
  readonly actor: string;
  readonly testid: string;
  readonly contains?: string;
  readonly in?: LocatorScope;
  readonly within?: number;
}

interface BrowserArguments<Input> {
  readonly input: Input;
  readonly capabilities: BrowserCapabilities;
  readonly signal: AbortSignal;
}

type InteractionInput = CommonInput & {
  readonly text: string;
  readonly enter?: boolean;
  readonly settleMs?: number;
  readonly key?: string;
};
type ExpectInput = CommonInput & {
  readonly attribute?: string;
  readonly absent?: boolean;
  readonly count?: number;
  readonly value?: string;
  readonly containsText?: string;
  readonly statusText?: string;
  readonly ignoreCase?: boolean;
  readonly notContains?: string;
  readonly nonEmpty?: boolean;
};
type ElementCountInput = CommonInput & {
  readonly equals?: number;
  readonly relativeTo?: string;
  readonly plus?: number;
};
type SequenceInput = CommonInput & { readonly equals: readonly string[] };
type UnavailableInput = CommonInput;
interface AllPresentInput {
  readonly actor: string;
  readonly count: number;
  readonly prefix: string;
  readonly within?: number;
}
type StableInput = CommonInput & { readonly samples?: number; readonly intervalMs?: number };
type RecordNumberInput = CommonInput & { readonly as: string; readonly count?: boolean };
type ExpectNumberInput = CommonInput & {
  readonly comparison?: 'atMost' | 'atLeast';
  readonly equals?: number;
  readonly relativeTo?: string;
  readonly plus?: number;
  readonly atLeast?: number;
  readonly atMost?: number;
};
interface OrderMatchesInput {
  readonly actors: readonly string[];
  readonly prefix: string;
}
type AgreementInput = Omit<CommonInput, 'actor'> & {
  readonly actors: readonly string[];
  readonly numeric?: boolean;
};
type ActorsWithInput = Omit<CommonInput, 'actor'> & {
  readonly actors: readonly string[];
  readonly equals?: number;
  readonly maxEach?: number;
};

// A count must hold across samples this far apart before it passes.
const COUNT_CONFIRM_MS = 500;
const escapePattern = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

function interaction(capabilities: BrowserCapabilities): BrowserCapability {
  return capabilities['browser-interaction'];
}

function observation(capabilities: BrowserCapabilities): BrowserCapability {
  return capabilities['browser-observation'];
}

function inputScope(browser: BrowserCapability, value: LocatorScope | undefined):
    { testid: string; contains?: string | RegExp } | undefined {
  if (!value) return undefined;
  if (value.containsAll !== undefined) {
    if (!Array.isArray(value.containsAll) || value.containsAll.length === 0
      || !value.containsAll.every(item => typeof item === 'string' && item.length > 0)) {
      throw new TypeError('locator containsAll must be a non-empty string array');
    }
    const terms = value.containsAll.map(item => escapePattern(browser.expand(item) ?? ''));
    return { testid: value.testid,
      contains: new RegExp(`^${terms.map(term => `(?=[\\s\\S]*${term})`).join('')}[\\s\\S]*$`, 'i') };
  }
  return { testid: value.testid, contains: browser.expand(value.contains) };
}

async function readValue(loc: Locator, timeout?: number, displayedStatus = false): Promise<string> {
  return loc.evaluate((element, displayedStatus) => {
    if (displayedStatus && element.tagName === 'SELECT') {
      return Array.from(element.options ?? []).filter(option => option.selected).map(option => option.label).join(' ');
    }
    if (displayedStatus && element.tagName === 'INPUT'
      && !['text', 'search', 'tel', 'url', 'email', 'button', 'submit', 'reset'].includes(element.type ?? 'text')) {
      return '';
    }
    if (['INPUT', 'TEXTAREA', 'SELECT'].includes(element.tagName)) return element.value || '';
    const text = element.innerText || '';
    return displayedStatus ? text : text.trim();
  }, displayedStatus, { timeout });
}

export function parseRenderedNumber(text: string | null | undefined): number | null {
  const match = (text ?? '').replace(/[,\u00a0]/g, '').match(/-?\d+(\.\d+)?/);
  return match ? Number(match[0]) : null;
}

function readControlNumber(text: string, control: string): number | null {
  // The catalog interface permits a stock state, without requiring numeric text.
  return control === 'item-stock' && /^out\s+of\s+stock$/i.test(text.trim())
    ? 0 : parseRenderedNumber(text);
}

async function clearInput({ input, capabilities }: BrowserArguments<{ actor: string }>) {
  await actorFor(capabilities, input.actor).loc('message-input').fill('');
  return { cleared: true };
}

async function click({ input, capabilities, signal }:
    BrowserArguments<CommonInput & { settleMs?: number; ifAvailable?: boolean; unlessVisible?: string | string[]; awaitWrites?: boolean }>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  const deadline = Date.now() + (input.within ?? browser.defaultWithin);
  const destinations = typeof input.unlessVisible === 'string'
    ? [input.unlessVisible] : input.unlessVisible ?? [];
  let visibleDestination: string | undefined;
  const destinationVisible = async (): Promise<boolean> => {
    visibleDestination = undefined;
    for (const testid of destinations) {
      while (true) {
        try {
          const sentinel = actor.loc(testid);
          if (!await sentinel.isVisible()) break;
          // Native scrolling does not wait for animation stability. A translated
          // closed drawer remains offscreen; ordinary inline content becomes visible.
          const visible = await sentinel.evaluateAll(async elements => {
            // Resolve once. A destination removed by navigation is absent, not a timeout.
            const element = elements[0];
            if (!element) return false;
            const place = () => {
              const box = element.getBoundingClientRect();
              return `${box.x},${box.y}`;
            };
            // A page still scrolling on its own, such as a restored position under smooth
            // scrolling, carries the destination back out of view. Judge it once it holds still.
            const until = Date.now() + 2000;
            while (true) {
              element.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'instant' });
              const placed = place();
              const intersecting = await new Promise<boolean>(resolve => {
                const observer = new element.ownerDocument.defaultView.IntersectionObserver(entries => {
                  observer.disconnect();
                  resolve(entries.some(entry => entry.isIntersecting && entry.intersectionRatio > 0));
                });
                observer.observe(element);
              });
              if (intersecting || place() === placed || Date.now() >= until) return intersecting;
            }
          });
          if (visible) {
            visibleDestination = testid;
            return true;
          }
          break;
        } catch (error) {
          // An unreadable destination does not establish whether a toggle is open.
          // Preserve that failure instead of clicking blindly and changing app state.
          // A render can replace the destination between visibility and scrolling.
          // Retry that read only; never repeat the navigation click.
          if (!/Element is not attached to the DOM/i.test(String(error))
            || Date.now() >= deadline || signal.aborted) throw error;
          await browser.sleep(Math.min(100, deadline - Date.now()), signal);
        }
      }
    }
    return false;
  };
  if (await destinationVisible()) {
    return { clicked: false, testid: input.testid, visible: visibleDestination };
  }
  const scope = inputScope(browser, input.in);
  const target = actor.loc(input.testid, { contains: browser.expand(input.contains), scope });
  if (input.ifAvailable) {
    while (!await target.isVisible() || await target.isDisabled()) {
      if (await destinationVisible()) {
        return { clicked: false, testid: input.testid, visible: visibleDestination };
      }
      if (Date.now() >= deadline) return { clicked: false, testid: input.testid };
      await browser.sleep(Math.min(100, deadline - Date.now()), signal);
    }
  }
  // Remember the clicked element so a same-name replacement can be recognised as a confirmation.
  // Only an action on one entry (an order, a ticket) can confirm inline; toggles and navigation cannot.
  // Best effort: a control that is not on screen yet keeps the plain click behaviour.
  const clickedText = !scope || input.unlessVisible || input.ifAvailable ? null
    : await (async () => !await target.isVisible() ? null : target.evaluate(element => {
      (globalThis as { __stackBenchClicked?: unknown }).__stackBenchClicked = element;
      return element.innerText;
    }, undefined, { timeout: 1000 }))().catch(() => null);
  let writeCompletion: Awaited<ReturnType<typeof withWriteCompletion>> | undefined;
  const submit = async () => {
    await target.click({ timeout: input.within ?? browser.defaultWithin });
    if (clickedText !== null) {
      await confirmReplacement(actor, browser, input.testid, scope, clickedText, signal).catch(() => {});
    }
  };
  try {
    if (input.awaitWrites) writeCompletion = await withWriteCompletion(actor.page as PlaywrightPage, async () => {
      await submit();
      if (input.settleMs) await browser.sleep(input.settleMs, signal);
    }, browser.authReadEndpoints);
    else await submit();
  } catch (error) {
    // An already-open view can finish loading while its covered navigation
    // control waits for actionability. Observe that destination; do not click again.
    const covered = input.unlessVisible && !signal.aborted && errorField(error, 'name') === 'TimeoutError'
      && /intercepts pointer events/i.test(String(error));
    if (covered && await Promise.race([destinationVisible(), browser.sleep(250, signal).then(() => false)])
      && !signal.aborted) {
      return { clicked: false, testid: input.testid, visible: visibleDestination };
    }
    // The contract gives an overlay that blocks navigation a visible overlay-close. A
    // menu opened to reveal this control can cover it once the page finishes loading.
    const overlayClose = actor.loc('overlay-close');
    if (covered && !input.awaitWrites && await overlayClose.isVisible()) {
      await overlayClose.click({ timeout: browser.defaultWithin });
      await overlayClose.waitFor({ state: 'hidden', timeout: browser.defaultWithin }).catch(() => {});
      if (await destinationVisible()) return { clicked: false, testid: input.testid, visible: visibleDestination };
      await submit();
      if (input.settleMs) await browser.sleep(input.settleMs, signal);
      return { clicked: input.testid };
    }
    throw error;
  }
  if (!input.awaitWrites && input.settleMs) await browser.sleep(input.settleMs, signal);
  return { clicked: input.testid, ...(writeCompletion ? { writes: writeCompletion.writes, capturedWritesCompleted: true } : {}) };
}

// "Cancel order" can become an inline "Yes, cancel order" under the same name. A user confirms
// it, so a click that replaces its control with one differently labelled control is followed once.
// The same element staying in place, or reappearing with the same label, is not a confirmation.
async function confirmReplacement(actor: BrowserActor, browser: BrowserCapability, testid: string,
  scope: ReturnType<typeof inputScope>, clickedText: string, signal: AbortSignal): Promise<void> {
  const clickedRemains = () => actor.page.locator('body').evaluate(() =>
    (globalThis as { __stackBenchClicked?: { isConnected: boolean } }).__stackBenchClicked?.isConnected === true);
  const deadline = Date.now() + 1500;
  while (Date.now() < deadline) {
    await browser.sleep(100, signal);
    if (await clickedRemains()) return;
    const next = actor.loc(testid, { scope });
    if (!await next.isVisible() || await next.isDisabled()) continue;
    if ((await next.innerText()).trim() === clickedText.trim()) return;
    await next.click({ timeout: browser.defaultWithin });
    return;
  }
}

async function openItem({ input, capabilities, signal }:
    BrowserArguments<{ actor: string; item: string; unlessVisible?: string;
      within?: number; settleMs?: number }>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  const item = browser.expand(input.item) ?? input.item;
  const timeout = input.within ?? browser.defaultWithin;
  const card = actor.loc('item-card', { contains: item }).first();
  await card.waitFor({ state: 'visible', timeout });
  if (input.unlessVisible && await card.locator(browser.testId(input.unlessVisible))
    .filter({ visible: true }).count() > 0) {
    return { item, opened: false, visible: input.unlessVisible };
  }
  await card.locator(browser.testId('item-name')).first().click({ timeout });
  await actor.loc('item-detail').waitFor({ state: 'visible', timeout });
  if (input.settleMs) await browser.sleep(input.settleMs, signal);
  return { item, opened: true };
}

async function fill({ input, capabilities, signal }: BrowserArguments<InteractionInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  const scope = inputScope(browser, input.in);
  const visible = actor.loc(input.testid, { scope });
  await visible.waitFor({ state: 'visible', timeout: input.within ?? browser.defaultWithin });
  // A contract can reuse an input's ID to show saved values; typing targets the editable one.
  const editable = actor.loc(input.testid, { scope, editable: true });
  const loc = await editable.count() > 0 ? editable : visible;
  const text = browser.expand(input.text) ?? '';
  const tag = await loc.evaluate(element => element.tagName);
  if (tag === 'SELECT') {
    try {
      // Playwright's string form matches either value or label. A label-only
      // retry repeats the same wait when the option is missing or disabled.
      await loc.selectOption(text);
    } catch (error) {
      if (errorField(error, 'name') !== 'TimeoutError') throw error;
      const options = await loc.evaluate(element => element.tagName === 'SELECT'
        ? Array.from(element.options ?? []).map(option => ({ value: option.value, label: option.label }))
        : null);
      if (options && !options.some(option => option.value === text || option.label === text)) {
        fail('choice-missing', { control: input.testid,
          ...(input.in ? { scope: input.in.testid } : {}),
          requestedChoice: findingText(text) });
      }
      throw error;
    }
  } else {
    const type = tag === 'INPUT' ? await loc.getAttribute('type') : null;
    const value = type === 'datetime-local' && /^\d{4}-\d{2}-\d{2}$/.test(text)
      ? `${text}T00:00`
      : type === 'date' && /^\d{4}-\d{2}-\d{2}T/.test(text) ? text.slice(0, 10) : text;
    await loc.fill(value);
  }
  if (input.enter) await loc.press('Enter');
  if (input.settleMs) await browser.sleep(input.settleMs, signal);
  return { filled: input.testid };
}

async function pressKey({ input, capabilities, signal }:
    BrowserArguments<{ actor: string; key?: string; settleMs?: number }>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  await actor.page.keyboard.press(input.key ?? 'Escape');
  await browser.sleep(input.settleMs ?? 600, signal);
  return { key: input.key ?? 'Escape' };
}

async function reload({ input, capabilities, signal }:
    BrowserArguments<{ actor: string; settleMs?: number; application?: boolean }>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  // Returning from hosted login must preserve this page's sessionStorage as well as cookies.
  if (input.application && !browser.applicationUrl) throw new Error('Application return requires the trusted application URL');
  await actor.prepareNavigation?.(browser.defaultWithin, signal);
  await runApplicationNavigation(() => input.application
    ? actor.page.goto(browser.applicationUrl!, { waitUntil: 'domcontentloaded', timeout: 20000 })
    : actor.page.reload({ waitUntil: 'domcontentloaded', timeout: 20000 }), actor.page);
  await browser.sleep(input.settleMs ?? 2500, signal);
  return { reloaded: true, ...(input.application ? { application: true } : {}) };
}

async function typeInto({ input, capabilities }:
    BrowserArguments<{ actor: string; text: string }>) {
  const actor = actorFor(capabilities, input.actor);
  const field = actor.loc('message-input');
  await field.click();
  await field.type(input.text, { delay: 40 });
  return { typed: true };
}

async function wait({ input, capabilities, signal }:
    BrowserArguments<{ actor: string; ms: number; since?: string }>) {
  actorFor(capabilities, input.actor);
  const elapsed = input.since === undefined ? 0 : elapsedSince(capabilities, input.since);
  const waitedMs = Math.max(0, input.ms - elapsed);
  await capabilities.clock.sleep(waitedMs, signal);
  return { waitedMs };
}

function elapsedSince(capabilities: BrowserArguments<unknown>['capabilities'], since: string): number {
  const start = observation(capabilities).recorded.get(since);
  if (start === undefined) inconclusive('assertion-without-action', { action: 'recordTime' });
  return performance.now() - start;
}

async function recordTime({ input, capabilities }: BrowserArguments<{ as: string }>) {
  observation(capabilities).recorded.set(input.as, performance.now());
  return { recorded: input.as };
}

async function expectElapsed({ input, capabilities }: BrowserArguments<{ since: string; atMost: number }>) {
  const elapsedMs = elapsedSince(capabilities, input.since);
  if (elapsedMs > input.atMost) inconclusive('observation-window-missed', {
    detail: `Observation began ${Math.round(elapsedMs)}ms after its timing origin; limit ${input.atMost}ms`,
  });
  return { elapsedMs, atMost: input.atMost };
}

// A user opens a collapsed <details> section to read it; its summary has no contracted name.
// Sections inside the scoped entry open too, so content that arrives later is shown.
async function openDisclosures(actor: BrowserActor, browser: BrowserCapability, testid: string,
  scope?: { readonly testid: string }): Promise<void> {
  await actor.page.locator(browser.testId(testid)).evaluateAll(controls => controls.forEach(control => {
    for (let section = control.closest('details'); section; section = section.parentElement?.closest('details') ?? null) {
      section.open = true;
    }
  }));
  if (scope) await actor.page.locator(browser.testId(scope.testid)).evaluateAll(entries => entries.forEach(entry => {
    for (const section of Array.from(entry.querySelectorAll('details'))) section.open = true;
  }));
}

async function expect({ input, capabilities, signal }: BrowserArguments<ExpectInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const within = input.within ?? browser.defaultWithin;
  const contains = browser.expand(input.contains);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const loc = actor.loc(input.testid, { contains, scope });

  if (input.absent) {
    const deadline = Date.now() + within;
    while (Date.now() <= deadline) {
      await openDisclosures(actor, browser, input.testid, scope);
      if (await loc.isVisible()) fail('control-present', { control: input.testid,
        ...(contains ? { matchingText: findingText(contains) } : {}),
        ...(scope?.testid ? { scope: scope.testid } : {}),
        ...(scope?.contains ? { scopeText: findingText(String(scope.contains)) } : {}) });
      await browser.sleep(250, signal);
    }
    return { absent: true };
  }

  const countDeadline = Date.now() + within;
  await openDisclosures(actor, browser, input.testid, scope);
  const visible = await loc.waitFor({ state: 'visible', timeout: within })
    .then(() => true).catch(error => {
      if (errorField(error, 'name') !== 'TimeoutError') throw error;
      return false;
    });
  if (!visible) fail('control-missing', { control: input.testid,
      ...(scope ? { scope: scope.testid } : {}),
      ...(contains ? { matchingText: findingText(contains) } : {}),
      ...(scope?.contains ? { scopeText: findingText(String(scope.contains)) } : {}),
      ...(contains || scope?.contains ? { filtered: true } : {}) });

  if (input.count !== undefined) {
    const all = scope
      ? actor.page.locator(browser.testId(scope.testid), { hasText: scope.contains })
        .filter({ visible: true }).first().locator(browser.testId(input.testid))
      : (contains
        ? actor.page.locator(browser.testId(input.testid), { hasText: contains })
        : actor.page.locator(browser.testId(input.testid)));
    // A count passes only when it holds across two samples; a duplicate that
    // appears just after the first matching sample must not pass.
    const countNow = () => all.filter({ visible: true }).count();
    let count = await countNow();
    for (;;) {
      if (count === input.count) {
        await browser.sleep(COUNT_CONFIRM_MS, signal);
        const again = await countNow();
        if (again === count) break;
        count = again;
      }
      if (Date.now() >= countDeadline) break;
      await browser.sleep(Math.min(250, countDeadline - Date.now()), signal);
      count = await countNow();
    }
    if (count !== input.count) {
      fail('count-mismatch', { control: input.testid, expected: input.count, observed: count,
        ...(contains ? { matchingText: findingText(contains) } : {}) });
    }
  }
  if (!visible) return { visible: false };

  const expectedText = input.value ?? input.containsText ?? input.statusText;
  let observedStatusText: string | undefined;
  if (expectedText !== undefined) {
    const deadline = Date.now() + within;
    const read = async () => input.attribute
      ? await loc.getAttribute(input.attribute) ?? ''
      : readValue(loc, undefined, input.statusText !== undefined);
    const matches = (value: string) => {
      const text = input.statusText === undefined ? value : value.replace(/^[\s\u00b7\u2022]+|[\s\u00b7\u2022]+$/g, '');
      const actual = input.ignoreCase ? text.toLowerCase() : text;
      const expected = input.ignoreCase ? expectedText.toLowerCase() : expectedText;
      return input.containsText === undefined ? actual === expected : actual.includes(expected);
    };
    let value = await read();
    while (!matches(value) && Date.now() <= deadline) {
      await browser.sleep(250, signal);
      value = await read();
    }
    if (input.statusText !== undefined) observedStatusText = value;
    if (!matches(value)) {
      const sensitive = /^password$/i.test(await loc.getAttribute('type') ?? '') || /password|secret|token/i.test(input.testid);
      fail(input.containsText === undefined ? 'value-mismatch' : 'text-missing', { control: input.testid,
        ...(input.containsText !== undefined && contains ? { matchingText: findingText(contains) } : {}),
        ...(!sensitive ? { observed: findingText(value), expected: findingText(expectedText) } : {}) });
    }
  }
  if (input.notContains) {
    const text = (await loc.innerText()) || '';
    if (text.includes(input.notContains)) fail('text-unexpected', { control: input.testid,
      ...(!/password|secret|token/i.test(input.testid) ? { matchedText: findingText(input.notContains) } : {}) });
  }
  if (input.nonEmpty) {
    const text = (await readValue(loc)).trim();
    if (!text) fail('control-empty', { control: input.testid });
  }
  return { visible: true, ...(input.value === undefined ? {} : {
    ...(input.attribute ? { attribute: input.attribute } : {}), value: input.value,
  }), ...(input.statusText === undefined ? {} : { statusText: input.statusText, observedText: observedStatusText }) };
}

async function waitUntilAbsent({ input, capabilities }: BrowserArguments<CommonInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const contains = browser.expand(input.contains);
  const scope = inputScope(browser, input.in);
  const loc = actor.loc(input.testid, { contains, scope });
  const within = input.within ?? browser.defaultWithin;
  const hidden = await loc.waitFor({ state: 'hidden', timeout: within })
    .then(() => true).catch(error => {
      if (errorField(error, 'name') !== 'TimeoutError') throw error;
      return false;
    });
  if (!hidden) fail('control-present', { control: input.testid,
    ...(contains ? { matchingText: findingText(contains) } : {}),
    ...(scope?.testid ? { scope: scope.testid } : {}),
    ...(scope?.contains ? { scopeText: findingText(String(scope.contains)) } : {}) });
  return { absent: true };
}

async function expectElementCount({ input, capabilities, signal }:
    BrowserArguments<ElementCountInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const within = input.within ?? 10000;
  const deadline = Date.now() + within;
  const loc = countLocator(actor, browser, input);
  const equals = expectedNumber(browser, input);
  if (equals === undefined || !Number.isSafeInteger(equals) || equals < 0) {
    throw new Error('expected element count must be a nonnegative safe integer');
  }
  for (;;) {
    let count = await loc.count();
    if (count === equals) {
      await browser.sleep(COUNT_CONFIRM_MS, signal);
      count = await loc.count();
      if (count === equals) return { count };
    }
    if (Date.now() > deadline) {
      fail('count-mismatch', { control: input.testid, expected: equals, observed: count });
    }
    await browser.sleep(400, signal);
  }
}

async function expectSequence({ input, capabilities, signal }: BrowserArguments<SequenceInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const within = input.within ?? browser.defaultWithin;
  const deadline = Date.now() + within;
  const primary = input.in
    ? actor.page.locator(browser.testId(input.in.testid),
      input.in.contains ? { hasText: browser.expand(input.in.contains) } : {}).filter({ visible: true }).first()
    : undefined;
  const fallback = browser.sequenceScopeFallback;
  const alternate = fallback && input.testid === fallback.testid && input.in?.testid === fallback.from
    ? actor.page.locator(browser.testId(fallback.to),
      input.in.contains ? { hasText: browser.expand(input.in.contains) } : {}).filter({ visible: true }).first()
    : undefined;
  let seen: string[] = [];
  for (;;) {
    const root = alternate && primary && !await primary.isVisible() ? alternate : primary ?? actor.page;
    seen = (await root.locator(browser.testId(input.testid)).filter({ visible: true }).allInnerTexts())
      .map(value => value.replace(/\s+/g, ' ').trim());
    if (seen.length === input.equals.length
      && seen.every((value, index) => value === browser.expand(input.equals[index]))) {
      return { values: seen };
    }
    if (Date.now() > deadline) fail('order-mismatch', { control: input.testid });
    await browser.sleep(250, signal);
  }
}

async function expectUnavailable({ input, capabilities, signal }:
    BrowserArguments<UnavailableInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const within = input.within ?? browser.defaultWithin;
  const deadline = Date.now() + within;
  const scope = input.in
    ? actor.page.locator(browser.testId(input.in.testid),
      input.in.contains ? { hasText: browser.expand(input.in.contains) } : {}).filter({ visible: true }).first()
    : actor.page;
  const loc = scope.locator(browser.testId(input.testid),
    input.contains ? { hasText: browser.expand(input.contains) } : {}).filter({ visible: true }).first();
  let reason = 'absent';
  while (Date.now() <= deadline) {
    if (await loc.isVisible()) {
      const disabled = await loc.isDisabled();
      const ariaDisabled = await loc.getAttribute('aria-disabled');
      if (!disabled && ariaDisabled !== 'true') {
        fail('control-available', { control: input.testid, actor: input.actor });
      }
      reason = 'disabled';
    } else reason = 'absent';
    await browser.sleep(250, signal);
  }
  return { unavailable: true, reason };
}

async function expectAllPresent({ input, capabilities, signal }:
    BrowserArguments<AllPresentInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const within = input.within ?? 10000;
  const deadline = Date.now() + within;
  for (;;) {
    const counts: number[] = [];
    for (let index = 1; index <= input.count; index++) {
      counts.push(await actor.page.locator(browser.testId('message-item'),
        { hasText: `${input.prefix}-${pad(index, input.count)}` }).count());
    }
    const missing = counts.filter(count => count === 0).length;
    const duplicated = counts.filter(count => count > 1).length;
    if (!missing && !duplicated) return { missing, duplicated };
    if (Date.now() > deadline) {
      fail('entries-missing', { expected: input.count, missing, duplicated });
    }
    await browser.sleep(500, signal);
  }
}

async function expectStable({ input, capabilities, signal }: BrowserArguments<StableInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const loc = actor.loc(input.testid, { contains: browser.expand(input.contains) });
  await loc.waitFor({ state: 'visible', timeout: input.within ?? browser.defaultWithin });
  const seen: string[] = [];
  for (let index = 0; index < (input.samples ?? 4); index++) {
    seen.push(((await loc.innerText()) || '').trim());
    await browser.sleep(input.intervalMs ?? 700, signal);
  }
  const distinct = [...new Set(seen)];
  if (distinct.length > 1) fail('value-unstable', { control: input.testid });
  return { samples: seen };
}

function countLocator(actor: BrowserActor, browser: BrowserCapability, input: CommonInput): Locator {
  const scope = inputScope(browser, input.in);
  const root = scope
    ? actor.page.locator(browser.testId(scope.testid),
      scope.contains ? { hasText: scope.contains } : {}).filter({ visible: true }).first()
    : actor.page;
  const contains = browser.expand(input.contains);
  return root.locator(browser.testId(input.testid),
    contains ? { hasText: contains } : {}).filter({ visible: true });
}

function expectedNumber(browser: BrowserCapability, input: ElementCountInput): number | undefined {
  if (input.relativeTo === undefined) return input.equals;
  const base = browser.recorded.get(input.relativeTo);
  if (base === undefined) inconclusive('assertion-without-action', { action: 'recordNumber' });
  return base + (input.plus ?? 0);
}

async function recordNumber({ input, capabilities, signal }: BrowserArguments<RecordNumberInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  if (input.count) {
    const value = await countLocator(actor, browser, input).count();
    browser.recorded.set(input.as, value);
    return { key: input.as, value };
  }
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  if (input.testid === 'warehouse-total' && scope?.testid === 'admin-warehouse-item') {
    const observed = await readWarehouseTotal(actor, browser, scope.contains,
      input.within ?? browser.defaultWithin, signal);
    browser.recorded.set(input.as, observed.value);
    return { key: input.as, ...observed };
  }
  const loc = actor.loc(input.testid, { contains: browser.expand(input.contains), scope });
  await loc.waitFor({ state: 'visible', timeout: input.within ?? browser.defaultWithin });
  const value = readControlNumber(await readValue(loc), input.testid);
  if (value === null) fail('number-missing', { control: input.testid });
  browser.recorded.set(input.as, value);
  return { key: input.as, value };
}

export function numberMatches(value: number, expected: { equals?: number; atLeast?: number; atMost?: number }): boolean {
  return (expected.equals === undefined || value === expected.equals)
    && (expected.atLeast === undefined || value >= expected.atLeast)
    && (expected.atMost === undefined || value <= expected.atMost);
}

async function expectNumber({ input, capabilities, signal }:
    BrowserArguments<ExpectNumberInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const within = input.within ?? browser.defaultWithin;
  const deadline = Date.now() + within;
  const contains = browser.expand(input.contains);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const target = expectedNumber(browser, input);
  const expected = {
    ...(input.atLeast === undefined ? {} : { atLeast: input.atLeast }),
    ...(input.atMost === undefined ? {} : { atMost: input.atMost }),
    ...(target === undefined ? {} : { [input.comparison ?? 'equals']: target }),
  };
  if (input.testid === 'warehouse-total' && scope?.testid === 'admin-warehouse-item') {
    return readWarehouseTotal(actor, browser, scope.contains, within, signal, expected);
  }
  const loc = actor.loc(input.testid, { contains, scope });
  await loc.waitFor({ state: 'visible', timeout: within }).catch(error => {
    if (errorField(error, 'name') !== 'TimeoutError') throw error;
    fail('control-missing', { control: input.testid,
      ...(scope ? { scope: scope.testid } : {}),
      ...(contains ? { matchingText: findingText(contains) } : {}),
      ...(scope?.contains ? { scopeText: findingText(String(scope.contains)) } : {}),
      ...(contains || scope?.contains ? { filtered: true } : {}) });
  });

  const matches = (number: number): boolean => numberMatches(number, expected);

  // A read that starts in time counts even if it finishes after the deadline, and
  // a control that became visible only near the deadline is still read once.
  let last = null;
  for (let first = true; first || Date.now() < deadline; first = false) {
    try {
      last = readControlNumber(await readValue(loc, Math.max(1_000, deadline - Date.now())), input.testid);
    } catch (error) {
      if (last !== null && !signal?.aborted && Date.now() >= deadline
        && errorField(error, 'name') === 'TimeoutError' && !harnessBrowserFailure(error)) break;
      throw error;
    }
    if (last !== null && matches(last)) return { value: last };
    if (Date.now() >= deadline) break;
    await browser.sleep(Math.min(250, deadline - Date.now()), signal);
  }
  fail('number-mismatch', { control: input.testid, observed: last, expected,
    ...(scope?.contains && !/password|secret|token/i.test(scope.testid)
      ? { scopeText: findingText(String(scope.contains)) } : {}) });
}

const matchesInventoryName = (text: string, name: string): boolean => new RegExp(
  `(?<![\\p{L}\\p{N}_])${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\p{L}\\p{N}_])`, 'u').test(text);

async function readWarehouseInventoryRows(actor: BrowserActor, browser: BrowserCapability,
  identities: readonly { control: string; names: readonly string[] }[]) {
  const selectors = Object.fromEntries(['admin-item-row', 'admin-warehouse-item', 'admin-location-row',
    'admin-stock', 'admin-location-qty', 'warehouse-total'].map(id => [id, browser.testId(id)]));
  const rows = ['admin-item-row', 'admin-warehouse-item', 'admin-location-row'];
  const observed = await (actor.page as PlaywrightPage).locator(browser.testId('admin-panel'))
      .filter({ visible: true }).locator(rows.map(id => selectors[id]).join(',')).filter({ visible: true })
      .evaluateAll((elements, selectors) => {
        interface Node {
          nodeType: number; textContent: string | null; childNodes: ArrayLike<Node>;
          matches(selector: string): boolean; closest(selector: string): Node | null; contains(node: Node): boolean;
          querySelectorAll(selector: string): ArrayLike<Node>;
          tagName: string; value?: string; innerText: string;
          getBoundingClientRect(): { width: number; height: number };
          ownerDocument: { defaultView: { getComputedStyle(node: Node): { display: string; visibility: string } } };
        }
        const visible = (node: Node): boolean => {
          const box = node.getBoundingClientRect();
          return !!(box.width || box.height) && node.ownerDocument.defaultView.getComputedStyle(node).visibility === 'visible';
        };
        // Marked labels can be nested. Keep each same-role row's labels separate
        // until redundant wrappers are resolved; child holdings own their text.
        const ownText = (node: Node, root: Node, excludedRows: string): string => {
          if (node.nodeType === 3) return node.textContent ?? '';
          if (node.nodeType !== 1 || node !== root && node.matches(excludedRows)) return '';
          const style = node.ownerDocument.defaultView.getComputedStyle(node);
          if (style.display === 'none' || style.visibility !== 'visible') return '';
          const text = Array.from(node.childNodes, child => ownText(child, root, excludedRows)).join('');
          return node.tagName === 'BR' || !style.display.startsWith('inline') && style.display !== 'contents'
            ? ` ${text} ` : text;
        };
        const observations = elements.map(element => {
          const row = element as unknown as Node;
          const control = ['admin-item-row', 'admin-warehouse-item', 'admin-location-row']
            .find(id => row.matches(selectors[id]!))!;
          const numeric = control === 'admin-item-row' ? 'admin-stock'
            : control === 'admin-warehouse-item' ? 'warehouse-total' : 'admin-location-qty';
          const owner = selectors[control]!;
          const values = Array.from(row.querySelectorAll(selectors[numeric]!))
              .filter(child => child.closest(owner) === row && visible(child))
              .map(child => ['INPUT', 'TEXTAREA', 'SELECT'].includes(child.tagName)
                ? child.value ?? '' : child.innerText);
          const excludedRows = [owner, selectors['admin-location-row']!].join(',');
          return { row, control, values,
            text: ownText(row, row, `${excludedRows},${selectors[numeric]}`).replace(/\s+/g, ' ').trim() };
        });
        return observations.map(entry => ({ control: entry.control, text: entry.text, values: entry.values,
          descendantLabels: observations.filter(child => child !== entry && child.control === entry.control
            && entry.row.contains(child.row)).map(child => child.text).filter(Boolean) }));
      }, selectors);
  // A repeated marker can wrap the same complete view. Compare the declared
  // identities, not decoration or DOM shape. Keep every numeric projection.
  return observed.filter(row => {
    if (row.values.length || !row.descendantLabels.length) return true;
    const identify = (text: string) => identities.flatMap((entry, index) => entry.control === row.control
      && entry.names.every(name => matchesInventoryName(text, name)) ? [index] : []);
    const matches = [...row.descendantLabels, ...(row.text ? [row.text] : [])].map(identify);
    return matches.some(match => match.length !== 1 || match[0] !== matches[0]![0]);
  }).map(({ descendantLabels: _descendants, ...row }) => row);
}

async function readWarehouseTotal(actor: BrowserActor, browser: BrowserCapability, name: string | undefined,
  within: number, signal: AbortSignal, expected?: { equals?: number; atLeast?: number; atMost?: number }) {
  const deadline = Date.now() + within;
  let failure: ActionApplicationFailure | undefined;
  for (;;) {
    signal.throwIfAborted();
    if (Date.now() >= deadline) {
      if (failure) throw failure;
      inconclusive('observation-window-missed', { detail: 'The warehouse total read did not start within its observation window' });
    }
    const observed = await readWarehouseInventoryRows(actor, browser,
      name === undefined ? [] : [{ control: 'admin-warehouse-item', names: [name] }]);
    signal.throwIfAborted();
    if (Date.now() > deadline) {
      if (failure) throw failure;
      inconclusive('observation-window-missed', {
        detail: 'The warehouse total snapshot arrived after its observation deadline',
      });
    }
    const rows = observed.filter(row => row.control === 'admin-warehouse-item');
    const selected = rows.filter(row => name !== undefined && matchesInventoryName(row.text, name));
    const values = selected.flatMap(row => row.values.map(text => readControlNumber(text, 'warehouse-total')));
    const value = values[0];
    const target = expected ?? { equals: value ?? undefined };
    let mismatch: ReturnType<typeof finding> | undefined;
    if (!selected.length || selected.some(row => !row.values.length)) {
      mismatch = finding('control-missing', { control: 'warehouse-total', scope: 'admin-warehouse-item', matchingText: findingText(name ?? '') });
    } else if (value == null || values.some(number => number === null || !numberMatches(number, target))) {
      mismatch = finding('number-mismatch', { control: 'warehouse-total',
        observed: values.find(number => number === null || !numberMatches(number, target)) ?? null,
        expected: target, scopeText: findingText(name ?? '') });
    }
    const observation = { copies: selected.length, values };
    if (!mismatch && value != null) return { value, ...observation };
    failure = new ActionApplicationFailure(renderFinding(mismatch!), {
      finding: mismatch!, observation,
    });
    await browser.sleep(Math.max(0, Math.min(250, deadline - Date.now())), signal);
  }
}

async function expectWarehouseInventory({ input, capabilities, signal }: BrowserArguments<{
  actor: string; items: Array<{ name: string; stock: { East: number; West: number } }>; within?: number;
}>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const deadline = Date.now() + (input.within ?? browser.defaultWithin);
  const expected = [
    ...input.items.map(item => ({ control: 'admin-item-row', names: [item.name],
      quantity: item.stock.East + item.stock.West })),
    ...['East', 'West'].map(name => ({ control: 'admin-warehouse-item', names: [name], quantity: null })),
    ...input.items.flatMap(item => (['East', 'West'] as const).map(warehouse => ({
      control: 'admin-location-row', names: [item.name, warehouse], quantity: item.stock[warehouse],
    }))),
  ];
  let failure: ActionApplicationFailure | undefined;
  for (;;) {
    signal.throwIfAborted();
    if (Date.now() >= deadline) {
      if (failure) throw failure;
      inconclusive('observation-window-missed', { detail: 'The warehouse inventory read did not start within its observation window' });
    }
    const observed = await readWarehouseInventoryRows(actor, browser, expected);
    signal.throwIfAborted();
    if (Date.now() > deadline) {
      if (failure) throw failure;
      inconclusive('observation-window-missed', {
        detail: 'The warehouse inventory snapshot arrived after its observation deadline',
      });
    }
    const copies = expected.map(() => 0);
    let mismatch: ReturnType<typeof finding> | undefined;
    const records = observed.map(row => {
      const identities = expected.flatMap((entry, index) => entry.control === row.control
        && entry.names.every(name => matchesInventoryName(row.text, name)) ? [index] : []);
      const numbers = row.control === 'admin-warehouse-item' ? [] : row.values.map(value => readControlNumber(value, row.control === 'admin-item-row'
        ? 'admin-stock' : 'admin-location-qty'));
      if (identities.length !== 1) {
        mismatch ??= finding('value-mismatch', { control: row.control, observed: findingText(row.text),
          expected: findingText('one declared inventory identity') });
      } else {
        const index = identities[0]!;
        copies[index]!++;
        const entry = expected[index]!;
        if (entry.quantity !== null && (!numbers.length || numbers.some(value => value !== entry.quantity))) {
          mismatch ??= finding('number-mismatch', {
            control: row.control === 'admin-item-row' ? 'admin-stock' : 'admin-location-qty',
            observed: numbers.find(value => value !== entry.quantity) ?? null,
            expected: { equals: entry.quantity }, scopeText: findingText(entry.names.join(' / ')),
          });
        }
      }
      return { ...row, numbers, identities: identities.map(index => expected[index]!.names) };
    });
    const missing = copies.findIndex(count => count === 0);
    if (missing !== -1) mismatch ??= finding('control-missing', { control: expected[missing]!.control,
      scope: 'admin-panel', matchingText: findingText(expected[missing]!.names.join(' / ')) });
    const result = { expected: expected.map((entry, index) => ({ ...entry, copies: copies[index] })), observed: records };
    if (!mismatch) return result;
    failure = new ActionApplicationFailure(renderFinding(mismatch), {
      finding: mismatch, expected, observation: result,
    });
    await browser.sleep(Math.max(0, Math.min(250, deadline - Date.now())), signal);
  }
}

async function expectOrderMatches({ input, capabilities }:
    BrowserArguments<OrderMatchesInput>) {
  const browser = observation(capabilities);
  const sequences: Record<string, string[]> = {};
  for (const name of input.actors) {
    const actor = actorFor(capabilities, name);
    const texts = await actor.page.locator(browser.testId('message-item')).allInnerTexts();
    sequences[name] = texts.flatMap((text) => {
      const matched = text.match(new RegExp(`${input.prefix}-\\d+`))?.[0];
      return matched ? [matched] : [];
    });
    if (sequences[name].length === 0) fail('control-empty', { control: 'message-item' });
  }
  const [first, ...rest] = input.actors;
  if (!first) throw new TypeError('expectOrderMatches requires at least one actor');
  const firstSequence = sequences[first];
  if (!firstSequence) throw new TypeError(`no sequence recorded for actor "${first}"`);
  for (const other of rest) {
    const otherSequence = sequences[other];
    if (!otherSequence) throw new TypeError(`no sequence recorded for actor "${other}"`);
    if (firstSequence.join('|') !== otherSequence.join('|')) {
      fail('order-mismatch', { control: 'message-item', actors: [first, other] });
    }
  }
  return { sequences };
}

async function expectAgreement({ input, capabilities, signal }:
    BrowserArguments<AgreementInput>) {
  const browser = observation(capabilities);
  const contains = browser.expand(input.contains);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const deadline = Date.now() + (input.within ?? 10000);
  let seen: Record<string, string> = {};
  for (;;) {
    seen = {};
    for (const name of input.actors) {
      const actor = actorFor(capabilities, name);
      const loc = actor.loc(input.testid, { contains, scope });
      const visible = await loc.isVisible();
      const text = !visible ? '<missing>' : (input.numeric
        ? await readValue(loc)
        : ((await loc.innerText()) || '<missing>')).trim() || '<missing>';
      seen[name] = input.numeric ? String(readControlNumber(text, input.testid) ?? '<no number>') : text;
    }
    const missing = Object.entries(seen)
      .filter(([, value]) => value === '<missing>' || value === '<no number>')
      .map(([name]) => name);
    if (!missing.length && new Set(Object.values(seen)).size === 1) return { seen };
    if (Date.now() > deadline) {
      if (missing.length) fail('control-unreadable', { control: input.testid, actors: missing });
      fail('clients-disagree', { control: input.testid, actors: input.actors });
    }
    await browser.sleep(500, signal);
  }
}

async function expectActorsWith({ input, capabilities }:
    BrowserArguments<ActorsWithInput>) {
  const browser = observation(capabilities);
  const contains = browser.expand(input.contains);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const counts = await Promise.all(input.actors.map(async (name): Promise<[string, number]> => {
    const actor = actorFor(capabilities, name);
    const loc = actor.loc(input.testid, { contains, scope });
    const all = scope
      ? actor.page.locator(browser.testId(scope.testid), { hasText: scope.contains }).first()
        .locator(browser.testId(input.testid)).filter({ visible: true })
      : (contains
        ? actor.page.locator(browser.testId(input.testid), { hasText: contains }).filter({ visible: true })
        : actor.page.locator(browser.testId(input.testid)).filter({ visible: true }));
    await settledLocatorCount(loc, input.within ?? browser.defaultWithin);
    return [name, await all.count()];
  }));

  const held = counts.filter(([, count]) => count > 0);
  if (input.equals !== undefined && held.length !== input.equals) {
    fail('actors-with-control', { control: input.testid, expected: input.equals, observed: held.length });
  }
  if (input.maxEach !== undefined) {
    const maxEach = input.maxEach;
    if (counts.some(([, count]) => count > maxEach)) {
      fail('too-many-per-actor', { control: input.testid, maxEach });
    }
  }
  return { counts: Object.fromEntries(counts) };
}

function errorField(error: unknown, field: string): unknown {
  return typeof error === 'object' && error !== null
    ? (error as Record<string, unknown>)[field]
    : undefined;
}

function contractBrowserAction<Input, Result>(
  implementation: (arguments_: BrowserArguments<Input>) => Result | Promise<Result>,
): ActionImplementation {
  const bounded = browserApplicationBoundary(implementation, ({ input }) => {
    const scope = errorField(input, 'in');
    const testid = errorField(scope, 'testid');
    return typeof testid === 'string' ? testid : undefined;
  });
  return actionImplementation(bounded);
}

interface ScriptCanaryArguments {
  readonly input: { readonly actor: string };
  readonly capabilities: {
    readonly actors: { get(name: string): { readonly page: {
      exposeFunction(name: string, callback: (probe?: boolean) => string): Promise<void>;
      evaluate<Result>(callback: () => Result): Promise<Result>;
    } } | undefined };
    readonly 'browser-observation': BrowserCapability;
  };
}

// Execution is recorded outside the document so DOM replacement cannot erase it.
async function scriptCanaryProbe({ input, capabilities }: ScriptCanaryArguments): Promise<void> {
  const actor = actorFor(capabilities, input.actor);
  const reply = await actor.page.evaluate(() => {
    const page = globalThis as unknown as {
      __stackBenchScriptCanary?: (probe: boolean) => Promise<string>;
    };
    return page.__stackBenchScriptCanary?.(true);
  });
  if (reply !== 'ready') inconclusive('invalid-input', { detail: 'script execution observer is unavailable' });
}

async function armScriptCanary(args: ScriptCanaryArguments): Promise<void> {
  const { input, capabilities } = args;
  const actor = actorFor(capabilities, input.actor);
  const recorded = capabilities['browser-observation'].recorded;
  const key = `script-canary:${input.actor}`;
  if (recorded.get(key) !== undefined) throw new Error('script execution observer is already armed');
  await actor.page.exposeFunction('__stackBenchScriptCanary', (probe?: boolean) => {
    if (probe !== true) recorded.set(key, (recorded.get(key) ?? 0) + 1);
    return 'ready';
  });
  recorded.set(key, 0);
  await scriptCanaryProbe(args);
}

async function expectNoScriptExecution(args: ScriptCanaryArguments): Promise<unknown> {
  const { input, capabilities } = args;
  const key = `script-canary:${input.actor}`;
  const recorded = capabilities['browser-observation'].recorded;
  if (recorded.get(key) === undefined) inconclusive('invalid-input', { detail: 'script execution observer was not armed' });
  await scriptCanaryProbe(args);
  const observation = { actor: input.actor, executions: recorded.get(key)! };
  if (observation.executions > 0) throw new ActionApplicationFailure('stored content executed script', { observation });
  return observation;
}

export const BROWSER_ACTION_IMPLEMENTATIONS = Object.freeze({
  armScriptCanary: actionImplementation(armScriptCanary),
  expectNoScriptExecution: actionImplementation(expectNoScriptExecution),
  clearInput: contractBrowserAction(clearInput),
  click: contractBrowserAction(click),
  expect: contractBrowserAction(expect),
  expectActorsWith: contractBrowserAction(expectActorsWith),
  expectAgreement: contractBrowserAction(expectAgreement),
  expectAllPresent: contractBrowserAction(expectAllPresent),
  expectElementCount: contractBrowserAction(expectElementCount),
  expectNumber: contractBrowserAction(expectNumber),
  expectWarehouseInventory: contractBrowserAction(expectWarehouseInventory),
  expectOrderMatches: contractBrowserAction(expectOrderMatches),
  expectSequence: contractBrowserAction(expectSequence),
  expectStable: contractBrowserAction(expectStable),
  expectUnavailable: contractBrowserAction(expectUnavailable),
  fill: contractBrowserAction(fill),
  openItem: contractBrowserAction(openItem),
  pressKey: contractBrowserAction(pressKey),
  recordNumber: contractBrowserAction(recordNumber),
  reload: contractBrowserAction(reload),
  typeInto: contractBrowserAction(typeInto),
  wait: contractBrowserAction(wait),
  recordTime: contractBrowserAction(recordTime),
  expectElapsed: contractBrowserAction(expectElapsed),
  waitUntilAbsent: contractBrowserAction(waitUntilAbsent),
});
