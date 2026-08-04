#!/usr/bin/env node
// Stack Bench functional grader.
//
// Executes versioned scenario specs against a running app using multiple
// isolated browser contexts (one per actor), and scores each feature 0-3 from
// observed behavior only. Mechanically enforces the rules human graders applied:
//   - JS console errors during a feature cap it at 2/3
//   - a feature that only works after a reload caps at 1/3
//   - untestable (setup failed) scores 0
//
// The grader never reloads a page except in the refresh probe, so "realtime"
// means realtime.
//
// Usage: node grade.mjs --url http://localhost:6173 --level 1 [--out report.json]
//                      [--label spacetime-l1] [--headed] [--feature N]
// Exit codes: 0 = graded (any score), 2 = usage/infra error.

import { chromium } from 'playwright';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_WITHIN = 5000;
const MAX_PER_FEATURE = 3;

function parseArgs(argv) {
  const args = { level: 1, headed: false };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--url': args.url = argv[++i]; break;
      case '--level': args.level = parseInt(argv[++i], 10); break;
      case '--out': args.out = argv[++i]; break;
      case '--label': args.label = argv[++i]; break;
      case '--feature': args.feature = parseInt(argv[++i], 10); break;
      case '--spec': args.spec = argv[++i]; break;
      case '--media': args.media = argv[++i]; break;
      case '--trace': args.trace = true; break;
      case '--headed': args.headed = true; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!args.url) {
    console.error('Usage: node grade.mjs --url <app-url> --level <N> [--out <file>] [--label <s>] [--feature <N>]');
    process.exit(2);
  }
  return args;
}

const tid = id => `[data-testid="${id}"]`;
const uniq = () => Math.random().toString(36).slice(2, 7);

// ─── Actor: an isolated browser context with its own identity ────────────────

class Actor {
  constructor(name, page) {
    this.name = name;
    this.page = page;
    this.consoleErrors = [];
    page.on('console', m => {
      if (m.type() === 'error') this.consoleErrors.push(m.text().slice(0, 200));
    });
    page.on('pageerror', e => this.consoleErrors.push(`pageerror: ${e.message.slice(0, 200)}`));
  }
  loc(testid, { contains, scope } = {}) {
    // `scope` narrows the search to inside a specific container (e.g. the badge
    // belonging to ONE room), so a stale element elsewhere can't satisfy it.
    const root = scope
      ? this.page.locator(tid(scope.testid), { hasText: scope.contains }).first()
      : this.page;
    return contains
      ? root.locator(tid(testid), { hasText: contains }).first()
      : root.locator(tid(testid)).first();
  }
}

// Scenario strings may reference a run-scoped room as "{room:base}".
const expand = (s, ctx) =>
  typeof s === 'string' ? s.replace(/\{room:([^}]+)\}/g, (_, b) => ctx.roomName(b)) : s;

// ─── Step execution ──────────────────────────────────────────────────────────

async function enterRoom(actor, roomName) {
  const item = actor.page.locator(tid('room-item'), { hasText: roomName }).first();
  await item.waitFor({ state: 'visible', timeout: DEFAULT_WITHIN });
  await item.click();
  const input = actor.loc('message-input');
  if (!(await input.isVisible().catch(() => false))) {
    await actor.page.waitForTimeout(750);
    // Apps may implement click-to-join then click-to-enter.
    if (!(await input.isVisible().catch(() => false))) await item.click();
  }
  await input.waitFor({ state: 'visible', timeout: DEFAULT_WITHIN });
}

async function runStep(step, actors, ctx) {
  const actor = actors.get(step.actor);
  if (!actor) throw new Error(`unknown actor "${step.actor}"`);
  const page = actor.page;

  switch (step.do) {
    case 'register': {
      await page.locator(tid('name-input')).first().fill(`${step.name}-${ctx.scope}`);
      await page.locator(tid('name-submit')).first().click();
      await page.locator(tid('room-list')).first()
        .waitFor({ state: 'attached', timeout: DEFAULT_WITHIN });
      return;
    }
    case 'createRoom': {
      const roomName = ctx.roomName(step.room);
      const nameInput = page.locator(tid('room-name-input')).first();
      if (!(await nameInput.isVisible().catch(() => false))) {
        await page.locator(tid('room-create')).first().click();
      }
      await nameInput.fill(roomName);
      await page.locator(tid('room-name-submit')).first().click();
      // Must arrive via the app's live update path — no reload.
      await page.locator(tid('room-item'), { hasText: roomName }).first()
        .waitFor({ state: 'visible', timeout: DEFAULT_WITHIN });
      return;
    }
    case 'enterRoom': return enterRoom(actor, ctx.roomName(step.room));
    case 'send': {
      const input = actor.loc('message-input');
      await input.fill(step.text);
      await input.press('Enter');
      return;
    }
    case 'typeInto': {
      const input = actor.loc('message-input');
      await input.click();
      await input.type(step.text, { delay: 40 });
      return;
    }
    case 'clearInput': {
      await actor.loc('message-input').fill('');
      return;
    }
    case 'click': {
      await actor.loc(step.testid).click({ timeout: step.within ?? DEFAULT_WITHIN });
      return;
    }
    case 'reload': {
      await page.reload({ waitUntil: 'domcontentloaded' });
      await page.waitForTimeout(step.settleMs ?? 2500);
      return;
    }
    case 'wait': return page.waitForTimeout(step.ms);
    case 'expect': return runExpect(actor, step, ctx);
    default: throw new Error(`unknown action "${step.do}"`);
  }
}

