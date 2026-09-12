// Standalone development process command; Node built-ins only.
import { spawn } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync, openSync, closeSync, rmSync, realpathSync } from 'node:fs';
import { resolve, relative, isAbsolute, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

function marker(pid: number): string | null {
  try {
    const stat = readFileSync(`/proc/${pid}/stat`, 'utf8');
    const fields = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
    return fields[0] === 'Z' ? null : fields[19] ?? null;
  } catch { return null; }
}

export function validateProject(root: string, server: string, database: string): void {
  const read = (name: string) => JSON.parse(readFileSync(join(root, name), 'utf8'));
  const config = { ...read('spacetime.json'),
    ...(existsSync(join(root, 'spacetime.local.json')) ? read('spacetime.local.json') : {}) };
  if (config.server !== server || config.database !== database || config.publish !== undefined) {
    throw new Error('Configure one database using the supplied server URL and database name.');
  }
  const contained = (path: unknown): string => {
    if (typeof path !== 'string' || !path.trim()) throw new Error('Configure module-path and generate out-dir.');
    const full = resolve(root, path);
    let existing = full;
    while (!existsSync(existing)) existing = dirname(existing);
    for (const candidate of [full, realpathSync(existing)]) {
      const rel = relative(realpathSync(root), candidate);
      if (isAbsolute(rel) || rel === '..' || rel.startsWith('../')) throw new Error('Project paths must stay inside the application directory.');
    }
    return full;
  };
  if (!existsSync(contained(config['module-path']))) throw new Error('Create the module directory before starting development.');
  if (!Array.isArray(config.generate) || !config.generate.length) throw new Error('Configure at least one generate target.');
  for (const target of config.generate) {
    if (!target || target.language !== 'typescript') throw new Error('Configure TypeScript generate targets.');
    contained(target['out-dir']);
    if (target['module-path'] !== undefined) contained(target['module-path']);
    if ((target.server !== undefined && target.server !== server)
      || (target.database !== undefined && target.database !== database)) throw new Error('Generate targets must use the supplied database.');
  }
}

// Calls are serialized by the installed flock wrapper. The detached watcher is
// still owned by the agent UID and the existing container teardown stops it.
export async function main(args: string[]): Promise<void> {
  const [root, state, cli, server, database, command = 'status', ...extra] = args;
  if (!root || !state || !cli || !server || !database || extra.length
    || !['start', 'status', 'stop'].includes(command)) throw new Error('Usage: spacetime-dev start|status|stop');
  const record = join(state, 'process.json'), ready = join(state, 'ready'), log = join(state, 'watcher.log');
  const previous = existsSync(record) ? JSON.parse(readFileSync(record, 'utf8')) : null;
  const alive = () => previous && Number.isSafeInteger(previous.pid) && previous.pid > 1
    && typeof previous.marker === 'string' && marker(previous.pid) === previous.marker;
  if (command === 'stop') {
    if (alive()) {
      // Stop the whole build group together, including a compiler child.
      process.kill(-previous.pid, 'SIGKILL');
      for (let i = 0; i < 50 && alive(); i++) await delay(100);
      if (alive()) throw new Error(`Watcher did not stop. Read ${log}`);
    }
    rmSync(record, { force: true }); rmSync(ready, { force: true });
    console.log(`Stopped. Log: ${log}`); return;
  }
  if (alive()) {
    console.log(`${existsSync(ready) ? 'Running; initial publish and bindings completed' : 'Starting; initial publish not yet confirmed'}. Log: ${log}`);
    return;
  }
  if (command === 'status') { console.log(`Not running. Log: ${log}`); return; }
  validateProject(root, server, database);
  rmSync(ready, { force: true });
  const output = openSync(log, 'w', 0o600);
  const child = spawn(cli, ['dev', '--yes', '--delete-data=never', '--server-only', '--ready-file', ready],
    { cwd: root, detached: true, stdio: ['ignore', output, output] });
  closeSync(output);
  await new Promise<void>((done, reject) => { child.once('spawn', done); child.once('error', reject); });
  const pid = child.pid!;
  const identity = marker(pid);
  if (!identity) throw new Error(`Watcher exited at startup. Read ${log}`);
  writeFileSync(record, JSON.stringify({ pid, marker: identity }));
  child.unref();
  // Bounded startup observation; a slow compile keeps running and status can
  // inspect it later. Never mistake a live process for a completed publish.
  for (let i = 0; i < 50; i++) {
    if (marker(pid) !== identity) throw new Error(`Watcher exited. Read ${log}`);
    if (existsSync(ready)) { console.log(`Running; initial publish and bindings completed. Log: ${log}`); return; }
    await delay(100);
  }
  console.log(`Starting; initial publish not yet confirmed. Run spacetime-dev status. Log: ${log}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch(error => { console.error(error.message); process.exitCode = 1; });
}
