#!/usr/bin/env node
// Drives one headless coding session: build a level, upgrade to the next, or fix
// reported bugs. Self-contained — no dependency on the sequential-upgrade tool.
//
// Cost and token usage come from the CLI's own JSON result, so there is no
// telemetry collector to run.
//
// Usage:
//   node agent.mjs --mode build   --backend spacetime --level 1 --app <dir>
//   node agent.mjs --mode upgrade --backend spacetime --level 2 --app <dir>
//   node agent.mjs --mode fix     --backend spacetime --app <dir>
//
// Prints a JSON line: { appDir, costUsd, tokens, durationMs, sessionId, ok }

import { execFileSync, spawn } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, rmSync,
         openSync, readSync, closeSync } from 'node:fs';
import { join, dirname, resolve, basename } from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { loadTrack, levelPrompt, appendix, suitesFor, dbName, moduleName, portsFor, DEFAULT_TRACK } from './tracks.mjs';
import { resolveLegacyRecipeRelease } from './recipe-release.mjs';
import { writeSandbox } from './sandbox.mjs';
import { killTree } from './platform.mjs';
import { leaseFromEnv } from './backend-lease.mjs';
import { resolveContainerImage } from './container-image.mjs';
import { hashDirectory, sessionProvenance, sha256 } from './provenance.mjs';
import { executeStackCapability } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';
import { DEFAULT_BUILD_IMAGE } from './product-config.mjs';
import { dockerMountArguments } from './container-mount.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(ROOT, '..', '..');
const CONTROL_COMMAND_TIMEOUT_MS = 120_000;
const BUILD_SESSION_TIMEOUT_MS = 58 * 60_000;

// The benchmark runs its own SpacetimeDB host so that measurements describe the
// module under test rather than whatever else is published on a shared machine,
// and so restarting it for durability tests cannot take somebody else down.
const STDB_URI = process.env.STACK_BENCH_STDB_URI ?? 'http://127.0.0.1:3210';

// Build the app against the SpacetimeDB in THIS repository, not a published
// release: otherwise a change to the host, the CLI or the module SDK is not
// under test, and a result describes software nobody is working on.
const LOCAL_CLI = join(REPO, 'target', 'release', 'spacetimedb-cli.exe');
const STDB_BIN = process.env.SPACETIME_BIN ?? (existsSync(LOCAL_CLI) ? LOCAL_CLI : 'spacetime');
const LOCAL_PKG = process.env.STDB_PACKAGE ?? join(REPO, 'crates', 'bindings-typescript');

// Both shells and npm read a Windows backslash as an escape, so paths handed to
// the model are written the one way every tool agrees on.
const fwd = p => p.split('\\').join('/');

// The thinking budget is deliberately NOT set. Customers do not set
// MAX_THINKING_TOKENS, so pinning it measures a configuration nobody runs --
// and it is the single largest cost component here (43% more reasoning volume
// on SpacetimeDB than postgres), which makes an arbitrary value the worst
// possible thing to guess at. A pin of 10000 was tried and changed nothing
// measurable: signature volume ranged 2.4-6.0KB per block both before and
// after, driven by the task rather than the setting.
//
// Reproducibility is preserved by MEASURING instead: run.json records the
// thinking blocks and bytes each run actually produced, so a CLI default that
// moves shows up in the data rather than silently shifting every score.
// STACK_BENCH_THINKING still forces a value for a deliberate experiment.
const THINKING_TOKENS = process.env.STACK_BENCH_THINKING ?? null;

// Reasoning effort, pinned and recorded rather than inherited.
//
// The CLI takes --effort low|medium|high|xhigh|max. The harness never passed it,
// so every run so far took whatever the environment happened to carry — and
// this machine has CLAUDE_EFFORT=high, which agent.mjs forwards with the rest
// of process.env. The comparisons stayed fair because both stacks got the same
// level, but nothing recorded which level produced a number, and anyone else
// running this would silently get different ones.
//
// Pinned to high to match every result collected so far. STACK_BENCH_EFFORT
// changes it deliberately.
const EFFORT = process.env.STACK_BENCH_EFFORT ?? 'high';

// Coding sessions always run in Docker; there is no host execution path.
//
// The container gives a property the sandbox never had — the harness is not on
// the filesystem the build can see — and the post-build half now works: an app
// was stood up in one and graded end to end, restart/stop/start really restart,
// and `spacetime dev` now publishes and streams logs with one retained identity.
//
// Historical ledger: containerisation cost a day, a
// $9.46 sweep that graded nothing, and an hour-long hang. Those failures have
// since been resolved and remain here as the reason isolation is a hard gate.
// The contamination it
// prevents has happened once, was CAUGHT by leak-audit, and cost one voided run.
// Reproduce the complete boundary with `npm run test:container`; no model spend
// is needed.
const IMAGE = process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE;