async function runExpect(actor, step, ctx = { roomName: x => x }) {
  const within = step.within ?? DEFAULT_WITHIN;
  const contains = expand(step.contains, ctx);
  const scope = step.in ? { testid: step.in.testid, contains: expand(step.in.contains, ctx) } : undefined;
  const where = scope ? ` inside ${tid(scope.testid)} "${scope.contains}"` : '';
  const loc = actor.loc(step.testid, { contains, scope });

  if (step.absent) {
    // Poll until gone (or never present) — hidden OR detached both count.
    const deadline = Date.now() + within;
    for (;;) {
      const visible = await loc.isVisible().catch(() => false);
      if (!visible) return;
      if (Date.now() > deadline) {
        throw new Error(`${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}${where} still visible after ${within}ms`);
      }
      await actor.page.waitForTimeout(250);
    }
  }

  await loc.waitFor({ state: 'visible', timeout: within }).catch(() => {
    throw new Error(`${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}${where} not visible within ${within}ms`);
  });

  if (step.count !== undefined || step.maxCount !== undefined) {
    const all = scope
      ? actor.page.locator(tid(scope.testid), { hasText: scope.contains }).first().locator(tid(step.testid))
      : (contains ? actor.page.locator(tid(step.testid), { hasText: contains }) : actor.page.locator(tid(step.testid)));
    const n = await all.count();
    if (step.count !== undefined && n !== step.count) {
      throw new Error(`expected exactly ${step.count} ${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}, found ${n}`);
    }
    if (step.maxCount !== undefined && n > step.maxCount) {
      throw new Error(`expected at most ${step.maxCount} ${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}, found ${n}`);
    }
  }

  if (step.notContains) {
    const text = (await loc.innerText().catch(() => '')) || '';
    if (text.includes(step.notContains)) {
      throw new Error(`${tid(step.testid)} unexpectedly contains "${step.notContains}" (text: "${text.trim().slice(0, 80)}")`);
    }
  }
}

// ─── Feature grading ─────────────────────────────────────────────────────────

