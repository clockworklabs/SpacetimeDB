#!/usr/bin/env node
// Stack Bench UI-contract linter.
//
// Walks the app's golden path (register -> create room -> enter room -> send a
// message) using ONLY contract testids, checking every lintable hook for the
// requested level along the way. Hooks with stage "scenario" need a second
// user or timing and are reported as SCENARIO (not checked here).
//
// Usage: node lint.mjs --url http://localhost:6173 --level 3 [--json] [--headed]
// Exit codes: 0 = all lintable hooks pass, 1 = failures, 2 = usage/infra error.

import { chromium } from 'playwright';
import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const CONTRACTS_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'contracts');
const CHECK_TIMEOUT = 5000;

function parseArgs(argv) {
  const args = { level: 1, json: false, headed: false };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--url': args.url = argv[++i]; break;
      case '--level': args.level = parseInt(argv[++i], 10); break;
      case '--json': args.json = true; break;
      case '--out': args.out = argv[++i]; break;
      case '--label': args.label = argv[++i]; break;
      case '--headed': args.headed = true; break;
      default:
        console.error(`Unknown argument: ${argv[i]}`);
        process.exit(2);
    }
  }
  if (!args.url || !Number.isInteger(args.level) || args.level < 1) {
    console.error('Usage: node lint.mjs --url <app-url> --level <N> [--json] [--headed]');
    process.exit(2);
  }
  return args;
}

function loadHooks(level) {
  const files = readdirSync(CONTRACTS_DIR).filter(f => /^level-\d+\.json$/.test(f)).sort();
  const hooks = [];
  for (const f of files) {
    const contract = JSON.parse(readFileSync(join(CONTRACTS_DIR, f), 'utf8'));
    if (contract.level <= level) hooks.push(...contract.hooks);
  }
  if (hooks.length === 0) {
    console.error(`No contracts found for level ${level} in ${CONTRACTS_DIR}`);
    process.exit(2);
  }
  return hooks;
}

const tid = id => `[data-testid="${id}"]`;
const uniq = Date.now().toString(36).slice(-5);

async function checkHook(page, hook, results) {
  const loc = page.locator(tid(hook.id)).first();
  try {
    if (hook.revealedBy && !(await loc.count())) {
      await page.locator(tid(hook.revealedBy)).first().click({ timeout: CHECK_TIMEOUT });
    }
    await loc.waitFor({
      state: hook.check === 'visible' ? 'visible' : 'attached',
      timeout: CHECK_TIMEOUT,
    });
    results.push({ id: hook.id, status: 'PASS' });
    return true;
  } catch {
    results.push({
      id: hook.id,
      status: 'FAIL',
      detail: `no element matching ${tid(hook.id)} became ${hook.check}` +
        (hook.revealedBy ? ` (after clicking ${tid(hook.revealedBy)})` : '') +
        ` — expected: ${hook.element}`,
    });
    return false;
  }
}