// Set once in main(), because the addresses a build is TOLD to use depend on
// where the build runs.
//
// Host services — the databases, the SpacetimeDB host, the lint server — stay on
// the machine, and `localhost` inside a container is the container. Those
// addresses are rewritten to Docker's host alias. Dev-server ports are NOT
// rewritten: those servers start inside the container and are published back
// out, so the grader on the host still reaches them at localhost.
const HOST_ADDR = process.env.STACK_BENCH_HOST_ALIAS ?? 'host.docker.internal';
const hostUrl = u => u.replace(/127\.0\.0\.1|localhost/g, HOST_ADDR);

// Where the two artifacts under test are mounted. Container paths also keep the
// repository root out of the prompt, which is what the contaminated run followed
// to reach the grader.
const C_PKG = '/deps/bindings-typescript';
const C_BIN = '/deps/spacetimedb-cli';

// The Linux build of the repository's CLI, which is what a container mounts at
// C_BIN. Built by container/build-linux-cli.sh; target/release holds the
// Windows binary and the two must not be confused for each other.
const LINUX_CLI = process.env.STACK_BENCH_LINUX_CLI
  ?? join(ROOT, 'container', 'bin', 'spacetimedb-cli');

// What the session ACTUALLY thought, read back from its own transcript.
//
// This is the measurement that replaces the pin. Reasoning volume is the
// largest single component of the cost gap between backends, so leaving the
// budget at the CLI default is only defensible if a shift in that default is
// visible afterwards rather than silently absorbed into every score.
//
// The text of a thinking block is redacted at the wire level -- `thinking` is
// an empty string and the content survives only as an opaque signature -- so
// this counts blocks and signature bytes. Neither is a token count, but both
// move with reasoning volume, and they are what the transcript actually has.
function thinkingVolume(appDir, sessionId) {
  if (!sessionId) return null;
  try {
    const store = join(homedir(), '.claude', 'projects');
    if (!existsSync(store)) return null;
    const want = resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase();
    const dir = readdirSync(store).find(d => {
      const n = d.toLowerCase();
      return n === want || n === want.replace(/^-+/, '');
    });
    const file = dir && join(store, dir, `${sessionId}.jsonl`);
    if (!file || !existsSync(file)) return null;

    let blocks = 0, bytes = 0;
    for (const line of readFileSync(file, 'utf8').split('\n')) {
      if (!line.includes('"thinking"')) continue;   // cheap filter before parsing
      let rec;
      try { rec = JSON.parse(line); } catch { continue; }
      for (const c of rec?.message?.content ?? []) {
        if (c.type !== 'thinking') continue;
        blocks++;
        bytes += (c.signature ?? '').length;
      }
    }
    return { blocks, signatureBytes: bytes,
             bytesPerBlock: blocks ? Math.round(bytes / blocks) : 0 };
  } catch { return null; }
}


// The version of the CLI a container actually ran, which is a Linux binary the
// host cannot execute. Reporting the Windows build's version instead would
// attribute the run to whatever happens to be sitting in target/release — and
// the two go stale independently, which is exactly how a stale CLI produced a
// retracted finding here before.
function linuxSpacetimeVersion(image) {
  try {
    const releaseVolume = process.env.STACK_BENCH_RELEASE_DEPS_VOLUME?.trim() || null;
    const mountArgs = releaseVolume
      ? dockerMountArguments({ kind: 'volume', source: releaseVolume,
        target: '/release-deps', readOnly: true })
      : ['-v', `${LINUX_CLI}:/deps/spacetimedb-cli:ro`];
    const entrypoint = releaseVolume ? '/release-deps/spacetimedb-cli' : '/deps/spacetimedb-cli';
    const out = execFileSync('docker',
      ['run', '--rm', ...mountArgs, '--entrypoint', entrypoint, image, '--version'],
      { encoding: 'utf8', stdio: 'pipe', env: { ...process.env, MSYS_NO_PATHCONV: '1' },
        timeout: CONTROL_COMMAND_TIMEOUT_MS });
    const commit = out.match(/Commit:\s*([0-9a-f]+)/i)?.[1] ?? null;
    return { commit, binarySha256: sha256(readFileSync(LINUX_CLI)),
      raw: out.trim().split(/\r?\n/).slice(0, 2).join(' ') };
  } catch { return { commit: null, binarySha256: null, raw: 'unknown' }; }
}

function bindingsIdentity(pkgDir) {
  try {
    const p = JSON.parse(readFileSync(join(pkgDir, 'package.json'), 'utf8'));
    const source = hashDirectory(pkgDir, { exclude: name =>
      /(^|\/)(node_modules|dist|target)(\/|$)/.test(name) });
    return { package: `${p.name}@${p.version}`, sourceSha256: source.sha256,
      sourceFiles: source.files.length };
  } catch { return { package: 'unknown', sourceSha256: null, sourceFiles: 0 }; }
}

