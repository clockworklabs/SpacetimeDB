#!/usr/bin/env node
// Emit the human grading sheet for a level from the scenario files, so the
// manual pass and the automated one are scoring the same things. Hand-written
// rubrics drift the moment a criterion changes, and a disagreement between
// human and grader is only interesting if both were asked the same question.
//
// Usage: node make-rubric.mjs [--level 1] [--out ../GRADING_RUBRIC.md]

import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadTrack, DEFAULT_TRACK } from '../src/composition/tracks.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));

const args = { level: '01', track: DEFAULT_TRACK, out: null };
for (let i = 2; i < process.argv.length; i++) {
  if (process.argv[i] === '--level') args.level = String(process.argv[++i]).padStart(2, '0');
  else if (process.argv[i] === '--track') args.track = process.argv[++i];
  else if (process.argv[i] === '--out') args.out = process.argv[++i];
}

const track = loadTrack(args.track);
args.out ??= join(HERE, '..', `GRADING_RUBRIC${track.slug ? `_${track.slug}` : ''}.md`);

// Ports follow the same per-track offset the harness uses, so the URLs in the
// sheet point at the app the grader is actually scoring.
const off = track.portOffset;
const URLS = { SpacetimeDB: 6173 + off, PostgreSQL: 6273 + off, MongoDB: 6373 + off };
const files = readdirSync(track.scenarios)
  .filter(f => f.startsWith(`${args.level}-`) && f.endsWith('.json') && !f.includes('wip'));

const lines = [`# Level ${Number(args.level)} manual grading`, '',
  'Each numbered item is one point. Judge only what you can see in the browser:',
  'if something is untestable, score 0, and when in doubt score lower.', '',
  '| Backend | URL |', '|---|---|',
  ...Object.entries(URLS).map(([k, p]) => `| ${k} | http://localhost:${p} |`), ''];

let n = 0;
const totals = [];
for (const file of files) {
  const spec = JSON.parse(readFileSync(join(track.scenarios, file), 'utf8'));
  const group = file.replace(/^\d+-|\.json$/g, '').replace(/-/g, ' ');
  const before = n;
  lines.push('---', '', `## ${group[0].toUpperCase()}${group.slice(1)}`, '');
  for (const f of spec.features ?? []) {
    lines.push(`**${f.name}**`, '');
    if (f.note) lines.push(`> ${f.note}`, '');
    for (const c of f.criteria ?? []) lines.push(`${++n}. ${c.desc}`);
    lines.push('');
  }
  totals.push([group, n - before]);
}

lines.push('---', '',
  '## What the grader cannot see', '',
  'This is where a human pass earns its time. Note anything under these headings',
  'even though none of it is scored:', '',
  '- **Hollow UI** — a control that exists and does nothing, or a state that renders but never updates',
  '- **Jank** — flicker, list jumping, focus stolen while typing, scroll position lost, a spinner that never resolves',
  '- **Latency that technically passes** — something arriving in two seconds that should feel instant',
  '- **Silent failure** — an action that does nothing and says nothing',
  '- **Anything that made you hesitate.** If it felt off, write it down; that instinct is what the harness has not learned yet.',
  '', '---', '', '## Recording your scores', '',
  `| | ${Object.keys(URLS).join(' | ')} |`,
  `|---|${Object.keys(URLS).map(() => '---').join('|')}|`,
  ...totals.map(([g, t]) => `| ${g} (${t}) |${Object.keys(URLS).map(() => ' |').join('')}`),
  `| **Total (${n})** |${Object.keys(URLS).map(() => ' |').join('')}`,
  '',
  'Where you and the grader disagree is the useful part: either the grader has a',
  'hole, or a criterion is written loosely enough to pass something it should not.',
  '');

writeFileSync(args.out, lines.join('\n'));
console.log(`wrote ${args.out} (${n} points across ${files.length} scenario files)`);