async function run() {
  const args = parseArgs(process.argv);
  const hooks = loadHooks(args.level);
  const byStage = stage => hooks.filter(h => h.stage === stage);
  const results = [];
  const blocked = stage => {
    for (const h of hooks.filter(x => x.stage === stage)) {
      results.push({ id: h.id, status: 'BLOCKED', detail: 'earlier golden-path step failed' });
    }
  };

  const browser = await chromium.launch({ headless: !args.headed });
  const page = await browser.newContext().then(c => c.newPage());
  page.setDefaultTimeout(CHECK_TIMEOUT);

  try {
    await page.goto(args.url, { waitUntil: 'domcontentloaded', timeout: 15000 });

    // Stage: landing (fresh identity -> registration UI must be shown)
    let ok = true;
    for (const h of byStage('landing')) ok = (await checkHook(page, h, results)) && ok;

    if (ok) {
      await page.locator(tid('name-input')).first().fill(`lint-${uniq}`);
      await page.locator(tid('name-submit')).first().click();
    }

    // Stage: main (registered -> rooms + online users)
    if (ok) {
      for (const h of byStage('main')) ok = (await checkHook(page, h, results)) && ok;
    } else blocked('main');

    // Create a room, then check hooks that need one to exist. The created room
    // must arrive in the list via the app's live update path (no reload) — this
    // is itself a contract requirement, so we wait for OUR room, not any room.
    const ourRoom = page.locator(tid('room-item'), { hasText: `lint-room-${uniq}` }).first();
    if (ok) {
      const nameInput = page.locator(tid('room-name-input')).first();
      if (!(await nameInput.isVisible().catch(() => false))) {
        await page.locator(tid('room-create')).first().click();
      }
      await nameInput.fill(`lint-room-${uniq}`);
      await page.locator(tid('room-name-submit')).first().click();
      for (const h of byStage('main-after-create')) {
        if (h.id === 'room-item') {
          try {
            await ourRoom.waitFor({ state: 'visible', timeout: CHECK_TIMEOUT });
            results.push({ id: h.id, status: 'PASS' });
          } catch {
            results.push({
              id: h.id, status: 'FAIL',
              detail: `created room "lint-room-${uniq}" but no ${tid(h.id)} containing it appeared without a reload`,
            });
            ok = false;
          }
        } else ok = (await checkHook(page, h, results)) && ok;
      }
    } else blocked('main-after-create');

    // Stage: room (enter our room, send a probe message via Enter).
    // Apps may implement click-to-join then click-to-enter; allow a second click.
    if (ok) {
      await ourRoom.click();
      const msgInput = page.locator(tid('message-input')).first();
      if (!(await msgInput.isVisible().catch(() => false))) {
        await page.waitForTimeout(750);
        if (!(await msgInput.isVisible().catch(() => false))) await ourRoom.click();
      }
      for (const h of byStage('room')) await checkHook(page, h, results); // non-blocking: check all
      ok = !results.some(r => r.status === 'FAIL' && hooks.find(h => h.id === r.id)?.stage === 'room');
    } else blocked('room');

    if (ok && !results.some(r => r.id === 'message-input' && r.status !== 'PASS')) {
      const probe = `lint probe ${uniq}`;
      await page.locator(tid('message-input')).first().fill(probe);
      await page.locator(tid('message-input')).first().press('Enter');
      for (const h of byStage('room-after-send')) {
        const loc = page.locator(tid(h.id), { hasText: probe }).first();
        try {
          await loc.waitFor({ state: 'visible', timeout: CHECK_TIMEOUT });
          results.push({ id: h.id, status: 'PASS' });
        } catch {
          results.push({
            id: h.id, status: 'FAIL',
            detail: `sent "${probe}" via Enter but no ${tid(h.id)} containing it appeared — expected: ${h.element}`,
          });
        }
      }
    } else blocked('room-after-send');

    for (const h of byStage('scenario')) {
      results.push({ id: h.id, status: 'SCENARIO', detail: h.note });
    }
  } catch (err) {
    console.error(`Golden path aborted: ${err.message}`);
    for (const h of hooks) {
      if (!results.some(r => r.id === h.id)) {
        results.push({ id: h.id, status: h.stage === 'scenario' ? 'SCENARIO' : 'BLOCKED', detail: 'golden path aborted' });
      }
    }
  } finally {
    await browser.close();
  }

  const failures = results.filter(r => r.status === 'FAIL' || r.status === 'BLOCKED');
  const report = {
    label: args.label ?? null,
    url: args.url,
    level: args.level,
    pass: failures.length === 0,
    counts: {
      lintable: results.filter(r => r.status !== 'SCENARIO').length,
      pass: results.filter(r => r.status === 'PASS').length,
      fail: results.filter(r => r.status === 'FAIL').length,
      blocked: results.filter(r => r.status === 'BLOCKED').length,
      scenario: results.filter(r => r.status === 'SCENARIO').length,
    },
    results,
  };
  if (args.out) {
    const { writeFileSync, mkdirSync } = await import('node:fs');
    mkdirSync(dirname(args.out), { recursive: true });
    writeFileSync(args.out, JSON.stringify(report, null, 2));
    if (!args.json) console.log(`\nLint report written to ${args.out}`);
  }
  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    for (const r of results) {
      console.log(`${r.status.padEnd(9)} ${r.id}${r.detail ? ` — ${r.detail}` : ''}`);
    }
    console.log(failures.length === 0
      ? `\nCONTRACT LINT PASS (${results.filter(r => r.status === 'PASS').length} hooks)`
      : `\nCONTRACT LINT FAIL (${failures.length} hook(s) missing or blocked)`);
  }
  process.exit(failures.length === 0 ? 0 : 1);
}

run();
