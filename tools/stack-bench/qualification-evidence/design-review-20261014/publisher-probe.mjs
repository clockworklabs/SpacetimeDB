import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, unlinkSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const out = dirname(fileURLToPath(import.meta.url));
const root = resolve(out, '../..');
const compiled = resolve(root, '.loop-test/critical-review-20261014/tests/campaign-publish.contract.js');
const temporary = resolve(dirname(compiled), 'publisher-review-probe.js');
let source = readFileSync(compiled, 'utf8');
const replace = (from, to) => { assert.equal(source.split(from).length, 2); source = source.replace(from, to); };
replace('definition.repetitions = 2;', `definition.repetitions = 1;
    definition.conditions[0].productionQuality = true;
    definition.conditions.push({ ...definition.conditions[0], id: 'model-free-other', productionQuality: false });`);
replace("const payload = createCampaignUpload(campaign, 'fixture-suite');", `const payload = createCampaignUpload(campaign, 'fixture-suite');
    writeFileSync(${JSON.stringify(resolve(out, 'publisher-evidence.json'))}, JSON.stringify({
      kind: 'synthetic-local-publisher-probe',
      limits: ['Synthetic retained evidence. Local test server only. No model runs or external upload.',
        'One completed and one invalid attempt; proves schema loss, not published aggregation.'],
      conditions: plan.conditions,
      plannedAttempts: plan.attempts.map(a => ({id: a.id, condition: a.condition})),
      payload
    }, null, 2));`);
writeFileSync(resolve(out, 'publisher-probe-generated.txt'), source);
writeFileSync(temporary, source);
try {
  const result = spawnSync(process.execPath, ['--test', temporary], { cwd: root, encoding: 'utf8' });
  writeFileSync(resolve(out, 'publisher-probe-test.txt'), result.stdout + result.stderr);
  process.stdout.write(result.stdout + result.stderr);
  process.exitCode = result.status ?? 1;
} finally { unlinkSync(temporary); }
