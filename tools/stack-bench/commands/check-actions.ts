#!/usr/bin/env node
// Probes are unauthenticated or malformed and must never mutate data.

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
  method?: string;
  body?: BodyInit | null;
  missingNote: string;
  applicationRejectionStatuses?: number[];
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

// Use non-writing probes declared by the selected track.
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
      method: request.method ?? 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: request.body,
    });
    const rejectedByApplication = request.applicationRejectionStatuses?.includes(r.status) ?? false;
    const recognizedWithoutRunning = r.status >= 400 && r.status < 500
      && ![404, 405, 429].includes(r.status);
    const ok = r.ok || rejectedByApplication || recognizedWithoutRunning;
    return { ok, status: r.status,
      note: r.status === 404 ? request.missingNote
        : ok ? '' : `action probe returned HTTP ${r.status}` };
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
    console.log(`  ${r.ok ? 'ready' : 'UNUSABLE'}  ${r.id.padEnd(11)} ${r.status ? `HTTP ${r.status}` : ''} ${r.note}`);
  }
  console.log(missing.length
    ? `\n${missing.length} named action(s) unusable — contention and volume tests cannot be issued against this app.`
    : '\nall named actions are ready.');
}
if (args.out) {
  const id = `${args.parentAttemptId ?? 'actions'}-action-check`;
  writeArtifact(args.out, { kind: 'action_check', id,
    attempt: { id, parentId: args.parentAttemptId ?? null },
      identities: emptyArtifactIdentities({ stackAdapter: { id: backend } }),
      payload: { backend, results, missing: missing.map(m => m.id) } });
}
process.exit(missing.length ? 1 : 0);