// The CLI version inside the build image. Read by running it, not by trusting
// the tag: the image is pinned by ARG and a tag can be moved.
function imageCliVersion(image) {
  try {
    return execFileSync('docker', ['run', '--rm', '--entrypoint', 'claude', image, '--version'],
      { encoding: 'utf8', stdio: 'pipe', env: { ...process.env, MSYS_NO_PATHCONV: '1' },
        timeout: CONTROL_COMMAND_TIMEOUT_MS }).trim();
  } catch { return 'unknown'; }
}

function imageNodeVersion(image) {
  try {
    return execFileSync('docker', ['run', '--rm', '--entrypoint', 'node', image, '--version'],
      { encoding: 'utf8', stdio: 'pipe', env: { ...process.env, MSYS_NO_PATHCONV: '1' },
        timeout: CONTROL_COMMAND_TIMEOUT_MS }).trim();
  } catch { return 'unknown'; }
}

function containerImage(name) {
  try {
    const out = execFileSync('docker', ['inspect', '-f', '{{.Config.Image}} {{.Image}}', name],
      { encoding: 'utf8', stdio: 'pipe', timeout: CONTROL_COMMAND_TIMEOUT_MS }).trim();
    const [reference, imageId] = out.split(/\s+/, 2);
    return { reference, imageId };
  } catch { return { reference: 'unknown', imageId: null }; }
}

// Anything ambient that could change how the model behaves. We cannot prove no
// environment variable influenced a run, but we can record which ones were
// present so the question is answerable later. CLAUDE_EFFORT was set on this
// machine for every run in the project before anyone noticed.
function ambientEnv() {
  const seen = {};
  for (const [k, v] of Object.entries(process.env)) {
    if (!/^(CLAUDE|ANTHROPIC|MAX_THINKING|DISABLE_AUTOUPDATER|FORCE_PROMPT)/.test(k)) continue;
    // Never record a credential, only that one was present.
    seen[k] = /KEY|TOKEN|SECRET/i.test(k) ? '<redacted, present>' : v;
  }
  return seen;
}

function parseArgs(argv) {
  const a = { level: 1, runIndex: 0, model: 'claude-sonnet-5', guidance: 'prescribed',
    track: DEFAULT_TRACK };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--mode': a.mode = argv[++i]; break;
      case '--track': a.track = argv[++i]; break;
      case '--backend': a.backend = argv[++i]; break;
      case '--level': a.level = parseInt(argv[++i], 10); break;
      case '--app': a.app = argv[++i]; break;
      case '--run-index': a.runIndex = parseInt(argv[++i], 10); break;
      case '--model': a.model = argv[++i]; break;
      // --stack is the accurate name: the only thing this varies is whether the
      // stack is prescribed, not how much help the model gets. Everything else
      // (harness ports, branding, the level spec, the contract appendix) is
      // identical in both. --guidance is kept as an alias so older invocations
      // and recorded runs still resolve.
      case '--stack': a.guidance = argv[++i] === 'free' ? 'minimal' : 'prescribed'; break;
      case '--guidance': a.guidance = argv[++i]; break;
      case '--thinking': a.thinking = argv[++i]; break;
      // Comma-separated skill directories to inline, e.g.
      //   --skills typescript-server,typescript-client,cli
      case '--skills': a.skills = argv[++i].split(',').map(x => x.trim()).filter(Boolean); break;
      // Container or nothing. For sweeps whose claim is that no build could
      // reach the grader — there, a silent fallback would be a false claim.
      // An API key, when supplied, is used instead of the mounted plan
      // credential — it keeps a rotating token off the build's filesystem.
      case '--api-key': a.apiKey = argv[++i]; break;
      case '--print-prompt': a.printPrompt = true; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!a.mode || !a.backend || !a.app) {
    console.error('Usage: node agent.mjs --mode build|upgrade|fix --backend <b> --app <dir> [--level N]');
    process.exit(2);
  }
  return a;
}

const dbUrl = (backend, runIndex, dbPort, track) => executeStackCapability(
  STACK_ADAPTER_REGISTRY.get(backend), 'agent', 'connection-url',
  { dbPort, database: dbName(track, runIndex), hostUrl });

