// Does a client keep its IDENTITY across a SpacetimeDB host restart?
//
// A changed token string proves nothing on its own — a JWT can be re-issued with
// a new iat while carrying the same subject, which is the same identity. What
// matters is the claims, and whether the module still recognises the user.
import { chromium } from 'playwright';

const url = process.argv[2], restartCmd = process.argv[3];
const claims = t => {
  try { return JSON.parse(Buffer.from(t.split('.')[1], 'base64url').toString()); }
  catch { return null; }
};

const b = await chromium.launch();
const p = await b.newContext().then(c => c.newPage());
p.setDefaultTimeout(8000);
await p.goto(url, { waitUntil: 'domcontentloaded' });
await p.locator('[data-testid="name-input"]').first().fill('ProbeUser');
await p.locator('[data-testid="name-submit"]').first().click();
await p.waitForTimeout(3000);

const read = async () => {
  const t = await p.evaluate(() => localStorage.getItem('auth_token'));
  return { token: t ?? '', claims: t ? claims(t) : null };
};

const before = await read();
console.log('BEFORE  sub:', before.claims?.sub, '| iss:', before.claims?.iss, '| iat:', before.claims?.iat);

if (restartCmd) {
  const { execFileSync } = await import('node:child_process');
  execFileSync('bash', ['-c', restartCmd], { stdio: 'ignore', timeout: 300000 });
}
await p.waitForTimeout(8000);
await p.reload({ waitUntil: 'domcontentloaded' });
await p.waitForTimeout(8000);

const after = await read();
console.log('AFTER   sub:', after.claims?.sub, '| iss:', after.claims?.iss, '| iat:', after.claims?.iat);
console.log('');
console.log('token string identical :', before.token === after.token);
console.log('SUBJECT identical      :', before.claims?.sub === after.claims?.sub, '  <-- this is the identity');
console.log('app shows registration :', await p.locator('[data-testid="name-input"]').first().isVisible().catch(() => false));
await b.close();
