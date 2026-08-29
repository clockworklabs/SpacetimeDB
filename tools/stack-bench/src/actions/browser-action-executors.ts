import { ActionApplicationFailure } from './action-contract.js';
import type {
  ActionImplementation,
  ActionImplementationArguments,
} from './action-contract.js';
import { settledLocatorCount } from '../evidence/browser-evidence.js';
import { harnessBrowserFailure } from '../evidence/harness-errors.js';

interface Locator {
  click(options?: unknown): Promise<void>;
  count(): Promise<number>;
  evaluate<Result>(callback: (element: { readonly tagName: string }) => Result): Promise<Result>;
  fill(value: string): Promise<void>;
  filter(options: unknown): Locator;
  first(): Locator;
  getAttribute(name: string): Promise<string | null>;
  innerText(): Promise<string>;
  inputValue(): Promise<string>;
  isDisabled(): Promise<boolean>;
  isVisible(): Promise<boolean>;
  locator(selector: string, options?: unknown): Locator;
  allInnerTexts(): Promise<string[]>;
  press(key: string): Promise<void>;
  selectOption(value: string | { readonly label: string }): Promise<unknown>;
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
  readonly absent?: boolean;
  readonly count?: number;
  readonly maxCount?: number;
  readonly value?: string;
  readonly notContains?: string;
  readonly nonEmpty?: boolean;
};
type ElementCountInput = CommonInput & { readonly equals: number };
type SequenceInput = CommonInput & { readonly equals: readonly string[] };
type UnavailableInput = CommonInput;
interface AllPresentInput {
  readonly actor: string;
  readonly count: number;
  readonly prefix: string;
  readonly within?: number;
}
type StableInput = CommonInput & { readonly samples?: number; readonly intervalMs?: number };
type RecordNumberInput = CommonInput & { readonly as: string };
type ExpectNumberInput = CommonInput & {
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

export const BROWSER_ACTION_IDS = Object.freeze([
  'clearInput',
  'click',
  'expect',
  'expectActorsWith',
  'expectAgreement',
  'expectAllPresent',
  'expectElementCount',
  'expectNumber',
  'expectOrderMatches',
  'expectSequence',
  'expectStable',
  'expectUnavailable',
  'fill',
  'pressKey',
  'recordNumber',
  'reload',
  'typeInto',
  'wait',
].sort());

const fail = (message: string): never => { throw new ActionApplicationFailure(message); };
const escapePattern = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

function actorFor(capabilities: BrowserCapabilities, name: string): BrowserActor {
  const actor = capabilities.actors.get(name);
  if (!actor) throw new ActionApplicationFailure(`unknown actor "${name}"`);
  return actor;
}

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
  const tag = await loc.evaluate(element => element.tagName).catch(() => '');
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') {
    return (await loc.inputValue().catch(() => '')) || '';
  }
  return (await loc.innerText().catch(() => '')) || '';
}

export function parseRenderedNumber(text: string | null | undefined): number | null {
  const match = (text ?? '').replace(/[,\u00a0]/g, '').match(/-?\d+(\.\d+)?/);
  return match ? Number(match[0]) : null;
}

const pad = (index: number, count: number): string =>
  String(index).padStart(String(count).length, '0');

async function clearInput({ input, capabilities }: BrowserArguments<{ actor: string }>) {
  await actorFor(capabilities, input.actor).loc('message-input').fill('');
  return { cleared: true };
}

async function click({ input, capabilities, signal }:
    BrowserArguments<CommonInput & { settleMs?: number }>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  const scope = inputScope(browser, input.in);
  await actor.loc(input.testid, { contains: browser.expand(input.contains), scope })
    .click({ timeout: input.within ?? browser.defaultWithin });
  if (input.settleMs) await browser.sleep(input.settleMs, signal);
  return { clicked: input.testid };
}

