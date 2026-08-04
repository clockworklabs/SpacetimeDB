#!/usr/bin/env node
// Generates the per-level "Testing Hooks" prompt appendix from the contract
// JSONs. Cumulative: appendix for level N includes hooks from levels 1..N.
//
// Usage: node gen-appendix.mjs            (regenerates all appendix files)

import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
// Two tracks: the legacy chat ladder in contracts/, and the property-ordered
// sequence in spec/contracts/. Both generate cumulative per-level appendices.
const TRACK = process.argv.includes('--track')
  ? process.argv[process.argv.indexOf('--track') + 1]
  : 'legacy';
const CONTRACTS_DIR = TRACK === 'spec' ? join(ROOT, 'spec', 'contracts') : join(ROOT, 'contracts');
const FILE_RE = TRACK === 'spec' ? /^\d+-[a-z-]+\.json$/ : /^level-\d+\.json$/;

const files = readdirSync(CONTRACTS_DIR).filter(f => FILE_RE.test(f)).sort();
const contracts = files.map(f => JSON.parse(readFileSync(join(CONTRACTS_DIR, f), 'utf8')));

for (const { level } of contracts) {
  const hooks = contracts.filter(c => c.level <= level).flatMap(c => c.hooks);
  const rows = hooks.map(h => {
    const when = h.stage === 'scenario' ? h.note : h.element;
    return `| \`${h.id}\` | ${when} |`;
  });
  const md = `

## Appendix: Testing Hooks (required)

The app is graded by an automated harness that locates elements **only** via
\`data-testid\` attributes. Add the exact test IDs below to the corresponding
elements. These are plain HTML attributes — they must not change your design,
styling, architecture, or backend in any way.

Rules:
- Attribute name is exactly \`data-testid\`; values are exactly as listed (kebab-case).
- Repeated elements (each room in the list, each message) carry the same testid on every instance.
- An element that is hidden until a menu/toggle opens still counts, as long as it is in the DOM after its toggle is clicked.
- Do not add testids beyond this list to elements that could be confused with these.

| Test ID | Element |
|---|---|
${rows.join('\n')}

Before declaring DEPLOY_COMPLETE, verify the hooks by running the contract
linter (command provided in your build instructions) and fix any failures.
`;
  const prefix = TRACK === 'spec' ? 'appendix-' : 'appendix-level-';
  const out = join(CONTRACTS_DIR, `${prefix}${String(level).padStart(2, '0')}.md`);
  writeFileSync(out, md);
  console.log(`wrote ${out} (${hooks.length} hooks)`);
}