async function gradeFeature(browser, feature, args, runCtx) {
  // Features share the app's DATABASE even though each gets fresh browser
  // contexts, so user and room names are scoped per feature — otherwise a
  // defect in one feature (e.g. a hijacked account) corrupts later setups.
  const scope = `${runCtx.runId}f${feature.id}`;
  const ctx = { ...runCtx, scope, roomName: base => `${base}-${scope}` };
  const actors = new Map();
  const contexts = [];
  const slug = `${args.label ?? 'run'}-f${feature.id}`;
  for (const name of feature.actors) {
    // Isolated storage per actor. Video is per-context, so each actor gets its
    // own recording — you can watch what every participant saw, side by side.
    const context = await browser.newContext(
      args.media ? { recordVideo: { dir: args.media, size: { width: 1280, height: 800 } } } : {}
    );
    if (args.trace) await context.tracing.start({ screenshots: true, snapshots: true });
    const page = await context.newPage();
    page.setDefaultTimeout(DEFAULT_WITHIN);
    await page.goto(args.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
    actors.set(name, new Actor(name, page));
    contexts.push({ context, name, page });
  }

  const closeAll = async () => {
    for (const { context, name, page } of contexts) {
      if (args.trace) {
        await context.tracing.stop({ path: join(args.media ?? '.', `${slug}-${name}.trace.zip`) }).catch(() => {});
      }
      const video = args.media ? page.video() : null;
      await context.close();                     // video is only finalized on close
      if (video) {
        await video.saveAs(join(args.media, `${slug}-${name}.webm`)).catch(() => {});
        await video.delete().catch(() => {});     // drop Playwright's hashed original
      }
    }
  };

  const featureMax = feature.max ?? MAX_PER_FEATURE;
  const result = {
    id: feature.id, name: feature.name, score: 0, max: featureMax,
    criteria: [], caps: [], consoleErrors: [],
  };

  try {
    // Setup is not scored, but a failure makes the feature untestable (0).
    for (const step of feature.setup) {
      await runStep(step, actors, ctx);
    }
  } catch (err) {
    result.setupError = err.message;
    result.caps.push('setup-failed → 0');
    for (const c of feature.criteria) {
      result.criteria.push({ id: c.id, points: c.points, passed: false, detail: 'setup failed' });
    }
    await closeAll();
    return result;
  }

  let refreshDependent = false;
  for (const criterion of feature.criteria) {
    let passed = true, detail = null;
    try {
      for (const step of criterion.steps) await runStep(step, actors, ctx);
    } catch (err) {
      passed = false;
      detail = err.message;
      if (args.media) {
        const shotActor = actors.get(criterion.steps[criterion.steps.length - 1]?.actor) ?? actors.values().next().value;
        const shot = join(args.media, `${slug}-${criterion.id}.png`);
        await shotActor.page.screenshot({ path: shot, fullPage: true }).catch(() => {});
        result.screenshots = [...(result.screenshots ?? []), shot];
      }
      // Refresh probe: does the assertion pass once the page is reloaded?
      // If so the feature is refresh-dependent, not realtime.
      const failing = criterion.steps[criterion.steps.length - 1];
      if (failing?.do === 'expect' && !failing.absent) {
        try {
          const actor = actors.get(failing.actor);
          await actor.page.reload({ waitUntil: 'domcontentloaded' });
          await actor.page.waitForTimeout(2000);
          await runExpect(actor, { ...failing, within: 6000 });
          refreshDependent = true;
          detail += ' — PASSES AFTER RELOAD (refresh-dependent)';
        } catch { /* genuinely absent, not just refresh-dependent */ }
      }
    }
    result.criteria.push({ id: criterion.id, desc: criterion.desc, points: criterion.points, passed, detail });
    if (passed) result.score += criterion.points;
  }

  for (const actor of actors.values()) {
    for (const e of actor.consoleErrors) result.consoleErrors.push(`[${actor.name}] ${e}`);
  }

  // Caps, applied in severity order.
  if (feature.max === undefined && refreshDependent && result.score > 1) {
    result.caps.push('refresh-dependent → capped at 1');
    result.score = 1;
  }
  if (feature.max === undefined && result.consoleErrors.length && result.score > 2) {
    result.caps.push('console errors → capped at 2');
    result.score = 2;
  }

  await closeAll();
  if (args.media) result.videos = contexts.map(c => join(args.media, `${slug}-${c.name}.webm`));
  return result;
}

async function countExistingRooms(browser, args, runId) {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    page.setDefaultTimeout(DEFAULT_WITHIN);
    await page.goto(args.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
    await page.locator(tid('name-input')).first().fill(`preflight-${runId}`);
    await page.locator(tid('name-submit')).first().click();
    await page.locator(tid('room-list')).first().waitFor({ state: 'attached', timeout: DEFAULT_WITHIN });
    await page.waitForTimeout(1500);
    return await page.locator(tid('room-item')).count();
  } catch {
    return -1;                                     // couldn't determine
  } finally {
    await context.close();
  }
}

// ─── Main ────────────────────────────────────────────────────────────────────

async function main() {
  const args = parseArgs(process.argv);
  const specPath = args.spec ? args.spec : join(ROOT, 'scenarios', `level-${String(args.level).padStart(2, '0')}.json`);
  let spec;
  try {
    spec = JSON.parse(readFileSync(specPath, 'utf8'));
  } catch {
    console.error(`No scenario spec at ${specPath}`);
    process.exit(2);
  }

  const features = args.feature ? spec.features.filter(f => f.id === args.feature) : spec.features;
  const runId = uniq();
  const ctx = { runId, roomName: base => `${base}-${runId}` };

  const browser = await chromium.launch({ headless: !args.headed });
  const report = {
    label: args.label ?? null, url: args.url, level: args.level, runId,
    total: 0, max: features.reduce((n, f) => n + (f.max ?? MAX_PER_FEATURE), 0), features: [],
  };

  // Preflight: grading a dirty database silently biases scores downward (a long
  // room/user list breaks assertions that pass on a clean app), so surface it
  // rather than letting it look like a real failure.
  report.environment = { preexistingRooms: await countExistingRooms(browser, args, runId) };
  if (report.environment.preexistingRooms > 0) {
    console.log(`WARNING: app already has ${report.environment.preexistingRooms} room(s) — ` +
      `scores are not comparable. Reset the database first ` +
      `(llm-sequential-upgrade/reset-app.sh <app-dir>).\n`);
  }

  for (const feature of features) {
    process.stdout.write(`Feature ${feature.id}: ${feature.name} ... `);
    const r = await gradeFeature(browser, feature, args, ctx);
    report.features.push(r);
    report.total += r.score;
    console.log(`${r.score}/${r.max}${r.caps.length ? ` (${r.caps.join('; ')})` : ''}`);
    for (const c of r.criteria.filter(c => !c.passed)) {
      console.log(`    FAIL ${c.id} — ${c.detail}`);
    }
  }

  await browser.close();

  console.log(`\nTOTAL ${report.total}/${report.max}`);
  if (args.out) {
    mkdirSync(dirname(args.out), { recursive: true });
    writeFileSync(args.out, JSON.stringify(report, null, 2));
    console.log(`Report written to ${args.out}`);
  }
}

main();
