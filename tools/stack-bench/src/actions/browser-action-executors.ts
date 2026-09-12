import { actionImplementation, ActionApplicationFailure } from './action-contract.js';
import type {
  ActionImplementation,
} from './action-contract.js';
import { actorFor, fail, inconclusive, pad } from './actor-action-runtime.js';
import { finding, findingText, renderFinding } from './action-findings.js';
import { settledLocatorCount } from '../evidence/browser-evidence.js';
import { harnessBrowserFailure } from '../evidence/harness-errors.js';


interface Locator {
  click(options?: unknown): Promise<void>;
  count(): Promise<number>;
  evaluate<Result>(callback: (element: { readonly tagName: string;
    readonly options?: ArrayLike<{ value: string; label: string }>;
    readonly ownerDocument: { readonly defaultView: { readonly IntersectionObserver: new (
      callback: (entries: Array<{ isIntersecting: boolean; intersectionRatio: number }>) => void,
    ) => { observe(element: unknown): void; disconnect(): void } } };
  }) => Result): Promise<Result>;
  fill(value: string): Promise<void>;
  filter(options: unknown): Locator;
  first(): Locator;
  getAttribute(name: string): Promise<string | null>;
  getByRole(role: string, options: unknown): Locator;
  innerText(): Promise<string>;
  inputValue(): Promise<string>;
  isDisabled(): Promise<boolean>;
  isVisible(): Promise<boolean>;
  locator(selector: string, options?: unknown): Locator;
  or(locator: Locator): Locator;
  allInnerTexts(): Promise<string[]>;
  press(key: string): Promise<void>;
  selectOption(value: string | { readonly label: string }): Promise<unknown>;
  scrollIntoViewIfNeeded(options?: unknown): Promise<void>;
  type(text: string, options?: unknown): Promise<void>;
  waitFor(options?: unknown): Promise<void>;
}

interface Page {
  readonly keyboard: { press(key: string): Promise<void> };
  locator(selector: string, options?: unknown): Locator;
  reload(options?: unknown): Promise<unknown>;
}

interface LocatorScope {
  readonly testid: string;
  readonly contains?: string;
  readonly containsAll?: readonly string[];
}

interface BrowserActor {
  readonly page: Page;
  loc(testid: string, options?: {
    readonly contains?: string;
    readonly scope?: { readonly testid: string; readonly contains?: string | RegExp };
  }): Locator;
}

interface BrowserCapability {
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

async function readValue(loc: Locator): Promise<string> {
  const tag = await loc.evaluate(element => element.tagName);
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') {
    return (await loc.inputValue()) || '';
  }
  return ((await loc.innerText()) || '').trim();
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
    BrowserArguments<CommonInput & { settleMs?: number; ifAvailable?: boolean; unlessVisible?: string }>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  const deadline = Date.now() + (input.within ?? browser.defaultWithin);
  const destinationVisible = async (): Promise<boolean> => {
    if (!input.unlessVisible) return false;
    while (true) {
      try {
        const sentinel = actor.loc(input.unlessVisible);
        if (!await sentinel.isVisible()) return false;
        // A translated closed drawer is "visible" to Playwright. Scroll normal
        // inline content first, then distinguish it from an offscreen drawer.
        await sentinel.scrollIntoViewIfNeeded({ timeout: Math.max(1, deadline - Date.now()) });
        return await sentinel.evaluate(element => new Promise<boolean>(resolve => {
          const observer = new element.ownerDocument.defaultView.IntersectionObserver(entries => {
            observer.disconnect();
            resolve(entries.some(entry => entry.isIntersecting && entry.intersectionRatio > 0));
          });
          observer.observe(element);
        }));
      } catch (error) {
        // A render can replace the destination between visibility and scrolling.
        // Retry that read only; never repeat the navigation click.
        if (!/Element is not attached to the DOM/i.test(String(error))
          || Date.now() >= deadline || signal.aborted) throw error;
        await browser.sleep(Math.min(100, deadline - Date.now()), signal);
      }
    }
  };
  if (await destinationVisible()) {
    return { clicked: false, testid: input.testid, visible: input.unlessVisible };
  }
  const scope = inputScope(browser, input.in);
  const target = actor.loc(input.testid, { contains: browser.expand(input.contains), scope });
  if (input.ifAvailable) {
    while (!await target.isVisible() || await target.isDisabled()) {
      if (await destinationVisible()) {
        return { clicked: false, testid: input.testid, visible: input.unlessVisible };
      }
      if (Date.now() >= deadline) return { clicked: false, testid: input.testid };
      await browser.sleep(Math.min(100, deadline - Date.now()), signal);
    }
  }
  await target.click({ timeout: input.within ?? browser.defaultWithin });
  if (input.settleMs) await browser.sleep(input.settleMs, signal);
  return { clicked: input.testid };
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
  const loc = actor.loc(input.testid, { scope });
  await loc.waitFor({ state: 'visible', timeout: input.within ?? browser.defaultWithin });
  const text = browser.expand(input.text) ?? '';
  const tag = await loc.evaluate(element => element.tagName);
  if (tag === 'SELECT') {
    try {
      await loc.selectOption(text).catch(async () => { await loc.selectOption({ label: text }); });
    } catch (error) {
      if (harnessBrowserFailure(error)) throw error;
      const options = await loc.evaluate(element => element.tagName === 'SELECT'
        ? Array.from(element.options ?? []).map(option => ({ value: option.value, label: option.label }))
        : null).catch(() => null);
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
    BrowserArguments<{ actor: string; settleMs?: number }>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  await actor.page.reload({ waitUntil: 'domcontentloaded' });
  await browser.sleep(input.settleMs ?? 2500, signal);
  return { reloaded: true };
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
      if (await loc.isVisible()) fail('control-present', { control: input.testid,
        ...(contains ? { matchingText: findingText(contains) } : {}),
        ...(scope?.testid ? { scope: scope.testid } : {}),
        ...(scope?.contains ? { scopeText: findingText(String(scope.contains)) } : {}) });
      await browser.sleep(250, signal);
    }
    return { absent: true };
  }

