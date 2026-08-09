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

import { readFileSync, existsSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const TRACKS = join(dirname(fileURLToPath(import.meta.url)), 'tracks');

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
  if (args.out) writeFileSync(args.out, JSON.stringify({ backend: args.backend, results: [], missing: [] }, null, 2));
  process.exit(0);
}

// SpacetimeDB is addressed by module name on the host, not by the app's URL, and
// both live in the config the app generated. reset-db.sh reads the same two
// values the same way.
function spacetimeTarget(appDir) {
  const cfg = join(appDir, 'client', 'src', 'config.ts');
  if (!existsSync(cfg)) return null;
  const src = readFileSync(cfg, 'utf8');
  const pick = re => (src.match(re)?.[1] ?? null);
  const mod = pick(/MODULE_NAME\s*=\s*'([^']+)'/);
  let uri = pick(/URI\s*=\s*'([^']+)'/) ?? 'http://127.0.0.1:3210';
  // The SDK connects over ws://; the HTTP API does not answer there.
  uri = uri.replace(/^ws:/, 'http:').replace(/^wss:/, 'https:');
  return mod ? { mod, uri } : null;
}

async function probe(action) {
  try {
    if (args.backend === 'spacetime') {
      const t = spacetimeTarget(args.app ?? '.');
      if (!t) return { ok: false, status: 0, note: 'could not read MODULE_NAME from client/src/config.ts' };
      const r = await fetch(`${t.uri}/v1/database/${t.mod}/call/${action.reducer}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(action.args),
      });
      // 404 is the host saying there is no such reducer.
      return { ok: r.status !== 404, status: r.status, note: r.status === 404 ? `no reducer named "${action.reducer}"` : '' };
    }
    const base = (args.url ?? '').replace(/\/$/, '');
    if (!base) return { ok: false, status: 0, note: 'no --url given for a server-based backend' };
    const r = await fetch(`${base}${action.http.path}`, {
      method: action.http.method,
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({}),
    });
    return { ok: r.status !== 404, status: r.status, note: r.status === 404 ? `no route at ${action.http.path}` : '' };
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
if (args.out) writeFileSync(args.out, JSON.stringify({ backend: args.backend, results, missing: missing.map(m => m.id) }, null, 2));
process.exit(missing.length ? 1 : 0);
