#!/usr/bin/env node
// Report which named write actions answer. Exact recipe grading owns pass/fail.
//
// Presence, not behaviour: a 404 means the action is not there. Anything else —
// including a refusal — means it exists and answered, which is all this checks.
// Whether it refuses the RIGHT things is what the invariant suites are for.
//
// Nothing here is allowed to mutate the database: every probe is unauthenticated
// or deliberately malformed, so a working app rejects it.
//
// Usage:
//   node dist/commands/check-actions.js --backend spacetime --app <dir> [--out report.json]
//   node dist/commands/check-actions.js --backend postgres --url http://localhost:6573

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { emptyArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.js';
import { executeStackCapability } from '../src/stacks/stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';

import { STACK_BENCH_ROOT } from '../src/package-root.js';

const TRACKS = join(STACK_BENCH_ROOT, 'tracks');

interface CheckActionsArgs {
  backend?: string;
  url?: string;
  app?: string;
  out?: string;
  track?: string;
  quiet?: boolean;
  parentAttemptId?: string;
}

interface NamedAction {
  id: string;
  path: string;
  args?: unknown;
}

interface NamedActionRequest {
  url?: string;
  body?: BodyInit | null;
  missingNote: string;
}

interface ActionResult {
  id: string;
  ok: boolean;
  status: number;
  note: string;
}

function parseArgs(argv: string[]): CheckActionsArgs {
  const a: CheckActionsArgs = {};
  for (let i = 2; i < argv.length; i++) {
    const k = argv[i];
    if (k === '--backend') a.backend = argv[++i] ?? '';
    else if (k === '--url') a.url = argv[++i] ?? '';
    else if (k === '--app') a.app = argv[++i] ?? '';
    else if (k === '--out') a.out = argv[++i] ?? '';
    else if (k === '--track') a.track = argv[++i] ?? '';
    else if (k === '--quiet') a.quiet = true;
    else if (k === '--parent-attempt-id') a.parentAttemptId = argv[++i] ?? '';
    else { console.error(`Unknown arg ${k}`); process.exit(2); }
  }
  if (!a.backend) { console.error('--backend is required'); process.exit(2); }
  return a;
}

const args = parseArgs(process.argv);
const backend = args.backend;
if (!backend) throw new Error('--backend is required');

// The actions come from the track, not from this file: a track that names none
// (chat does not) is skipped rather than reported as six missing endpoints.
// Probes are chosen to be refused by a correct app — no credentials, or
// arguments that cannot identify a real row — so a check never writes anything.
const track = args.track ? JSON.parse(readFileSync(join(TRACKS, args.track, 'track.json'), 'utf8')) as {
  actions?: NamedAction[];
} : null;
const ACTIONS = (track?.actions ?? []).map(action => ({ ...action, http: { method: 'POST', path: action.path } }));
if (!ACTIONS.length) {
  if (!args.quiet) console.log(`  no named actions declared for track "${args.track ?? '(none)'}" — nothing to check`);
  if (args.out) {
    const id = `${args.parentAttemptId ?? 'actions'}-action-check`;
    writeArtifact(args.out, { kind: 'action_check', id,
      attempt: { id, parentId: args.parentAttemptId ?? null },
      identities: emptyArtifactIdentities({ stackAdapter: { id: backend } }),
      payload: { backend, results: [], missing: [] } });
  }
  process.exit(0);
}

// SpacetimeDB control targets come from the authenticated lease. Client config
// is app-controlled input and may use environment expressions rather than
// literals; it is neither authoritative nor safe for harness operations.
const adapter = STACK_ADAPTER_REGISTRY.get(backend);
const spacetime = executeStackCapability(adapter, 'grading', 'context',
  { requireBuildContainer: false });

async function probe(action: NamedAction): Promise<Omit<ActionResult, 'id'>> {
  try {
    const request = executeStackCapability(adapter, 'named-action', 'request',
      { action, input: { args: action.args }, spacetime, url: args.url }) as NamedActionRequest;
    if (!request.url) return { ok: false, status: 0, note: 'no --url given for a server-based backend' };
    const r = await fetch(request.url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: request.body,
    });
    return { ok: r.status !== 404, status: r.status,
      note: r.status === 404 ? request.missingNote : '' };
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    return { ok: false, status: 0, note: (message.split('\n')[0] ?? '').slice(0, 90) };
  }
}

const results: ActionResult[] = await Promise.all(ACTIONS.map(async action => ({
  id: action.id,
  ...(await probe(action)),
})));

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
      identities: emptyArtifactIdentities({ stackAdapter: { id: backend } }),
      payload: { backend, results, missing: missing.map(m => m.id) } });
}
process.exit(missing.length ? 1 : 0);
