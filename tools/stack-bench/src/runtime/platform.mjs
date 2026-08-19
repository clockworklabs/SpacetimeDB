// The few things the harness does that differ per operating system.
//
// Everything else here is Node and bash, which travel. Process and port
// control did not: the whole of it was taskkill, netstat and PowerShell with
// no fallback, so the benchmark could only ever have run on Windows. The
// nemesis phase wants Linux runners, and a result that cannot be reproduced on
// another machine is worth less than one that can.
//
// Each function returns something sane when its tools are missing rather than
// throwing, because process cleanup runs in teardown paths where a failure
// would mask whatever actually went wrong.

import { execFileSync } from 'node:child_process';

export const isWindows = process.platform === 'win32';

/** Discard a command's output the way this platform expects. */
export const nullDevice = isWindows ? 'NUL' : '/dev/null';

const run = (cmd, args) => {
  try {
    return execFileSync(cmd, args, {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
      timeout: 30_000,
    });
  } catch { return ''; }
};

const runResult = (cmd, args) => {
  try {
    return { ok: true, code: 0, output: execFileSync(cmd, args, {
      encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], timeout: 30_000,
    }) };
  } catch (error) {
    return { ok: false, code: error.status ?? null, output: `${error.stdout ?? ''}`, error };
  }
};

/** PIDs listening on a TCP port. */
export function pidsOnPort(port, { strict = false } = {}) {
  const out = new Set();
  if (isWindows) {
    const result = runResult('netstat', ['-ano']);
    if (!result.ok && strict) throw new Error(`could not inspect listeners on :${port}`);
    for (const line of result.output.split('\n')) {
      if (!new RegExp(`:${port}\\s`).test(line) || !/LISTENING/i.test(line)) continue;
      const pid = line.trim().split(/\s+/).pop();
      if (pid && pid !== '0') out.add(pid);
    }
    return [...out];
  }
  // lsof is the common case; ss covers minimal containers that lack it.
  const lsof = runResult('lsof', ['-ti', `tcp:${port}`, '-sTCP:LISTEN']);
  // lsof uses exit 1 for a successful query with no matches.
  if (!lsof.ok && lsof.code !== 1 && strict) {
    const ss = runResult('ss', ['-lptn']);
    if (!ss.ok) throw new Error(`could not inspect listeners on :${port}`);
    for (const line of ss.output.split('\n')) {
      if (!new RegExp(`(?:^|\\s)(?:\\[[^\\]]+\\]|\\S+):${port}\\s`).test(line)) continue;
      for (const match of line.matchAll(/pid=(\d+)/g)) out.add(match[1]);
    }
    return [...out];
  }
  for (const pid of lsof.output.split('\n')) {
    if (pid.trim()) out.add(pid.trim());
  }
  if (!out.size) {
    const ss = runResult('ss', ['-lptn']);
    if (!ss.ok) {
      if (strict && lsof.code !== 1) throw new Error(`could not inspect listeners on :${port}`);
      return [];
    }
    for (const line of ss.output.split('\n')) {
      if (!new RegExp(`(?:^|\\s)(?:\\[[^\\]]+\\]|\\S+):${port}\\s`).test(line)) continue;
      for (const match of line.matchAll(/pid=(\d+)/g)) out.add(match[1]);
    }
  }
  return [...out];
}

/**
 * PIDs whose command line mentions `needle` — a dev server holding a directory.
 *
 * The query must not match ITSELF. The PowerShell that runs the search carries
 * the needle in its own command line, so an unfiltered version returns the
 * searcher among the results: a cleanup that killed what this returned would
 * shoot the process asking the question. Confirmed by running it — the only
 * "match" was the powershell doing the matching.
 *
 * Note this finds processes that NAME the directory, not processes standing in
 * it. `npm run dev` shows as `npm-cli.js run dev` with the path nowhere in
 * sight, which is exactly the leftover that wedged a run. Unique per-run work
 * directories are what actually fix that; this narrows the blast radius.
 */
// The pids this process hangs from: itself, its parent shell, that shell's
// parent, up to the root. Killing anything in this chain kills the run —
// which is not hypothetical: an invocation carrying the app path in its own
// `--app` argument matched itself, killTree took out the invoking bash, and
// the command died with no output and nothing to debug from.
function ancestorPids() {
  const chain = new Set([process.pid]);
  try {
    if (isWindows) {
      const out = run('powershell', ['-NoProfile', '-Command',
        'Get-CimInstance Win32_Process | ForEach-Object { "{0} {1}" -f $_.ProcessId, $_.ParentProcessId }']);
      const parent = new Map();
      for (const line of out.split(/\r?\n/)) {
        const [pid, ppid] = line.trim().split(/\s+/);
        if (pid) parent.set(Number(pid), Number(ppid));
      }
      let p = process.pid;
      while (parent.has(p) && !chain.has(parent.get(p))) { p = parent.get(p); chain.add(p); }
    } else {
      const out = run('ps', ['-eo', 'pid=,ppid=']);
      const parent = new Map();
      for (const line of out.split('\n')) {
        const [pid, ppid] = line.trim().split(/\s+/);
        if (pid) parent.set(Number(pid), Number(ppid));
      }
      let p = process.pid;
      while (parent.has(p) && !chain.has(parent.get(p))) { p = parent.get(p); chain.add(p); }
    }
  } catch { /* a partial chain still protects this process itself */ }
  return chain;
}

