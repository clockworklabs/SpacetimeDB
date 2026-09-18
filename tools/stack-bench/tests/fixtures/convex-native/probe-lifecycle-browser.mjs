import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
const [url, username, output] = process.argv.slice(2);
assert(url && username && output && process.env.PLAYWRIGHT_MODULE);
const evidence = { result: 'running', url, username };
const save = () => writeFileSync(output, JSON.stringify(evidence, null, 2));
save();
let browser;
try {
  const { chromium } = await import(process.env.PLAYWRIGHT_MODULE);
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  await page.goto(url);
  await page.locator('#signup-username').fill(username);
  await page.locator('#signup-password').fill('LifecycleBrowser42');
  await page.locator('#signup-submit').click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === 'Signed in');
  assert.equal(await page.locator('#current-user').textContent(), username);
  assert(await page.evaluate(() => typeof window.getSessionToken() === 'string'));
  await page.locator('#purchase').click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === 'Purchased');
  await page.reload();
  await page.waitForFunction(name => document.querySelector('#current-user').textContent === name, username);
  evidence.result = 'passed';
} catch (error) { evidence.result = 'failed'; evidence.error = error.stack; process.exitCode = 1; }
finally { if (browser) await browser.close(); save(); }
console.log(JSON.stringify(evidence));
