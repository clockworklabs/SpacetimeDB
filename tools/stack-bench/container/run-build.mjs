#!/usr/bin/env node
// Run one build session inside the isolation image.
//
// This is the containerised replacement for agent.mjs spawning the CLI with
// `cwd: appDir`. The difference that matters is what is NOT here: no mount of
// tools/stack-bench, so the grader, the scenario files, the contracts and the
// prompts are not on the filesystem the build can reach. A fix round once read
// the scenario file defining the criteria it was failing and then ran
// grade.mjs; denying those paths is a blocklist against an agent that only
// needed grep and sed, so they are absent instead.
//
// The prompt arrives on stdin and the CLI's JSON result goes to stdout, exactly
// as the host path does, so callers do not care which one ran.
//
// Usage (argv mirrors what agent.mjs already computes):
//   node run-build.mjs --app <dir> --image <tag> --effort high \
//                      [--ports 6473,6573] [--model claude-sonnet-5]
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { homedir } from 'node:os';

const argv = process.argv.slice(2);
const opt = (k, d = null) => { const i = argv.indexOf(k); return i === -1 ? d : argv[i + 1]; };

const appDir = opt('--app');
if (!appDir) { console.error('run-build.mjs: --app is required'); process.exit(2); }

const REPO = resolve(join(new URL('.', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'), '..', '..', '..'));
const image = opt('--image', 'stack-bench-build:2.1.226');
const effort = opt('--effort', 'high');
const model = opt('--model', 'claude-sonnet-5');
const ports = (opt('--ports', '') || '').split(',').filter(Boolean);

// The two artifacts actually under test, mounted read-only from the repository.
// Copying them to a neutral path was considered and rejected: a staged copy can
// drift from the repo, and this project has already retracted a finding that
// came from a stale artifact.
const bindings = join(REPO, 'crates', 'bindings-typescript');
const cli = join(REPO, 'target', 'release', 'spacetimedb-cli.exe');

// Plan auth, not an API key. The CLI authenticates from this file and the host
// refreshes it, so it is mounted read-write rather than copied — a copy stops
// working the moment the token rotates. Verified to authenticate inside the
// image before this was wired up.
const creds = join(homedir(), '.claude', '.credentials.json');
if (!existsSync(creds)) {
  console.error(`run-build.mjs: no credentials at ${creds} — the container cannot authenticate`);
  process.exit(2);
}

const args = [
  'run', '--rm', '-i',
  '-v', `${resolve(appDir)}:/app`,
  '-v', `${creds}:/root/.claude/.credentials.json`,
];
if (existsSync(bindings)) args.push('-v', `${bindings}:/deps/bindings-typescript:ro`);
if (existsSync(cli)) args.push('-v', `${cli}:/deps/spacetimedb-cli:ro`);

// Dev servers start inside the container and the grader runs on the host, so
// the track's port window has to be published or nothing can be graded.
for (const p of ports) args.push('-p', `${p}:${p}`);

args.push(
  '-e', 'DISABLE_AUTOUPDATER=1',
  '-e', 'FORCE_PROMPT_CACHING_5M=1',
  '-w', '/app',
  image,
  'claude', '--print', '--output-format', 'json',
  '--permission-mode', 'acceptEdits',
  '--effort', effort,
  '--model', model,
);

// MSYS_NO_PATHCONV: Git Bash rewrites container-side paths like /app into
// Windows paths (C:/Program Files/Git/app) and every mount silently lands
// somewhere wrong.
const res = spawnSync('docker', args, {
  input: process.stdin.isTTY ? '' : readFileSync(0, 'utf8'),
  encoding: 'utf8',
  maxBuffer: 256 * 1024 * 1024,
  env: { ...process.env, MSYS_NO_PATHCONV: '1' },
});

if (res.stdout) process.stdout.write(res.stdout);
if (res.stderr) process.stderr.write(res.stderr);
process.exit(res.status ?? 1);