// Per-run databases must exist before the app connects, or the agent will go
// looking for one that does — which has led to apps silently using a foreign
// instance.
//
// A `build` also WIPES it, schema included. The between-suite reset truncates,
// because the app is running and its tables must survive; that left one run's
// schema in place for the next, and a build opened on another app's tables —
// "different column names like `qty` instead of `quantity`, missing columns,
// extra tables". It spent real turns diagnosing and clearing someone else's
// leftovers, and those turns land in the cost figure this benchmark exists to
// measure. Mongo already dropped its whole database; postgres only truncated.
export function ensureDatabase(backend, runIndex, dbPort, track, wipe = false,
  { exec = execFileSync, stdbBin = STDB_BIN } = {}) {
  const { lease } = leaseFromEnv(process.env, { backend, active: true });
  if (lease.runIndex !== runIndex || lease.track !== track.name) {
    throw new Error(`backend lease ${lease.runId} belongs to ${lease.track}/run${lease.runIndex}, `
      + `not ${track.name}/run${runIndex}`);
  }
  const expectedName = dbName(track, runIndex);
  const name = lease.resources.database ?? expectedName;
  return executeStackCapability(STACK_ADAPTER_REGISTRY.get(backend), 'database', 'prepare', {
    lease, name, expectedName, wipe, exec, cli: stdbBin,
    expectedServerUri: STDB_URI, expectedModule: moduleName(track, runIndex), dbPort,
  });
}

// prescribed: the stack is chosen for them (Express, socket.io, an ORM, a layout).
// minimal: only the database, the ports the harness needs, and branding — how to
// build it is the model's call. Prescribing a stack means measuring the stack we
// picked, not the database.
function backendDoc(args, p, track) {
  if (args.guidance === 'minimal' && !executeStackCapability(
    STACK_ADAPTER_REGISTRY.get(args.backend), 'agent', 'minimal-guidance-supported')) {
    console.error(`--stack free does not apply to ${args.backend}; its adapter requires the prescribed stack`);
    process.exit(2);
  }
  const rel = args.guidance === 'minimal'
    ? join('backends', 'minimal', `${args.backend}.md`)
    : join('backends', `${args.backend}.md`);
  const raw = readFileSync(join(ROOT, rel), 'utf8');
  return raw
    .replaceAll('<VITE_PORT>', String(p.vite))
    .replaceAll('<EXPRESS_PORT>', String(p.express ?? ''))
    .replaceAll('<APP_NOUN>', track.title)
    .replaceAll('<MODULE_NAME>', moduleName(track, args.runIndex))
    .replaceAll('<DATABASE_URL>', p.dbPort ? dbUrl(args.backend, args.runIndex, p.dbPort, track) ?? '' : '')
    .replaceAll('<STDB_URI>', hostUrl(STDB_URI))
    .replaceAll('<STDB_BIN>', C_BIN)
    .replaceAll('<STDB_PACKAGE>', `file:${C_PKG}`);
}

// SpacetimeDB is young enough that models have little of it in training data;
// the skill documents are its API reference, equivalent to what the other stacks
// get from having been on the internet for a decade.
// Which documents are inlined is the variable under test in the cost work, so
// it is a flag rather than an edit: both arms of a comparison then come from
// one binary, and run.json records which arm produced each number.
//
// The default omits `cli`, which is the state every result so far was measured
// in -- so the default keeps history comparable. `cli` is what teaches
// `spacetime dev` (auto-rebuild, auto-publish, regenerate bindings); without
// it a build hand-rolls that loop, and one run that had the knowledge came in
// at $10.29/32min against $22.51/127min without. That was n=1 and is exactly
// what --skills exists to settle.
function skillDocs(skills) {
  const strip = md => md.replace(/^---\n[\s\S]*?\n---\n/, '');
  return skills
    .map(s => strip(readFileSync(join(REPO, 'skills', s, 'SKILL.md'), 'utf8')))
    .join('\n\n---\n\n');
}

// Naming the linter's path anywhere the build can read it is a signpost to the
// harness: two directory listings from there sit grade.mjs and the scenario
// files that define the score. Moving it out of the prompt and into a shim
// narrowed that and did not close it. A sandboxed postgres build then proved
// it, scoring 49/49 while the audit caught the whole walk -- `cat`
// check-hooks.sh, read the path, `cat` lint.mjs, follow its
// `import '../tracks.mjs'` and on to the contract and the walk. Every one of
// those was Bash. The file-tool rules refused nothing because nothing was asked
// of them.
//
// So the path stops existing on disk. The harness answers lint requests over
// loopback for the life of the session, and the shim names a port. A port
// cannot be followed to grade.mjs.
// It listens in a SEPARATE process. Listening inside this one did not work and
// was not obviously broken: this process spends the whole session blocked in
// execFileSync, a blocked event loop accepts no connections, and the build's
// `curl` hung until its own 120-second tool timeout. One run finished with 14
// missing hooks after three fix rounds having never once seen a lint result.
function startLintServer(cmd, appDir) {
  const portFile = join(appDir, '.lint-port');
  rmSync(portFile, { force: true });
  const child = spawn(process.execPath,
    [join(ROOT, 'lint-server.mjs'), '--port-file', portFile, '--cmd', cmd],
    { detached: true, stdio: 'ignore' });
  child.unref();

  // The port file appears only once the socket is accepting, so waiting for it
  // is waiting for readiness — not a guess at how long a spawn takes.
  for (let i = 0; i < 100; i++) {
    if (existsSync(portFile)) {
      const port = readFileSync(portFile, 'utf8').trim();
      if (port) return { port, pid: child.pid };
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100);
  }
  killTree(child.pid);
  throw new Error('the lint server did not come up; a build without its hook check is not worth running');
}

