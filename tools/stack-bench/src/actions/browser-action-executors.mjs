import { ActionApplicationFailure } from './action-contract.mjs';
import { settledLocatorCount } from '../evidence/browser-evidence.mjs';
import { harnessBrowserFailure } from '../evidence/harness-errors.mjs';

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
  'expectStable',
  'fill',
  'pressKey',
  'recordNumber',
  'reload',
  'typeInto',
  'wait',
].sort());

const fail = message => { throw new ActionApplicationFailure(message); };

function actorFor(capabilities, name) {
  const actor = capabilities.actors.get(name);
  if (!actor) fail(`unknown actor "${name}"`);
  return actor;
}

function interaction(capabilities) {
  return capabilities['browser-interaction'];
}

function observation(capabilities) {
  return capabilities['browser-observation'];
}

async function readValue(loc) {
  const tag = await loc.evaluate(element => element.tagName).catch(() => '');
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') {
    return (await loc.inputValue().catch(() => '')) || '';
  }
  return (await loc.innerText().catch(() => '')) || '';
}

export function parseRenderedNumber(text) {
  const match = (text ?? '').replace(/[,\u00a0]/g, '').match(/-?\d+(\.\d+)?/);
  return match ? Number(match[0]) : null;
}

const pad = (index, count) => String(index).padStart(String(count).length, '0');

async function clearInput({ input, capabilities }) {
  await actorFor(capabilities, input.actor).loc('message-input').fill('');
  return { cleared: true };
}

async function click({ input, capabilities, signal }) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  await actor.loc(input.testid, { contains: browser.expand(input.contains), scope })
    .click({ timeout: input.within ?? browser.defaultWithin });
  if (input.settleMs) await browser.sleep(input.settleMs, signal);
  return { clicked: input.testid };
}

async function fill({ input, capabilities, signal }) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const loc = actor.loc(input.testid, { scope });
  await loc.waitFor({ state: 'visible', timeout: input.within ?? browser.defaultWithin });
  const text = browser.expand(input.text) ?? '';
  const tag = await loc.evaluate(element => element.tagName).catch(() => '');
  if (tag === 'SELECT') {
    await loc.selectOption(text).catch(async () => { await loc.selectOption({ label: text }); });
  } else {
    await loc.fill(text);
  }
  if (input.enter) await loc.press('Enter');
  if (input.settleMs) await browser.sleep(input.settleMs, signal);
  return { filled: input.testid };
}

async function pressKey({ input, capabilities, signal }) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  await actor.page.keyboard.press(input.key ?? 'Escape');
  await browser.sleep(input.settleMs ?? 600, signal);
  return { key: input.key ?? 'Escape' };
}

async function reload({ input, capabilities, signal }) {
  const actor = actorFor(capabilities, input.actor);
  const browser = interaction(capabilities);
  await actor.page.reload({ waitUntil: 'domcontentloaded' });
  await browser.sleep(input.settleMs ?? 2500, signal);
  return { reloaded: true };
}

async function typeInto({ input, capabilities }) {
  const actor = actorFor(capabilities, input.actor);
  const field = actor.loc('message-input');
  await field.click();
  await field.type(input.text, { delay: 40 });
  return { typed: true };
}

async function wait({ input, capabilities, signal }) {
  actorFor(capabilities, input.actor);
  await capabilities.clock.sleep(input.ms, signal);
  return { waitedMs: input.ms };
}

async function expect({ input, capabilities, signal }) {
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
      ? actor.page.locator(browser.testId(scope.testid), { hasText: scope.contains }).first()
        .locator(browser.testId(input.testid))
      : (contains
        ? actor.page.locator(browser.testId(input.testid), { hasText: contains })
        : actor.page.locator(browser.testId(input.testid)));
    const count = visible ? await all.count() : 0;
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
  return { visible: true };
}

async function expectElementCount({ input, capabilities, signal }) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const within = input.within ?? 10000;
  const deadline = Date.now() + within;
  const root = input.in
    ? actor.page.locator(browser.testId(input.in.testid),
      input.in.contains ? { hasText: input.in.contains } : {}).first()
    : actor.page;
  for (;;) {
    const count = await root.locator(browser.testId(input.testid),
      input.contains ? { hasText: input.contains } : {}).count();
    if (count === input.equals) return { count };
    if (Date.now() > deadline) {
      fail(`expected exactly ${input.equals} ${browser.testId(input.testid)}`
        + `${input.contains ? ` containing "${input.contains}"` : ''}, saw ${count} (after ${within}ms)`);
    }
    await browser.sleep(400, signal);
  }
}