async function fill({ input, capabilities, signal }: BrowserArguments<InteractionInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  const scope = inputScope(browser, input.in);
  const loc = actor.loc(input.testid, { scope });
  await loc.waitFor({ state: 'visible', timeout: input.within ?? browser.defaultWithin });
  const text = browser.expand(input.text) ?? '';
  const tag = await loc.evaluate(element => element.tagName).catch(() => '');
  if (tag === 'SELECT') {
    await loc.selectOption(text).catch(async () => { await loc.selectOption({ label: text }); });
  } else {
    const type = tag === 'INPUT' ? await loc.getAttribute('type').catch(() => null) : null;
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
    BrowserArguments<{ actor: string; ms: number }>) {
  actorFor(capabilities, input.actor);
  await capabilities.clock.sleep(input.ms, signal);
  return { waitedMs: input.ms };
}

async function expect({ input, capabilities, signal }: BrowserArguments<ExpectInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const within = input.within ?? browser.defaultWithin;
  const contains = browser.expand(input.contains);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const where = scope ? ` inside ${browser.testId(scope.testid)} "${scope.contains}"` : '';
  const loc = actor.loc(input.testid, { contains, scope });

  if (input.absent) {
    const deadline = Date.now() + within;
    for (;;) {
      const visible = await loc.isVisible().catch(() => false);
      if (!visible) return { absent: true };
      if (Date.now() > deadline) {
        fail(`${browser.testId(input.testid)}${contains ? ` containing "${contains}"` : ''}${where} still visible after ${within}ms`);
      }
      await browser.sleep(250, signal);
    }
  }

  const visible = await loc.waitFor({ state: 'visible', timeout: within })
    .then(() => true).catch(() => false);
  if (!visible && !(input.maxCount !== undefined && input.count === undefined)) {
    fail(`${browser.testId(input.testid)}${contains ? ` containing "${contains}"` : ''}${where} not visible within ${within}ms`);
  }

  if (input.count !== undefined || input.maxCount !== undefined) {
    const all = scope
      ? actor.page.locator(browser.testId(scope.testid), { hasText: scope.contains })
        .filter({ visible: true }).first().locator(browser.testId(input.testid))
      : (contains
        ? actor.page.locator(browser.testId(input.testid), { hasText: contains })
        : actor.page.locator(browser.testId(input.testid)));
    const count = visible ? await all.filter({ visible: true }).count() : 0;
    if (input.count !== undefined && count !== input.count) {
      fail(`expected exactly ${input.count} ${browser.testId(input.testid)}`
        + `${contains ? ` containing "${contains}"` : ''}, found ${count}`);
    }
    if (input.maxCount !== undefined && count > input.maxCount) {
      fail(`expected at most ${input.maxCount} ${browser.testId(input.testid)}`
        + `${contains ? ` containing "${contains}"` : ''}, found ${count}`);
    }
  }
  if (!visible) return { visible: false };

  if (input.value !== undefined) {
    const deadline = Date.now() + within;
    let value = await readValue(loc);
    while (value !== input.value && Date.now() <= deadline) {
      await browser.sleep(250, signal);
      value = await readValue(loc);
    }
    if (value !== input.value) {
      fail(`${browser.testId(input.testid)} expected value "${input.value}", got "${value}"`);
    }
  }
  if (input.notContains) {
    const text = (await loc.innerText().catch(() => '')) || '';
    if (text.includes(input.notContains)) {
      fail(`${browser.testId(input.testid)} unexpectedly contains "${input.notContains}" `
        + `(text: "${text.trim().slice(0, 80)}")`);
    }
  }
  if (input.nonEmpty) {
    const text = (await readValue(loc)).trim();
    if (!text) fail(`${browser.testId(input.testid)} is visible but empty`);
  }
  return { visible: true, ...(input.value === undefined ? {} : { value: input.value }) };
}

async function expectElementCount({ input, capabilities, signal }:
    BrowserArguments<ElementCountInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const within = input.within ?? 10000;
  const deadline = Date.now() + within;
  const root = input.in
    ? actor.page.locator(browser.testId(input.in.testid),
      input.in.contains ? { hasText: input.in.contains } : {}).filter({ visible: true }).first()
    : actor.page;
  for (;;) {
    const count = await root.locator(browser.testId(input.testid),
      input.contains ? { hasText: input.contains } : {}).filter({ visible: true }).count();
    if (count === input.equals) return { count };
    if (Date.now() > deadline) {
      fail(`expected exactly ${input.equals} ${browser.testId(input.testid)}`
        + `${input.contains ? ` containing "${input.contains}"` : ''}, saw ${count} (after ${within}ms)`);
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
    if (Date.now() > deadline) {
      fail(`expected ${browser.testId(input.testid)} sequence ${JSON.stringify(input.equals)}, `
        + `saw ${JSON.stringify(seen)} (after ${within}ms)`);
    }
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
  for (;;) {
    if (!await loc.isVisible().catch(() => false)) return { unavailable: true, reason: 'absent' };
    const disabled = await loc.isDisabled().catch(() => false);
    const ariaDisabled = await loc.getAttribute('aria-disabled').catch(() => null);
    if (disabled || ariaDisabled === 'true') return { unavailable: true, reason: 'disabled' };
    if (Date.now() > deadline) {
      fail(`${browser.testId(input.testid)} is available to ${input.actor} after ${within}ms`);
    }
    await browser.sleep(250, signal);
  }
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
      fail(`of ${input.count} "${input.prefix}" messages: ${missing} missing, `
        + `${duplicated} duplicated (after ${within}ms)`);
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
    seen.push(((await loc.innerText().catch(() => '')) || '').trim());
    await browser.sleep(input.intervalMs ?? 700, signal);
  }
  const distinct = [...new Set(seen)];
  if (distinct.length > 1) {
    fail(`${browser.testId(input.testid)} changed while idle: `
      + distinct.map(text => JSON.stringify(text.slice(0, 50))).join(' then '));
  }
  return { samples: seen };
}

async function recordNumber({ input, capabilities }: BrowserArguments<RecordNumberInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const loc = actor.loc(input.testid, { contains: browser.expand(input.contains), scope });
  await loc.waitFor({ state: 'visible', timeout: input.within ?? browser.defaultWithin });
  const value = parseRenderedNumber(await readValue(loc));
  if (value === null) {
    throw new ActionApplicationFailure(
      `${browser.testId(input.testid)} has no number to record`,
    );
  }
  browser.recorded.set(input.as, value);
  return { key: input.as, value };
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
  const where = scope ? ` inside ${browser.testId(scope.testid)} "${scope.contains}"` : '';
  const loc = actor.loc(input.testid, { contains, scope });
  await loc.waitFor({ state: 'visible', timeout: within }).catch(() => {
    fail(`${browser.testId(input.testid)}${where} not visible within ${within}ms`);
  });

  let equals = input.equals;
  if (input.relativeTo !== undefined) {
    const base = browser.recorded.get(input.relativeTo);
    if (base === undefined) {
      throw new ActionApplicationFailure(`no number recorded as "${input.relativeTo}"`);
    }
    equals = base + (input.plus ?? 0);
  }
  const matches = (number: number): boolean => (equals === undefined || number === equals)
    && (input.atLeast === undefined || number >= input.atLeast)
    && (input.atMost === undefined || number <= input.atMost);

  const deadline = Date.now() + within;
  let last = null;
  for (;;) {
    last = parseRenderedNumber(await readValue(loc));
    if (last !== null && matches(last)) return { value: last };
    if (Date.now() > deadline) break;
    await browser.sleep(250, signal);
  }
  const wanted = [
    equals !== undefined
      ? `exactly ${equals}${input.relativeTo !== undefined
        ? ` (${input.relativeTo} + ${input.plus ?? 0})` : ''}`
      : null,
    input.atLeast !== undefined ? `at least ${input.atLeast}` : null,
    input.atMost !== undefined ? `at most ${input.atMost}` : null,
  ].filter(Boolean).join(' and ') || 'a number';
  fail(`${browser.testId(input.testid)}${where} reads ${last === null ? 'no number' : last}, `
    + `expected ${wanted}`);
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
  }
  const [first, ...rest] = input.actors;
  if (!first) throw new TypeError('expectOrderMatches requires at least one actor');
  const firstSequence = sequences[first];
  if (!firstSequence) throw new TypeError(`no sequence recorded for actor "${first}"`);
  for (const other of rest) {
    const otherSequence = sequences[other];
    if (!otherSequence) throw new TypeError(`no sequence recorded for actor "${other}"`);
    if (firstSequence.join('|') !== otherSequence.join('|')) {
      const differentAt = firstSequence.findIndex(
        (value, index) => value !== otherSequence[index],
      );
      fail(`message order differs between ${first} and ${other} at position ${differentAt}: `
        + `${first} saw ${firstSequence.slice(Math.max(0, differentAt - 1), differentAt + 2).join(',')} / `
        + `${other} saw ${otherSequence.slice(Math.max(0, differentAt - 1), differentAt + 2).join(',')}`);
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
      const text = (input.numeric
        ? await readValue(loc)
        : ((await loc.innerText().catch(() => '<missing>')) || '<missing>')).trim() || '<missing>';
      seen[name] = input.numeric ? String(parseRenderedNumber(text) ?? '<no number>') : text;
    }
    if (new Set(Object.values(seen)).size === 1) return { seen };
    if (Date.now() > deadline) {
      fail(`clients disagree on ${browser.testId(input.testid)}: `
        + Object.entries(seen).map(([name, value]) =>
          `${name} sees ${JSON.stringify(value.slice(0, 40))}`).join(', '));
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
        .locator(browser.testId(input.testid))
      : (contains
        ? actor.page.locator(browser.testId(input.testid), { hasText: contains })
        : actor.page.locator(browser.testId(input.testid)));
    await settledLocatorCount(loc, input.within ?? browser.defaultWithin);
    return [name, await all.count()];
  }));

  const held = counts.filter(([, count]) => count > 0);
  const detail = counts.map(([name, count]) => `${name}=${count}`).join(' ');
  if (input.equals !== undefined && held.length !== input.equals) {
    fail(`expected exactly ${input.equals} actor(s) with ${browser.testId(input.testid)}`
      + `${contains ? ` containing "${contains}"` : ''}, found ${held.length} (${detail})`);
  }
  if (input.maxEach !== undefined) {
    const maxEach = input.maxEach;
    const greedy = counts.filter(([, count]) => count > maxEach);
    if (greedy.length) {
      fail(`${greedy.map(([name, count]) => `${name} has ${count}`).join(', ')} `
        + `— no actor may hold more than ${input.maxEach} (${detail})`);
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

export function browserApplicationBoundary<Arguments, Result>(
  implementation: (arguments_: Arguments) => Result | Promise<Result>,
): (arguments_: Arguments) => Promise<Result> {
  return async (args: Arguments): Promise<Result> => {
    try {
      return await implementation(args);
    } catch (error) {
      if (errorField(error, 'classification') || harnessBrowserFailure(error)) throw error;
      if (isExpectedBrowserFailure(error)) {
        throw new ActionApplicationFailure(String(errorField(error, 'message') ?? error));
      }
      throw error;
    }
  };
}

function contractBrowserAction<Input, Result>(
  implementation: (arguments_: BrowserArguments<Input>) => Result | Promise<Result>,
): ActionImplementation {
  const bounded = browserApplicationBoundary(implementation);
  return (arguments_: ActionImplementationArguments) => bounded({
    input: arguments_.input as Input,
    capabilities: arguments_.capabilities as unknown as BrowserCapabilities,
    signal: arguments_.signal,
  });
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
  pressKey: contractBrowserAction(pressKey),
  recordNumber: contractBrowserAction(recordNumber),
  reload: contractBrowserAction(reload),
  typeInto: contractBrowserAction(typeInto),
  wait: contractBrowserAction(wait),
});

if (Object.keys(BROWSER_ACTION_IMPLEMENTATIONS).sort().join('\0') !== BROWSER_ACTION_IDS.join('\0')) {
  throw new Error('browser action implementation registry does not match its declared action ids');
}