export function pidsMatching(needle) {
  const protectedPids = ancestorPids();
  let found;
  if (isWindows) {
    const script = `Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like '*${
      String(needle).replace(/'/g, "''")}*' -and $_.ProcessId -ne $PID `
      + `-and $_.CommandLine -notlike '*Get-CimInstance*' } | ForEach-Object { $_.ProcessId }`;
    found = run('powershell', ['-NoProfile', '-Command', script]).split(/\s+/);
  } else {
    found = run('pgrep', ['-f', needle]).split('\n').map(s => s.trim());
  }
  return found.filter(Boolean).filter(p => !protectedPids.has(Number(p)));
}

export function processTreePids(rootPid, processRows) {
  const root = Number(rootPid);
  if (!Number.isSafeInteger(root) || root <= 0) return [];
  const children = new Map();
  for (const line of String(processRows ?? '').split(/\r?\n/)) {
    const match = line.trim().match(/^(\d+)\s+(\d+)$/);
    if (!match) continue;
    const pid = Number(match[1]);
    const parent = Number(match[2]);
    if (!children.has(parent)) children.set(parent, []);
    children.get(parent).push(pid);
  }
  const ordered = [];
  const seen = new Set([root]);
  const visit = pid => {
    for (const child of children.get(pid) ?? []) {
      if (seen.has(child)) continue;
      seen.add(child);
      visit(child);
      ordered.push(child);
    }
  };
  visit(root);
  ordered.push(root);
  return ordered;
}

/** Kill a process and its descendants. Never throws — it may already be gone. */
export function killTree(pid) {
  if (!pid || String(pid) === '0' || Number(pid) === process.pid) return;
  if (isWindows) run('taskkill', ['/F', '/PID', String(pid), '/T']);
  else {
    const tree = processTreePids(pid, run('ps', ['-eo', 'pid=,ppid=']));
    run('kill', ['-9', ...tree.map(String)]);
  }
}

/** Kill a child spawned with detached:true, including its process group. */
export function killDetachedTree(pid) {
  if (!pid || String(pid) === '0' || Number(pid) === process.pid) return;
  if (isWindows) { killTree(pid); return; }
  try { process.kill(-Number(pid), 'SIGKILL'); }
  catch { killTree(pid); }
}

/** Block for `ms` without a child process — `timeout` needs a console on Windows. */
export function sleepSync(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

/**
 * CPU seconds and resident bytes for every process whose command line or name
 * mentions `needle`, keyed by pid.
 *
 * Keyed rather than summed because a dev server is a watcher supervising a
 * child: the set changes mid-run, and summing a changing set across two
 * samples produces negative CPU. CPU is cumulative for the process, so the
 * difference between two samples is the work done in between.
 */
export function sampleProcesses(needle) {
  const byPid = new Map();
  let rss = 0;
  if (isWindows) {
    const script = `Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like '*${needle}*' -or $_.Name -like '*${needle}*' } | ForEach-Object {
      $p = Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
      if ($p) { "{0}|{1}|{2}" -f $p.Id, $p.TotalProcessorTime.TotalSeconds, $p.WorkingSet64 } }`;
    for (const line of run('powershell', ['-NoProfile', '-Command', script]).split(/\r?\n/)) {
      const [pid, cpu, ws] = line.split('|');
      if (!pid || cpu === undefined) continue;
      byPid.set(pid, Number(cpu));
      rss += Number(ws) || 0;
    }
    return { byPid, rss };
  }
  // ps gives cumulative CPU as [dd-]hh:mm:ss and RSS in kilobytes.
  for (const line of run('ps', ['-eo', 'pid=,time=,rss=,args=']).split('\n')) {
    if (!line.includes(needle)) continue;
    const m = line.trim().match(/^(\d+)\s+([\d:-]+)\s+(\d+)\s/);
    if (!m) continue;
    const [, pid, time, kb] = m;
    const parts = time.replace('-', ':').split(':').map(Number);
    const secs = parts.reduce((acc, v) => acc * 60 + v, 0);
    byPid.set(pid, secs);
    rss += Number(kb) * 1024;
  }
  return { byPid, rss };
}

/** Is something answering at this URL? Any response counts. */
export function answers(url, timeoutSec = 3) {
  try {
    execFileSync('curl', ['-s', '-m', String(timeoutSec), '-o', nullDevice, url], { stdio: 'ignore' });
    return true;
  } catch { return false; }
}