async function expectAllPresent({ input, capabilities, signal }) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const within = input.within ?? 10000;
  const deadline = Date.now() + within;
  for (;;) {
    const counts = [];
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

async function expectStable({ input, capabilities, signal }) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const loc = actor.loc(input.testid, { contains: browser.expand(input.contains) });
  await loc.waitFor({ state: 'visible', timeout: input.within ?? browser.defaultWithin });
  const seen = [];
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

async function recordNumber({ input, capabilities }) {
  const actor = actorFor(capabilities, input.actor);
  const browser = observation(capabilities);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const loc = actor.loc(input.testid, { contains: browser.expand(input.contains), scope });
  await loc.waitFor({ state: 'visible', timeout: input.within ?? browser.defaultWithin });
  const value = parseRenderedNumber(await readValue(loc));
  if (value === null) fail(`${browser.testId(input.testid)} has no number to record`);
  browser.recorded.set(input.as, value);
  return { key: input.as, value };
}

async function expectNumber({ input, capabilities, signal }) {
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
    if (base === undefined) fail(`no number recorded as "${input.relativeTo}"`);
    equals = base + (input.plus ?? 0);
  }
  const matches = number => (equals === undefined || number === equals)
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

async function expectOrderMatches({ input, capabilities }) {
  const browser = observation(capabilities);
  const sequences = {};
  for (const name of input.actors) {
    const actor = capabilities.actors.get(name);
    if (!actor) fail(`expectOrderMatches: no actor "${name}"`);
    const texts = await actor.page.locator(browser.testId('message-item')).allInnerTexts();
    sequences[name] = texts
      .map(text => (text.match(new RegExp(`${input.prefix}-\\d+`)) || [])[0])
      .filter(Boolean);
  }
  const [first, ...rest] = input.actors;
  for (const other of rest) {
    if (sequences[first].join('|') !== sequences[other].join('|')) {
      const differentAt = sequences[first].findIndex((value, index) => value !== sequences[other][index]);
      fail(`message order differs between ${first} and ${other} at position ${differentAt}: `
        + `${first} saw ${sequences[first].slice(Math.max(0, differentAt - 1), differentAt + 2).join(',')} / `
        + `${other} saw ${sequences[other].slice(Math.max(0, differentAt - 1), differentAt + 2).join(',')}`);
    }
  }
  return { sequences };
}

async function expectAgreement({ input, capabilities, signal }) {
  const browser = observation(capabilities);
  const contains = browser.expand(input.contains);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const deadline = Date.now() + (input.within ?? 10000);
  let seen = {};
  for (;;) {
    seen = {};
    for (const name of input.actors) {
      const actor = capabilities.actors.get(name);
      if (!actor) fail(`expectAgreement: no actor "${name}"`);
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

async function expectActorsWith({ input, capabilities }) {
  const browser = observation(capabilities);
  const contains = browser.expand(input.contains);
  const scope = input.in
    ? { testid: input.in.testid, contains: browser.expand(input.in.contains) }
    : undefined;
  const counts = await Promise.all(input.actors.map(async name => {
    const actor = capabilities.actors.get(name);
    if (!actor) fail(`expectActorsWith: no actor "${name}"`);
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
    const greedy = counts.filter(([, count]) => count > input.maxEach);
    if (greedy.length) {
      fail(`${greedy.map(([name, count]) => `${name} has ${count}`).join(', ')} `
        + `— no actor may hold more than ${input.maxEach} (${detail})`);
    }
  }
  return { counts: Object.fromEntries(counts) };
}

function isExpectedBrowserFailure(error) {
  if (error instanceof ActionApplicationFailure) return true;
  if (error?.name === 'TimeoutError') return true;
  const stack = String(error?.stack ?? '');
  const message = String(error?.message ?? error ?? '');
  return /node_modules[\\/]playwright/.test(stack)
    || /^(?:locator|page|keyboard|browserContext)\./i.test(message);
}

export function browserApplicationBoundary(implementation) {
  return async args => {
    try {
      return await implementation(args);
    } catch (error) {
      if (error?.classification || harnessBrowserFailure(error)) throw error;
      if (isExpectedBrowserFailure(error)) {
        throw new ActionApplicationFailure(String(error?.message ?? error));
      }
      throw error;
    }
  };
}

export const BROWSER_ACTION_IMPLEMENTATIONS = Object.freeze({
  clearInput: browserApplicationBoundary(clearInput),
  click: browserApplicationBoundary(click),
  expect: browserApplicationBoundary(expect),
  expectActorsWith: browserApplicationBoundary(expectActorsWith),
  expectAgreement: browserApplicationBoundary(expectAgreement),
  expectAllPresent: browserApplicationBoundary(expectAllPresent),
  expectElementCount: browserApplicationBoundary(expectElementCount),
  expectNumber: browserApplicationBoundary(expectNumber),
  expectOrderMatches: browserApplicationBoundary(expectOrderMatches),
  expectStable: browserApplicationBoundary(expectStable),
  fill: browserApplicationBoundary(fill),
  pressKey: browserApplicationBoundary(pressKey),
  recordNumber: browserApplicationBoundary(recordNumber),
  reload: browserApplicationBoundary(reload),
  typeInto: browserApplicationBoundary(typeInto),
  wait: browserApplicationBoundary(wait),
});

if (Object.keys(BROWSER_ACTION_IMPLEMENTATIONS).sort().join('\0') !== BROWSER_ACTION_IDS.join('\0')) {
  throw new Error('browser action implementation registry does not match its declared action ids');
}