// Whether a containerised run can actually work, checked before anything is
// spent rather than discovered an hour in as a build error the model gets
// blamed for. Returns a reason instead of throwing, because the default and the
// explicit request are answered differently — see resolveIsolation.
//
// SpacetimeDB needs a CLI the container can execute. `target/release/
// spacetimedb-cli.exe` is a Windows PE binary, so container/build-linux-cli.sh
// compiles a Linux one from this same checkout. The benchmark deliberately
// tests the CLI in THIS repository rather than a published release, so falling
// back to a `spacetime` from the image would measure different software and
// report it under the same name.
function containerBlocker(backend) {
  try {
    execFileSync('docker', ['image', 'inspect', IMAGE],
      { stdio: 'pipe', timeout: CONTROL_COMMAND_TIMEOUT_MS });
  } catch (error) {
    const detail = String(error.message).split('\n')[0];
    return `cannot verify isolation image ${IMAGE}: ${detail} — `
      + `build it with docker build -t ${IMAGE} ${fwd(join(ROOT, 'container'))}`;
  }
  if (!executeStackCapability(STACK_ADAPTER_REGISTRY.get(backend),
    'agent', 'linux-cli-required')) return null;
  if (!existsSync(LINUX_CLI)) {
    return `no Linux SpacetimeDB CLI at ${fwd(LINUX_CLI)} — `
      + 'bash tools/stack-bench/container/build-linux-cli.sh';
  }
  // ELF magic, checked rather than assumed: the path existing says nothing
  // about which platform the file was built for, and a Windows binary copied
  // there would fail deep inside a build as an unexplained publish error.
  const magic = Buffer.alloc(4);
  try {
    const fd = openSync(LINUX_CLI, 'r');
    try { readSync(fd, magic, 0, 4, 0); } finally { closeSync(fd); }
  } catch {
    return `cannot read the Linux SpacetimeDB CLI at ${fwd(LINUX_CLI)}`;
  }
  if (magic.toString('binary') !== '\x7fELF') {
    return `${fwd(LINUX_CLI)} is not a Linux binary; rebuild it with `
      + 'container/build-linux-cli.sh';
  }
  return null;
}

// There is one coding-session topology to secure and clean up: Docker or fail.
function decideIsolation(args) {
  const blocker = containerBlocker(args.backend);
  if (!blocker) return { container: true, reason: null };
  console.error(`agent.mjs: isolated build unavailable: ${blocker}`);
  console.error('  benchmark coding sessions require the isolation container');
  process.exit(2);
}

// Every round must retain the container pin written by the first round. A
// missing or non-container marker is ambiguous prior state, never authority to
// choose a second execution topology.
//
// So the decision is made once, at `build`, and recorded beside the app rather
// than inside it (the app directory is what gets copied into source/ and
// audited). Later modes read it back and refuse ambiguous prior state.
//
// The marker lives outside the app because `build` wipes the app directory.
// An app with benchmark state and no marker predates this invariant; refuse it
// rather than guessing where its earlier rounds executed.
function resolveIsolation(args) {
  const marker = resolve(args.app, '..', '.stack-bench-isolation');

  if (args.mode === 'build') {
    const decided = decideIsolation(args);
    if (!args.printPrompt) {
      mkdirSync(dirname(marker), { recursive: true });
      writeFileSync(marker, 'container');
    }
    return decided;
  }

  if (existsSync(marker)) {
    const pinned = readFileSync(marker, 'utf8').trim();
    if (pinned !== 'container') {
      console.error(`agent.mjs: unsupported isolation marker ${JSON.stringify(pinned)}; expected "container"`);
      process.exit(2);
    }
    const blocker = containerBlocker(args.backend);
    if (blocker) {
      console.error(`agent.mjs: this run's build ran in a container, but ${blocker}`);
      console.error('  refusing to run this round in a different environment');
      process.exit(2);
    }
    return { container: true, reason: null };
  }

  // No marker. `.stack-bench-backend` is written by every agent round, so its
  // presence means a previous round already worked in this directory without
  // recording its topology. (A seeded upgrade does not
  // have it: restoreSource copies source directories, not dotfiles, so the
  // first round of a new run still decides fresh.)
  if (existsSync(join(args.app, '.stack-bench-backend'))) {
    console.error('agent.mjs: app has prior benchmark state but no isolation marker');
    console.error('  refusing to guess where earlier rounds ran; start a clean run');
    process.exit(2);
  }
  const decided = decideIsolation(args);
  if (!args.printPrompt) {
    mkdirSync(dirname(marker), { recursive: true });
    writeFileSync(marker, 'container');
  }
  return decided;
}

