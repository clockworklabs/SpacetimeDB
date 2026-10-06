import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { snapshot } from './snapshot.mjs';

const evidence = { result: 'running' };
writeFileSync('restart-evidence.json', JSON.stringify(evidence));
try {
  const before = JSON.parse(readFileSync('probe-evidence.json', 'utf8'));
  assert.equal(before.result, 'passed', 'Run the slice before restarting');
  const result = await snapshot(process.env.CONVEX_SELF_HOSTED_URL, process.env.CONVEX_SELF_HOSTED_ADMIN_KEY);
  assert.deepEqual(result.tables.items, before.final);
  assert.deepEqual(result.tables.orders, before.orders);
  Object.assign(evidence, result, { result: 'passed' });
} catch (error) { evidence.result = 'failed'; evidence.error = String(error); throw error; }
finally { writeFileSync('restart-evidence.json', JSON.stringify(evidence, null, 2)); }
console.log('Restart preserved exact stock/order rows and IDs');
