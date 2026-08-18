#!/usr/bin/env node
// Verify the named actions in the level spec actually answer, before anything
// is graded against them.
//
// Contention and volume tests need several writes to land at the same instant,
// which clicking cannot do — browser clicks arrive milliseconds apart and each
// request finishes before the next starts. The spec therefore names the write
// actions so the harness can issue them directly. When one is missing, the
// symptom downstream is a criterion quietly reporting INCONCLUSIVE and being
// subtracted from that run's denominator, which is how one stack ended up
// scored out of 48 and another out of 50 for the same level. Better to say so
// here, in one line, before any of that happens.
//
// Presence, not behaviour: a 404 means the action is not there. Anything else —
// including a refusal — means it exists and answered, which is all this checks.
// Whether it refuses the RIGHT things is what the invariant suites are for.
//
// Nothing here is allowed to mutate the database: every probe is unauthenticated
// or deliberately malformed, so a working app rejects it.
//
// Usage:
//   node check-actions.mjs --backend spacetime --app <dir> [--out report.json]
//   node check-actions.mjs --backend postgres --url http://localhost:6573

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { emptyArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.mjs';
import { executeStackCapability } from '../src/stacks/stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.mjs';

import { STACK_BENCH_ROOT } from '../src/project-paths.mjs';

const TRACKS = join(STACK_BENCH_ROOT, 'tracks');

function parseArgs(argv) {
  const a = {};
  for (let i = 2; i < argv.length; i++) {
    const k = argv[i];
    if (k === '--backend') a.backend = argv[++i];
    else if (k === '--url') a.url = argv[++i];
    else if (k === '--app') a.app = argv[++i];
    else if (k === '--out') a.out = argv[++i];
    else if (k === '--track') a.track = argv[++i];
    else if (k === '--quiet') a.quiet = true;
    else if (k === '--parent-attempt-id') a.parentAttemptId = argv[++i];
    else { console.error(`Unknown arg ${k}`); process.exit(2); }
  }
  if (!a.backend) { console.error('--backend is required'); process.exit(2); }
  return a;
}

const args = parseArgs(process.argv);

// The actions come from the track, not from this file: a track that names none
// (chat does not) is skipped rather than reported as six missing endpoints.
// Probes are chosen to be refused by a correct app — no credentials, or
// arguments that cannot identify a real row — so a check never writes anything.
const track = args.track ? JSON.parse(readFileSync(join(TRACKS, args.track, 'track.json'), 'utf8')) : null;
const ACTIONS = (track?.actions ?? []).map(a => ({ ...a, http: { method: 'POST', path: a.path } }));
if (!ACTIONS.length) {
  if (!args.quiet) console.log(`  no named actions declared for track "${args.track ?? '(none)'}" — nothing to check`);
  if (args.out) {
    const id = `${args.parentAttemptId ?? 'actions'}-action-check`;
    writeArtifact(args.out, { kind: 'action_check', id,
      attempt: { id, parentId: args.parentAttemptId ?? null },
      identities: emptyArtifactIdentities({ stackAdapter: { id: args.backend } }),
      payload: { backend: args.backend, results: [], missing: [] } });
  }
  process.exit(0);
}

// SpacetimeDB control targets come from the authenticated lease. Client config
// is app-controlled input and may use environment expressions rather than
// literals; it is neither authoritative nor safe for harness operations.
const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
const spacetime = executeStackCapability(adapter, 'grading', 'context',
  { requireBuildContainer: false });

async function probe(action) {
  try {
    const request = executeStackCapability(adapter, 'named-action', 'request',
      { action, input: { args: action.args }, spacetime, url: args.url });
    if (!request.url) return { ok: false, status: 0, note: 'no --url given for a server-based backend' };
    const r = await fetch(request.url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: request.body,
    });
    return { ok: r.status !== 404, status: r.status,
      note: r.status === 404 ? request.missingNote : '' };
  } catch (e) {
    return { ok: false, status: 0, note: String(e.message).split('\n')[0].slice(0, 90) };
  }
}

const results = [];
for (const a of ACTIONS) results.push({ id: a.id, ...(await probe(a)) });

const missing = results.filter(r => !r.ok);
if (!args.quiet) {
  for (const r of results) {
    console.log(`  ${r.ok ? 'present' : 'MISSING'}  ${r.id.padEnd(11)} ${r.status ? `HTTP ${r.status}` : ''} ${r.note}`);
  }
  console.log(missing.length
    ? `\n${missing.length} named action(s) missing — contention and volume tests cannot be issued against this app.`
    : '\nall named actions answer.');
}
if (args.out) {
  const id = `${args.parentAttemptId ?? 'actions'}-action-check`;
  writeArtifact(args.out, { kind: 'action_check', id,
    attempt: { id, parentId: args.parentAttemptId ?? null },
    identities: emptyArtifactIdentities({ stackAdapter: { id: args.backend } }),
    payload: { backend: args.backend, results, missing: missing.map(m => m.id) } });
}
process.exit(missing.length ? 1 : 0);