function writeLintShim(appDir, port) {
  const sh = join(appDir, 'check-hooks.sh');
  writeFileSync(sh, '#!/usr/bin/env bash\n'
    + '# Verifies the required data-testid hooks resolve in the running app.\n'
    + `curl -sS --fail-with-body http://${HOST_ADDR}:${port}/lint\n`);
  return './check-hooks.sh';
}

function buildPrompt(args, p, track, lintPort, materials = {}) {
  const lint = args.printPrompt ? './check-hooks.sh' : writeLintShim(args.app, lintPort);
  const common = [
    'The app lives in /app — work there.',
    // Container runs only, and identical for every backend so no stack gets
    // more guidance than another.
    //
    // Vite binds `localhost` by default. Inside a container that is the
    // container's own loopback, which a published port does not forward to —
    // measured: a server on 127.0.0.1 in a container is unreachable from the
    // host through -p, the same server on 0.0.0.0 answers 200. Without this
    // every containerised run would build a working app the grader cannot
    // reach, and fail for a reason that has nothing to do with the database.
    '',
      'Dev servers must listen on 0.0.0.0, not localhost, or nothing outside '
      + 'can reach them. For Vite set `server.host: true` in vite.config.ts.',
    '',
    backendDoc(args, p, track),
  ];
  const skills = materials.skillsText ?? skillDocs(args.backend, args.skills ?? DEFAULT_SKILLS);
  if (skills) common.push('', '---', '', skills);

  if (args.mode === 'fix') {
    return [
      'Fix the bugs reported by automated verification.',
      '',
      'Read BUG_REPORT.md in the app directory. Each entry says what was expected',
      'and what actually happened. Fix the app so the behaviour matches, redeploy,',
      'and make sure the dev server is running.',
      '',
      'Change only what is needed. Do not alter behaviour that is already correct.',
      '',
      `When the fixes are deployed, verify the testing hooks still resolve:\n\n    ${lint}\n`,
      'Output FIX_COMPLETE when done.',
      '',
      ...common,
    ].join('\n');
  }

  const verb = args.mode === 'upgrade'
    ? [
        `Add the level ${args.level} features below to the existing app.`,
        '',
        'Everything from earlier levels already works — do not rewrite it. Add only',
        'what the new level describes, and keep the earlier behaviour intact.',
      ]
    : [`Build the application described below, then deploy it and leave it running.`];

  return [
    ...verb,
    '',
    `When it is deployed, verify the testing hooks resolve:\n\n    ${lint}\n`,
    'Fix any failures and re-run until it prints CONTRACT LINT PASS.',
    `Output ${args.mode === 'upgrade' ? 'UPGRADE_COMPLETE' : 'DEPLOY_COMPLETE'} when the`,
    'dev server is confirmed running and the lint passes.',
    '',
    ...common,
    '',
    '---',
    '',
    materials.requirementText ?? levelPrompt(track, args.level),
    materials.contractText ?? appendix(track, args.level),
  ].join('\n');
}

// A build must not be able to read the thing that grades it.
// The deny list itself lives in sandbox.mjs, shared with probe-sandbox.mjs so
// the probe proves the rules a build actually gets.

