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
//                      [--ports 6473,6573] [--model claude-sonnet-5] \
//                      [--settings /app/.sandbox-settings.json] [--api-key <key>]
//
// Run it by hand from Git Bash and export MSYS_NO_PATHCONV=1 first, or the shell
// rewrites `/app/...` arguments into `C:/Program Files/Git/app/...` before this
// script ever sees them. agent.mjs spawns it without a shell and is unaffected.
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, mkdirSync } from 'node:fs';
import { join, resolve, basename, dirname } from 'node:path';
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
// The Linux build of the repository's own CLI — target/release holds a Windows
// PE binary a container cannot execute. Produced by build-linux-cli.sh, which
// compiles this checkout rather than fetching a release, so the CLI under test
// is still the one under test.
const cli = join(REPO, 'tools', 'stack-bench', 'container', 'bin', 'spacetimedb-cli');

// Auth. An API key is used when one is supplied, and otherwise the CLI
// authenticates from the mounted credential so runs bill to the plan rather
// than to a key. The credential is mounted read-write rather than copied
// because the host rotates that token and a copy stops working when it does.
//
// The key is preferred when present because it keeps a rotating credential
// off the build's filesystem entirely — the build can read anything mounted
// into it, and a token is worth more than a run.
const apiKey = opt('--api-key', process.env.ANTHROPIC_API_KEY ?? '');
const creds = join(homedir(), '.claude', '.credentials.json');
if (!apiKey && !existsSync(creds)) {
  console.error(`run-build.mjs: no --api-key/ANTHROPIC_API_KEY and no credentials at ${creds}`);
  console.error('  the container has no way to authenticate');
  process.exit(2);
}

// The audit trail has to survive `--rm`.
//
// leak-audit.mjs decides whether a run is contaminated, and cost-ledger.mjs
// reconstructs the bill; both read the session transcript. Inside the container
// the CLI files it under /root/.claude/projects/-app (cwd is /app, and the CLI
// names a project folder after its path with separators turned into dashes —
// checked, not assumed). Mounting the host folder that `leak-audit --app` looks
// in onto that exact path means the audit keeps working with no argument
// changes, and a containerised run stays as auditable as a host one.
//
// The host's whole ~/.claude/projects is deliberately NOT mounted: it holds
// every other run's transcripts and the user's own sessions.
const projects = join(homedir(), '.claude', 'projects',
  resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase());
mkdirSync(projects, { recursive: true });

// The SpacetimeDB CLI's own config, which holds the identity and token it mints
// on first publish. On Linux that is $XDG_CONFIG_HOME/spacetime (checked in
// crates/paths), and in a `--rm` container it would be discarded every round —
// so a fix round would arrive as a different identity and be refused ownership
// of the module the build round published. On the host this config persists
// globally, so persisting it per run is the behaviour being reproduced, not a
// new one.
//
// It lives beside the app rather than inside it: the app directory is what gets
// copied into source/ and audited, and a token does not belong in either.
const stdbConfig = resolve(appDir, '..', '.spacetime-cli-config');
mkdirSync(stdbConfig, { recursive: true });

// The container OUTLIVES the build session, and that is the whole point.
//
// The first version used `docker run --rm`, so the container died the moment the
// coding session returned — taking the app's dev servers with it. The grader
// runs afterwards and had nothing to talk to: "reseed FAILED (server did not
// come back)", then "ABORTED: could not reset database", after a sweep spent
// $9.46 and graded nothing.
//
// Running the app from the host instead is not an option either: a container
// install produces linux-x64 esbuild and rollup binaries, so a Windows host
// cannot execute the app's node_modules at all (checked, not assumed).
//
// So the container is long-lived and the session is exec'd into it. The build
// starts its own dev servers exactly as it does on the host, and they keep
// running afterwards for exactly the same reason — the process that owns them
// is still alive.
//
// The name is derived from the run's work directory, which already carries a
// timestamp, so it is unique per run and reconstructible from the app path alone
// — restart-backend.sh needs the same name and is given only the app dir.
const containerName = `stack-bench-${basename(dirname(resolve(appDir)))}`;
const dockerEnv = { ...process.env, MSYS_NO_PATHCONV: '1' };

function containerState(name) {
  const r = spawnSync('docker', ['inspect', '-f', '{{.State.Running}}', name],
    { encoding: 'utf8', env: dockerEnv });
  if (r.status !== 0) return 'absent';
  return r.stdout.trim() === 'true' ? 'running' : 'stopped';
}

// Create it if this is the first round of the run; reuse it for every round
// after, so a fix round finds the app, its node_modules and its servers exactly
// where the build round left them.
if (containerState(containerName) !== 'running') {
  // A stopped container of the same name would refuse the create and cannot be
  // reused anyway — its published ports are gone.
  spawnSync('docker', ['rm', '-f', containerName], { stdio: 'ignore', env: dockerEnv });

  const create = [
    'run', '-d', '--init', '--name', containerName,
    '-v', `${resolve(appDir)}:/app`,
    '-v', `${projects}:/root/.claude/projects/-app`,
    '-v', `${stdbConfig}:/root/.config/spacetime`,
  ];
  if (!apiKey) create.push('-v', `${creds}:/root/.claude/.credentials.json`);
  if (existsSync(bindings)) create.push('-v', `${bindings}:/deps/bindings-typescript:ro`);
  if (existsSync(cli)) create.push('-v', `${cli}:/deps/spacetimedb-cli:ro`);

  // Dev servers start inside the container and the grader runs on the host, so
  // the track's port window has to be published. Publishing happens at create
  // time only — a port cannot be added to a running container, which is the
  // other reason the session cannot own the container's lifetime.
  for (const p of ports) create.push('-p', `${p}:${p}`);

  // `--init` gives the container a real PID 1. Without it the dev servers the
  // build leaves behind are reparented to `sleep`, which never reaps them.
  create.push('-w', '/app', image, 'sleep', 'infinity');

  const made = spawnSync('docker', create, { encoding: 'utf8', env: dockerEnv });
  if (made.status !== 0) {
    console.error(`run-build.mjs: could not start ${containerName}`);
    console.error(made.stderr || made.stdout || '');
    process.exit(2);
  }
}

const args = ['exec', '-i', '-w', '/app'];

args.push('-e', 'DISABLE_AUTOUPDATER=1', '-e', 'FORCE_PROMPT_CACHING_5M=1');
if (apiKey) args.push('-e', `ANTHROPIC_API_KEY=${apiKey}`);
// A container does not inherit the caller's environment, so anything the run is
// meant to be configured by has to be handed over explicitly. Only variables the
// harness sets deliberately are forwarded — passing the whole environment would
// put the host's shape, and CLAUDE_EFFORT, back inside the build.
if (process.env.MAX_THINKING_TOKENS) {
  args.push('-e', `MAX_THINKING_TOKENS=${process.env.MAX_THINKING_TOKENS}`);
}

args.push(
  containerName,
  'claude', '--print', '--output-format', 'json',
  '--permission-mode', 'acceptEdits',
  '--effort', effort,
  '--model', model,
  // The app is the only directory a session may reach; inside the container
  // that is all there is, but the flag is kept so host and container runs are
  // configured identically.
  '--add-dir', '/app',
);
// The sandbox settings file is written into the app directory by the caller,
// so it arrives through the /app mount at a known container path.
if (opt('--settings')) args.push('--settings', opt('--settings'));

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
