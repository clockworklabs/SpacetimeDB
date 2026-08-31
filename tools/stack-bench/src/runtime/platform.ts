// Cleanup helpers must not hide the original failure when platform tools are absent.

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

export const isWindows = process.platform === 'win32';

/** Discard a command's output the way this platform expects. */
export const nullDevice = isWindows ? 'NUL' : '/dev/null';

export interface ProcessIdentity { pid: number; startMarker: string }

export function processIdentity(pidValue: string | number): ProcessIdentity | null {
  const pid = Number(pidValue);
  if (!Number.isSafeInteger(pid) || pid <= 0) return null;
  let startMarker = '';
  if (isWindows) {
    startMarker = run('powershell', ['-NoProfile', '-Command',
      `(Get-Process -Id ${pid} -ErrorAction Stop).StartTime.ToUniversalTime().Ticks`]).trim();
  } else {
    try {
      const stat = readFileSync(`/proc/${pid}/stat`, 'utf8');
      const fields = stat.slice(stat.lastIndexOf(')') + 2).trim().split(/\s+/);
      startMarker = fields[19] ?? '';
    } catch { return null; }
  }
  return /^\d+$/.test(startMarker) ? { pid, startMarker } : null;
}

export function processIdentityMatches(expected: ProcessIdentity): boolean {
  const actual = processIdentity(expected.pid);
  return actual?.startMarker === expected.startMarker;
}

interface CommandResult {
  ok: boolean;
  code: number | null;
  output: string;
  error?: unknown;
}

function errorField(error: unknown, field: 'status' | 'stdout'): unknown {
  return typeof error === 'object' && error !== null ? Reflect.get(error, field) : undefined;
}

const run = (cmd: string, args: readonly string[]): string => {
  try {
    return execFileSync(cmd, [...args], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
      timeout: 30_000,
    });
  } catch { return ''; }
};

const runResult = (cmd: string, args: readonly string[]): CommandResult => {
  try {
    return { ok: true, code: 0, output: execFileSync(cmd, [...args], {
      encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], timeout: 30_000,
    }) };
  } catch (error) {
    const status = errorField(error, 'status');
    return {
      ok: false,
      code: typeof status === 'number' ? status : null,
      output: `${errorField(error, 'stdout') ?? ''}`,
      error,
    };
  }
};

/** PIDs listening on a TCP port. */
export function pidsOnPort(
  port: string | number,
  { strict = false }: { strict?: boolean } = {},
): string[] {
  const out = new Set<string>();
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
      for (const match of line.matchAll(/pid=(\d+)/g)) {
        const pid = match[1];
        if (pid !== undefined) out.add(pid);
      }
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
      for (const match of line.matchAll(/pid=(\d+)/g)) {
        const pid = match[1];
        if (pid !== undefined) out.add(pid);
      }
    }
  }
  return [...out];
}

// Never return this process or an ancestor as a cleanup target.
function ancestorPids(): Set<number> {
  const chain = new Set<number>([process.pid]);
  try {
    if (isWindows) {
      const out = run('powershell', ['-NoProfile', '-Command',
        'Get-CimInstance Win32_Process | ForEach-Object { "{0} {1}" -f $_.ProcessId, $_.ParentProcessId }']);
      const parent = new Map<number, number>();
      for (const line of out.split(/\r?\n/)) {
        const [pid, ppid] = line.trim().split(/\s+/);
        if (pid) parent.set(Number(pid), Number(ppid));
      }
      let p = process.pid;
      while (parent.has(p)) {
        const next = parent.get(p);
        if (next === undefined || chain.has(next)) break;
        p = next;
        chain.add(p);
      }
    } else {
      const out = run('ps', ['-eo', 'pid=,ppid=']);
      const parent = new Map<number, number>();
      for (const line of out.split('\n')) {
        const [pid, ppid] = line.trim().split(/\s+/);
        if (pid) parent.set(Number(pid), Number(ppid));
      }
      let p = process.pid;
      while (parent.has(p)) {
        const next = parent.get(p);
        if (next === undefined || chain.has(next)) break;
        p = next;
        chain.add(p);
      }
    }
  } catch { /* a partial chain still protects this process itself */ }
  return chain;
}

export function pidsMatching(needle: string): string[] {
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

export function processTreePids(rootPid: string | number, processRows: unknown): number[] {
  const root = Number(rootPid);
  if (!Number.isSafeInteger(root) || root <= 0) return [];
  const children = new Map<number, number[]>();
  for (const line of String(processRows ?? '').split(/\r?\n/)) {
    const match = line.trim().match(/^(\d+)\s+(\d+)$/);
    if (!match) continue;
    const pidText = match[1];
    const parentText = match[2];
    if (pidText === undefined || parentText === undefined) continue;
    const pid = Number(pidText);
    const parent = Number(parentText);
    const siblings = children.get(parent) ?? [];
    if (!children.has(parent)) children.set(parent, siblings);
    siblings.push(pid);
  }
  const ordered: number[] = [];
  const seen = new Set<number>([root]);
  const visit = (pid: number): void => {
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
export function killTree(pid: string | number | null | undefined): void {
  if (!pid || String(pid) === '0' || Number(pid) === process.pid) return;
  if (isWindows) run('taskkill', ['/F', '/PID', String(pid), '/T']);
  else {
    const tree = processTreePids(pid, run('ps', ['-eo', 'pid=,ppid=']));
    run('kill', ['-9', ...tree.map(String)]);
  }
}

/** Kill a child spawned with detached:true, including its process group. */
export function killDetachedTree(pid: string | number | null | undefined): void {
  if (!pid || String(pid) === '0' || Number(pid) === process.pid) return;
  if (isWindows) { killTree(pid); return; }
  try { process.kill(-Number(pid), 'SIGKILL'); }
  catch { killTree(pid); }
}

/** Block for `ms` without a child process — `timeout` needs a console on Windows. */
export function sleepSync(ms: number): void {
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
export function sampleProcesses(needle: string): { byPid: Map<string, number>; rss: number } {
  const byPid = new Map<string, number>();
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
    const pid = m[1];
    const time = m[2];
    const kb = m[3];
    if (pid === undefined || time === undefined || kb === undefined) continue;
    const parts = time.replace('-', ':').split(':').map(Number);
    const secs = parts.reduce((acc, v) => acc * 60 + v, 0);
    byPid.set(pid, secs);
    rss += Number(kb) * 1024;
  }
  return { byPid, rss };
}

/** Is something answering at this URL? Any response counts. */
export function answers(url: string, timeoutSec = 3): boolean {
  try {
    execFileSync('curl', ['-s', '-m', String(timeoutSec), '-o', nullDevice, url], { stdio: 'ignore' });
    return true;
  } catch { return false; }
}