async function main() {
  const args = parseArgs(process.argv);
  const track = loadTrack(args.track);
  const p = portsFor(track, args.backend, args.runIndex);
  // Everything the prompt says about addresses and paths depends on where the
  // build will run, so this is settled before any prompt is rendered — printed
  // or sent. Resolving it after --print-prompt would make the regression gate
  // compare a prompt nobody is ever given.
  resolveIsolation(args);
  const imageIdentity = resolveContainerImage(IMAGE);
  const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const defaultSkills = executeStackCapability(adapter, 'agent', 'default-skills');
  const selectedSkills = defaultSkills.length ? (args.skills ?? defaultSkills) : [];
  const skillsText = skillDocs(selectedSkills);
  const recipeBinding = resolveLegacyRecipeRelease(track, args.level);
  const requirementText = recipeBinding?.plan.recipe.task.requirementText ?? levelPrompt(track, args.level);
  const contractText = recipeBinding?.plan.recipe.task.contractText ?? appendix(track, args.level);

  // Renders what the model would be given, without spending anything or
  // touching the app directory. The regression gate for harness changes is a
  // diff of this against a saved copy — captured with the same isolation flags,
  // since host and container prompts differ by design.
  if (args.printPrompt) {
    process.stdout.write(buildPrompt(args, p, track, undefined, { skillsText, requirementText, contractText }));
    return;
  }
  // Only a build wipes: an upgrade or a fix must find the data the previous
  // level left, which is the whole point of a cumulative ladder.
  // Called for every backend, not just those with a dbPort: spacetime has no
  // database port and would have skipped the pre-build wipe entirely, which is
  // exactly the backend whose leftover module causes the migration abort.
  ensureDatabase(args.backend, args.runIndex, p.dbPort, track, args.mode === 'build');
  // `build` means from scratch. Leaving a previous app in place lets the agent
  // inherit code — and a stale BUG_REPORT.md — from a run that has nothing to do
  // with this one.
  if (args.mode === 'build') {
    // A dev server from a previous run can still hold the directory open, so
    // give the filesystem a moment rather than failing the whole build.
    for (let attempt = 0; ; attempt++) {
      try { rmSync(args.app, { recursive: true, force: true }); break; }
      catch (err) {
        if (attempt >= 5) throw err;
        // A synchronous sleep with no child process: `timeout` refuses to run
        // when stdin is not a console, which is exactly how this is invoked.
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 3000);
      }
    }
  }
  mkdirSync(args.app, { recursive: true });
  writeFileSync(join(args.app, '.stack-bench-backend'), args.backend);

  // The linter runs here, in the harness, and answers over loopback. The build
  // is given a port instead of a path, so there is nothing in its directory to
  // read the harness location out of.
  const lintCmd = `node "${join(ROOT, 'linter', 'lint.mjs')}" --url http://localhost:${p.vite} --level ${args.level}`
    + (track.name === DEFAULT_TRACK ? '' : ` --track ${track.name}`);
  const lintServer = startLintServer(lintCmd, args.app);
  const lintPort = lintServer.port;
  // A detached child outlives a parent that throws. Without this, every failed
  // build would leave a listener behind — the exact thing this harness has been
  // fixing all day.
  const stopLint = () => killTree(lintServer.pid);
  process.on('exit', stopLint);
  for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => { stopLint(); process.exit(130); });

  const prompt = buildPrompt(args, p, track, lintPort, { skillsText, requirementText, contractText });
  const bugReportPath = join(args.app, 'BUG_REPORT.md');
  const bugReportText = args.mode === 'fix' && existsSync(bugReportPath)
    ? readFileSync(bugReportPath, 'utf8') : null;
  const provenance = sessionProvenance({ prompt, skillsText, contractText, bugReportText,
    scenarioPaths: suitesFor(track, args.level).map(suite => suite.spec),
    trackDir: track.dir, trackManifestPath: join(track.dir, 'track.json') });
  writeFileSync(join(args.app, `.prompt-${args.mode}-l${args.level}.md`), prompt);

  const started = Date.now();
  let raw = '';
  let spawnError = null;
  try {
    // The prompt goes in on stdin, not as an argument. Windows caps a command
    // line at 32767 characters, and the SpacetimeDB prompt carries the skill
    // documents on top of the level spec — it crossed that line as soon as L1
    // grew, and the spawn fails with a bare ENOENT that reads as "CLI missing".
    // The sandbox settings file is written into the app directory, so it reaches
    // a container through the same mount at a path that is known either way.
    const settings = writeSandbox(args.app);
    const cliEnv = { ...process.env,
      // Absent unless deliberately overridden — see THINKING_TOKENS above.
      ...((args.thinking ?? THINKING_TOKENS)
        ? { MAX_THINKING_TOKENS: String(args.thinking ?? THINKING_TOKENS) }
        : {}),
      // A CLI that updates itself mid-series changes the thing under test
      // between one backend and the next. The sequential harness has frozen
      // it since April; this one had not.
      DISABLE_AUTOUPDATER: '1',
      // Cache reads are ~69% of a run's bill, so cache TTL moves cost more
      // than anything else measured here. Unpinned, runs were getting the
      // 1-hour tier: a second run of the same backend within the hour reads
      // a prefix the first one paid to create and looks cheaper for reasons
      // that have nothing to do with the database. That is fatal for n=5
      // parallel trials, which would ALL share one warm prefix, and the
      // effect differs per backend because the prompts differ in size. The
      // 5-minute tier makes each trial pay its own way. The sequential
      // harness has pinned this since April for the same reason.
      FORCE_PROMPT_CACHING_5M: '1' };

    // Same CLI arguments, run somewhere the harness does not exist. The
    // container path is a separate script so that what a build can reach is
    // stated in one place — see container/run-build.mjs.
    raw = execFileSync(process.execPath, [
      join(ROOT, 'container', 'run-build.mjs'),
      '--app', args.app,
      '--backend', args.backend,
      '--image', imageIdentity.id,
      '--effort', EFFORT,
      '--model', args.model,
      '--settings', `/app/${basename(settings)}`,
      '--ports', [p.vite, p.express].filter(Boolean).join(','),
      ...(args.apiKey ? ['--api-key', args.apiKey] : []),
    ], { input: prompt, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, env: cliEnv,
      timeout: BUILD_SESSION_TIMEOUT_MS });
  } catch (err) {
    raw = (err.stdout || '').toString();
    spawnError = err.code
      ? `could not run the coding session: ${err.code} (${err.message.split('\n')[0]})`
      : null;
  }

  // The session is over, so the lint endpoint has no reason to stay open — and
  // an open listener would keep this process alive after it has printed its
  // result.
  killTree(lintServer.pid);

  let result = {};
  try { result = JSON.parse(raw); } catch { /* non-JSON means the session died */ }
  // A session that never started produced no code, and grading an empty
  // directory reports a real-looking 0 for the backend. Say so instead: this
  // failure mode cost a run once already, silently.
  if (spawnError || (!result.session_id && !raw.trim())) {
    console.error(`\nAGENT DID NOT RUN — ${spawnError ?? 'the session produced no output'}`);
  }
  const usage = result.usage ?? {};
  const input = usage.input_tokens ?? 0;
  const output = usage.output_tokens ?? 0;
  const cacheWrite = usage.cache_creation_input_tokens ?? 0;
  const cacheRead = usage.cache_read_input_tokens ?? 0;
  const turns = result.num_turns ?? null;

  // A single total cannot say WHY one backend cost more, and the answer is
  // almost never the database. Cost here is roughly (turns × prompt size):
  // cache reads dominate the token count and are re-paid every turn, so a
  // backend handed a bigger guidance pack pays more for identical work. Keep
  // the parts, so a cost difference can be attributed instead of assumed.
  const out = {
    appDir: args.app,
    mode: args.mode,
    level: args.level,
    track: args.track,
    backend: args.backend,
    model: args.model,
    guidance: args.guidance,
    stack: args.guidance === 'minimal' ? 'free' : 'prescribed',
    // The setup that produced this number. A score whose reasoning budget,
    // permission mode or CLI version is unknown cannot be compared with a later
    // one — the run would look identical in the record and not be.
    setup: {
      thinkingTokens: (args.thinking ?? THINKING_TOKENS) ? Number(args.thinking ?? THINKING_TOKENS) : 'cli default',
      permissionMode: 'acceptEdits',
      effort: EFFORT,
      // Which reference documents the model was handed. The cost work varies
      // this deliberately, so a number is meaningless without it.
      skills: selectedSkills,
      // Recorded because it materially changes cost, and because a figure whose
      // cache tier is unknown cannot be compared with one taken later.
      cacheTier: '5m',
      autoUpdater: 'disabled',
      // In a container the host CLI is not the one that ran, so the version is
      // read from the image. Reporting the host's would attribute a number to
      // software that took no part in producing it.
      cliVersion: imageCliVersion(imageIdentity.id),
      // Both the human-facing reference and immutable content identity are
      // recorded; the latter is what Docker actually executes.
      isolation: { mode: 'container', image: imageIdentity.reference,
        imageId: imageIdentity.id, hostAlias: HOST_ADDR },
      // Whether the run billed to a key or to the plan. Cost figures from the
      // two are not the same measurement.
      auth: (args.apiKey ?? process.env.ANTHROPIC_API_KEY) ? 'api-key' : 'credentials',
      // What is actually being benchmarked, not just what drove it.
      ...executeStackCapability(adapter, 'agent', 'setup-metadata', {
        imageId: imageIdentity.id,
        localPackage: LOCAL_PKG,
        env: process.env,
        helpers: { linuxSpacetimeVersion, bindingsIdentity, containerImage },
      }),
      // Ambient variables that could have influenced the model, recorded so the
      // question "what settings produced this" has an answer.
      env: ambientEnv(),
      // The orchestrator and coding session are separate runtimes. Recording
      // only the host Node version would attribute the model's tool execution
      // to a binary that never entered its container.
      node: { orchestrator: process.version, codingContainer: imageNodeVersion(imageIdentity.id) },
      platform: process.platform,
    },
    costUsd: Number((result.total_cost_usd ?? 0).toFixed(4)),
    tokens: input + output + cacheWrite + cacheRead,
    outputTokens: output,
    usage: { input, output, cacheWrite, cacheRead },
    provenance,
    turns,
    // What the model was handed before it did anything — the denominator for
    // every per-turn cost, and the axis the benchmark is least fair on.
    promptBytes: Buffer.byteLength(prompt),
    tokensPerTurn: turns ? Math.round((input + output + cacheWrite + cacheRead) / turns) : null,
    // Reasoning volume, measured rather than assumed — the budget is unpinned,
    // so this is how a change in the CLI default becomes visible in the record.
    thinking: thinkingVolume(args.app, result.session_id),
    durationMs: Date.now() - started,
    sessionId: result.session_id ?? null,
    ok: result.is_error === false,
  };
  writeFileSync(join(args.app, `.session-${args.mode}-l${args.level}.json`), JSON.stringify({ ...out, text: result.result }, null, 2));
  console.log(JSON.stringify(out));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(err => { console.error(err); process.exit(1); });
}