  const visible = await loc.waitFor({ state: 'visible', timeout: within })
    .then(() => true).catch(error => {
      if (harnessBrowserFailure(error)) throw error;
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
    const count = visible ? await all.filter({ visible: true }).count() : 0;
    if (input.count !== undefined && count !== input.count) {
      fail('count-mismatch', { control: input.testid, expected: input.count, observed: count,
        ...(contains ? { matchingText: findingText(contains) } : {}) });
    }
  }
  if (!visible) return { visible: false };

  if (input.value !== undefined) {
    const deadline = Date.now() + within;
    const read = async () => input.attribute
      ? await loc.getAttribute(input.attribute) ?? ''
      : readValue(loc);
    const matches = (value: string) => input.ignoreCase
      ? value.toLowerCase() === input.value!.toLowerCase() : value === input.value;
    let value = await read();
    while (!matches(value) && Date.now() <= deadline) {
      await browser.sleep(250, signal);
      value = await read();
    }
    if (!matches(value)) {
      const sensitive = /^password$/i.test(await loc.getAttribute('type') ?? '') || /password|secret|token/i.test(input.testid);
      fail('value-mismatch', { control: input.testid,
        ...(!sensitive ? { observed: findingText(value), expected: findingText(input.value) } : {}) });
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
  }) };
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
      if (harnessBrowserFailure(error)) throw error;
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
    const count = await loc.count();
    if (count === equals) return { count };
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
  const root = input.in
    ? actor.page.locator(browser.testId(input.in.testid),
      input.in.contains ? { hasText: browser.expand(input.in.contains) } : {}).filter({ visible: true }).first()
    : actor.page;
  let seen: string[] = [];
  for (;;) {
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

async function recordNumber({ input, capabilities }: BrowserArguments<RecordNumberInput>) {
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
  const contains = browser.expand(input.contains);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const loc = actor.loc(input.testid, { contains, scope });
  await loc.waitFor({ state: 'visible', timeout: within }).catch(error => {
    if (harnessBrowserFailure(error)) throw error;
    fail('control-missing', { control: input.testid,
      ...(scope ? { scope: scope.testid } : {}),
      ...(contains ? { matchingText: findingText(contains) } : {}),
      ...(scope?.contains ? { scopeText: findingText(String(scope.contains)) } : {}),
      ...(contains || scope?.contains ? { filtered: true } : {}) });
  });

  const target = expectedNumber(browser, input);
  const expected = {
    ...(input.atLeast === undefined ? {} : { atLeast: input.atLeast }),
    ...(input.atMost === undefined ? {} : { atMost: input.atMost }),
    ...(target === undefined ? {} : { [input.comparison ?? 'equals']: target }),
  };
  const matches = (number: number): boolean => numberMatches(number, expected);

  const deadline = Date.now() + within;
  let last = null;
  for (;;) {
    last = readControlNumber(await readValue(loc), input.testid);
    if (last !== null && matches(last)) return { value: last };
    if (Date.now() > deadline) break;
    await browser.sleep(250, signal);
  }
  fail('number-mismatch', { control: input.testid, observed: last, expected,
    ...(scope?.contains && !/password|secret|token/i.test(scope.testid)
      ? { scopeText: findingText(String(scope.contains)) } : {}) });
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

function isExpectedBrowserFailure(error: unknown): boolean {
  if (error instanceof ActionApplicationFailure) return true;
  if (errorField(error, 'name') === 'TimeoutError') return true;
  const stack = String(errorField(error, 'stack') ?? '');
  const message = String(errorField(error, 'message') ?? error ?? '');
  return /node_modules[\\/]playwright/.test(stack)
    || /^(?:locator|page|keyboard|browserContext)\./i.test(message);
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

export function browserApplicationBoundary<Arguments, Result>(
  implementation: (arguments_: Arguments) => Result | Promise<Result>,
  scopeOf?: (arguments_: Arguments) => string | undefined,
): (arguments_: Arguments) => Promise<Result> {
  return async (args: Arguments): Promise<Result> => {
    try {
      return await implementation(args);
    } catch (error) {
      if (errorField(error, 'classification') || harnessBrowserFailure(error)) throw error;
      if (isExpectedBrowserFailure(error)) throw pageFailure(String(errorField(error, 'message') ?? error), scopeOf?.(args));
      throw error;
    }
  };
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

export const BROWSER_ACTION_IMPLEMENTATIONS = Object.freeze({
  clearInput: contractBrowserAction(clearInput),
  click: contractBrowserAction(click),
  expect: contractBrowserAction(expect),
  expectActorsWith: contractBrowserAction(expectActorsWith),
  expectAgreement: contractBrowserAction(expectAgreement),
  expectAllPresent: contractBrowserAction(expectAllPresent),
  expectElementCount: contractBrowserAction(expectElementCount),
  expectNumber: contractBrowserAction(expectNumber),
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
