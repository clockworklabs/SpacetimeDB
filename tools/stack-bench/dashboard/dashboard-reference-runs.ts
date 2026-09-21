import { execFile } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.js';

const exec = promisify(execFile);
const docker = async (args: string[]) => {
  const result = await exec('docker', args,
    { timeout: 5000, maxBuffer: 4 * 1024 * 1024, windowsHide: true });
  return result.stdout + (args[0] === 'logs' ? result.stderr : '');
};

export interface ReferenceRun {
  id: string;
  title: string;
  status: 'running' | 'passed' | 'failed' | 'incomplete';
  updatedAt: string;
  points: { passed: number; measured: number; planned: number | null } | null;
  log: string;
}

export function referenceLogPoints(log: string): ReferenceRun['points'] {
  // A repetition starts a new measurement. Do not add repeated scores together.
  const latest = log.split(/qualifying [^\n]+: clean run \d+\/\d+/).at(-1) ?? '';
  const rows = [...latest.matchAll(/^\s+selected-source-\d+ \.\.\. (\d+)\/(\d+)\s*$/gm)];
  if (!rows.length) return null;
  const scope = latest.match(/scope: \d+ check\(s\), (\d+) point\(s\)/);
  return { passed: rows.reduce((n, row) => n + Number(row[1]), 0),
    measured: rows.reduce((n, row) => n + Number(row[2]), 0), planned: scope ? Number(scope[1]) : null };
}

export async function referenceRuns(resultsRoot: string, readDocker = docker): Promise<{ runs: ReferenceRun[]; error: string | null }> {
  const root = resolve(resultsRoot, 'reference-live');
  const runs = new Map<string, ReferenceRun>();
  // Finished artifacts remain visible after their controller containers are removed.
  if (existsSync(root)) for (const file of readdirSync(root, { withFileTypes: true })) {
    if (!file.isFile() || !file.name.endsWith('.json') || file.name.endsWith('.inputs.json')) continue;
    try {
      const value = JSON.parse(readFileSync(join(root, file.name), 'utf8'));
      const artifact = value.payload ?? value;
      if (artifact.kind !== 'reference_qualification') continue;
      const score = String(artifact.runs?.at(-1)?.score ?? '').match(/^(\d+)\/(\d+)$/);
      runs.set(file.name, { id: file.name, title: String(artifact.fixture ?? file.name),
        status: artifact.ok === true ? 'passed' : 'failed',
        updatedAt: String(artifact.completedAt ?? artifact.startedAt ?? ''),
        points: score ? { passed: Number(score[1]), measured: Number(score[2]), planned: Number(score[2]) } : null,
        log: redactCredentials((artifact.runs ?? []).flatMap((run: { failures?: string[] }) => run.failures ?? []).join('\n')) });
    } catch { /* A qualification artifact may be in the middle of an atomic replacement. */ }
  }
  try {
    const listing = await readDocker(['ps', '-a', '--no-trunc', '--format', '{{json .}}']);
    const ids = listing.trim().split('\n').filter(Boolean).map(line => JSON.parse(line))
      .filter(row => String(row.Command).includes('/references/reference-live.js'))
      .map(row => String(row.ID)).filter(id => /^[a-f0-9]{64}$/.test(id));
    if (ids.length) {
      const containers = JSON.parse(await readDocker(['inspect', ...ids]));
      await Promise.all(containers.map(async (container: { Id: string; Args: string[];
        State: { Running: boolean; StartedAt: string; FinishedAt: string } }) => {
        const args = container.Args;
        const output = args[args.indexOf('--out') + 1];
        // Only this dashboard's reference output tree belongs here.
        if (!args.includes('--out') || !output || resolve(output) !== join(root, basename(output))) return;
        const id = basename(output), final = runs.get(id);
        const level = args[args.indexOf('--level') + 1];
        const stack = args[args.indexOf('--backend') + 1];
        let log: string;
        try { log = await readDocker(['logs', '--tail', '4000', container.Id]); }
        catch { log = 'Controller log is unavailable.'; }
        runs.set(id, { id, title: `${stack} L${level} reference validation`,
          status: container.State.Running ? 'running' : final?.status ?? 'incomplete',
          updatedAt: container.State.Running ? container.State.StartedAt : container.State.FinishedAt,
          points: final?.points ?? referenceLogPoints(log),
          log: redactCredentials(log).slice(-96 * 1024) + (final?.log ? `\n${final.log}` : '') });
      }));
    }
    return { runs: [...runs.values()].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)), error: null };
  } catch {
    return { runs: [...runs.values()], error: 'Live reference status is unavailable. Docker could not be read.' };
  }
}
