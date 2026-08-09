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
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, rmSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { loadTrack, levelPrompt, appendix, dbName, moduleName, portsFor, DEFAULT_TRACK } from './tracks.mjs';
import { writeSandbox } from './sandbox.mjs';
import { killTree } from './platform.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(ROOT, '..', '..');

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

function cliVersion(bin) {
  try {
    return execFileSync(bin, ['--version'], { encoding: 'utf8', stdio: 'pipe' }).trim().split('\n')[0];
  } catch { return 'unknown'; }
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

function findClaude() {
  const appData = process.env.APPDATA ?? join(process.env.HOME ?? '', 'AppData', 'Roaming');
  const desktop = join(appData, 'Claude', 'claude-code');
  if (existsSync(desktop)) {
    const versions = readdirSync(desktop).sort();
    const exe = join(desktop, versions[versions.length - 1], 'claude.exe');
    if (existsSync(exe)) return exe;
  }
  return 'claude';
}

const dbUrl = (backend, runIndex, dbPort, track) =>
  backend === 'postgres'
    ? `postgresql://stackbench:stackbench@localhost:${dbPort}/${dbName(track, runIndex)}`
    : `mongodb://localhost:${dbPort}/${dbName(track, runIndex)}`;

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
function ensureDatabase(backend, runIndex, dbPort, track, wipe = false) {
  const name = dbName(track, runIndex);
  if (backend === 'postgres') {
    const container = process.env.POSTGRES_CONTAINER ?? 'stack-bench-postgres';
    try {
      execFileSync('docker', ['exec', container, 'psql', '-U', 'stackbench', '-d', 'postgres',
        '-c', `CREATE DATABASE ${name} OWNER stackbench;`], { stdio: 'pipe' });
    } catch { /* already exists */ }
    if (wipe) {
      try {
        execFileSync('docker', ['exec', container, 'psql', '-U', 'stackbench', '-d', name,
          '-c', 'DROP SCHEMA public CASCADE; CREATE SCHEMA public; '
              + 'GRANT ALL ON SCHEMA public TO stackbench;'], { stdio: 'pipe' });
        console.error(`  wiped ${name} (schema dropped) — a build starts on an empty database`);
      } catch (e) {
        console.error(`  WARNING: could not wipe ${name}: ${String(e.message).split('\n')[0]}`);
      }
    }
  } else if (backend === 'spacetime' && wipe) {
    // A published module survives a run. If the next build's schema is not
    // compatible with it, publish aborts demanding manual migration or
    // --delete-data — friction this benchmark has recorded and charged to
    // SpacetimeDB, when the cause was our own leftovers. Delete the module so
    // a build starts against nothing at all.
    try {
      execFileSync(STDB_BIN, ['delete', moduleName(track, runIndex), '-s', STDB_URI, '-y'],
        { stdio: 'pipe' });
      console.error(`  deleted module ${moduleName(track, runIndex)} — a build starts with none published`);
    } catch { /* not published is the desired state */ }
  } else if (backend === 'mongodb' && wipe) {
    const container = process.env.MONGO_CONTAINER ?? 'stack-bench-mongodb';
    try {
      execFileSync('docker', ['exec', container, 'mongosh', name, '--quiet',
        '--eval', 'db.dropDatabase()'], { stdio: 'pipe' });
      console.error(`  wiped ${name} — a build starts on an empty database`);
    } catch (e) {
      console.error(`  WARNING: could not wipe ${name}: ${String(e.message).split('\n')[0]}`);
    }
  }
  return name;
}

// prescribed: the stack is chosen for them (Express, socket.io, an ORM, a layout).
// minimal: only the database, the ports the harness needs, and branding — how to
// build it is the model's call. Prescribing a stack means measuring the stack we
// picked, not the database.
function backendDoc(args, p, track) {
  if (args.guidance === 'minimal' && args.backend === 'spacetime') {
    console.error('--stack free does not apply to spacetime: it IS the stack, so there is no '
      + 'framework, realtime mechanism or ORM to choose. Compare postgres/mongodb free runs '
      + 'against their own prescribed runs; spacetime prescribed is the reference.');
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
    .replaceAll('<DATABASE_URL>', p.dbPort ? dbUrl(args.backend, args.runIndex, p.dbPort, track) : '')
    .replaceAll('<STDB_URI>', STDB_URI)
    .replaceAll('<STDB_BIN>', fwd(STDB_BIN))
    .replaceAll('<STDB_PACKAGE>', `file:${fwd(LOCAL_PKG)}`);
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
const DEFAULT_SKILLS = ['typescript-server', 'typescript-client'];

function skillDocs(backend, skills) {
  if (backend !== 'spacetime') return '';
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

function writeLintShim(appDir, port) {
  const sh = join(appDir, 'check-hooks.sh');
  writeFileSync(sh, '#!/usr/bin/env bash\n'
    + '# Verifies the required data-testid hooks resolve in the running app.\n'
    + `curl -sS --fail-with-body http://127.0.0.1:${port}/lint\n`);
  return './check-hooks.sh';
}

function buildPrompt(args, p, track, lintPort) {
  const lint = args.printPrompt ? './check-hooks.sh' : writeLintShim(args.app, lintPort);
  const common = [
    `The app lives in ${args.app.replace(/\\/g, '/')} — work there.`,
    '',
    backendDoc(args, p, track),
  ];
  const skills = skillDocs(args.backend, args.skills ?? DEFAULT_SKILLS);
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
    levelPrompt(track, args.level),
    appendix(track, args.level),
  ].join('\n');
}

// A build must not be able to read the thing that grades it.
// The deny list itself lives in sandbox.mjs, shared with probe-sandbox.mjs so
// the probe proves the rules a build actually gets.

async function main() {
  const args = parseArgs(process.argv);
  const track = loadTrack(args.track);
  const p = portsFor(track, args.backend, args.runIndex);
  // Renders what the model would be given, without spending anything or
  // touching the app directory. The regression gate for harness changes is a
  // diff of this against a saved copy.
  if (args.printPrompt) { process.stdout.write(buildPrompt(args, p, track)); return; }
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

  const prompt = buildPrompt(args, p, track, lintPort);
  writeFileSync(join(args.app, `.prompt-${args.mode}-l${args.level}.md`), prompt);

  const started = Date.now();
  let raw = '';
  let spawnError = null;
  try {
    // The prompt goes in on stdin, not as an argument. Windows caps a command
    // line at 32767 characters, and the SpacetimeDB prompt carries the skill
    // documents on top of the level spec — it crossed that line as soon as L1
    // grew, and the spawn fails with a bare ENOENT that reads as "CLI missing".
    raw = execFileSync(findClaude(), [
      '--print', '--output-format', 'json',
      // NOT --dangerously-skip-permissions: that is bypassPermissions, which
      // switches the permission system off entirely — deny rules included.
      // probe-sandbox.mjs demonstrates it: under the bypass flag all five
      // probed paths come back in full, and under acceptEdits all five are
      // refused. The mode must be named explicitly, because the default mode
      // withholds approval from Write and Edit and a build cannot write files.
      '--permission-mode', 'acceptEdits',
      '--settings', writeSandbox(args.app),
      '--model', args.model,
      // Narrows what the session is POINTED at, which is hygiene rather than
      // isolation: `--dangerously-skip-permissions` makes `--add-dir` advisory,
      // and sessions have been observed reading `tracks/*/scenarios/*.json`
      // (the graded assertions) and copying a prior run's snapshot regardless.
      // Isolation is the sandbox settings above and keeping the harness out of
      // the app's ancestry; this argument alone guarantees nothing.
      // Everything the model legitimately needs (backend guidance, skill
      // documents, the level spec, the contract appendix) is inlined into the
      // prompt, and the linter is reached over loopback rather than by path.
      '--effort', EFFORT,
      '--add-dir', args.app,
    ], { cwd: args.app, input: prompt, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024,
      env: { ...process.env,
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
        FORCE_PROMPT_CACHING_5M: '1' } });
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
      skills: args.backend === 'spacetime' ? (args.skills ?? DEFAULT_SKILLS) : [],
      // Recorded because it materially changes cost, and because a figure whose
      // cache tier is unknown cannot be compared with one taken later.
      cacheTier: '5m',
      autoUpdater: 'disabled',
      cliVersion: cliVersion(findClaude()),
      node: process.version,
      platform: process.platform,
    },
    costUsd: Number((result.total_cost_usd ?? 0).toFixed(4)),
    tokens: input + output + cacheWrite + cacheRead,
    outputTokens: output,
    usage: { input, output, cacheWrite, cacheRead },
    turns,
    // What the model was handed before it did anything — the denominator for
    // every per-turn cost, and the axis the benchmark is least fair on.
    promptBytes: prompt.length,
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

main().catch(err => { console.error(err); process.exit(1); });
