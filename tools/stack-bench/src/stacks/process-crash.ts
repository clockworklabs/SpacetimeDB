import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import type { BackendLease } from '../runtime/backend-lease.js';
import { CODING_CONTAINER_AGENT } from '../runtime/coding-container-policy.js';
import { evidenceNowMs } from '../evidence/evidence-timing.js';
import { requireAttemptNetwork } from '../runtime/docker-network.js';
import { SPACETIME_PROCESS_RECORD } from './hosted-lifecycle.js';
import { CONVEX_PROCESS_RECORD } from './backends/convex-lifecycle.js';

const execute = promisify(execFile);
type Docker = (args: readonly string[]) => Promise<string>;
const docker: Docker = async args => (await execute('docker', [...args], {
  encoding: 'utf8', timeout: 30_000, maxBuffer: 1024 * 1024,
})).stdout;

export type CrashTarget = 'application' | 'database';
export interface ProcessCrashReceipt {
  backend: string;
  target: CrashTarget;
  containerId: string;
  requestedAtMs: number;
  completedAtMs: number;
  signal: 'SIGKILL';
  processEvidence: string;
  clockOffsetBeforeMs: number;
  clockOffsetAfterMs: number;
}

// The container remains alive: it owns the attempt's network namespace. Kill
// only application-user processes or the recorded native backend process group.
// /proc avoids adding process tools to the pinned database images.
function processCrashScript(processRecord: string | null): string {
  return `set -eu
self=$$; uid=$(id -u); group=""; killed=0
${processRecord ? `read group expected_start < ${processRecord}
case "$group:$expected_start" in *[!0-9:]*) exit 4;; esac
[ "$group" -gt 1 ] && [ "$expected_start" -gt 0 ] || exit 4
IFS= read -r stat < "/proc/$group/stat"; rest=\${stat##*) }; set -- $rest
[ "$3" = "$group" ] && [ "\${20}" = "$expected_start" ] || { echo 'stale backend process record' >&2; exit 4; }`
    : `[ "$uid" -gt 0 ] || { echo 'refusing user-wide root crash' >&2; exit 4; }`}
scan() {
  targets=""
  for entry in /proc/[0-9]*; do
    pid=\${entry##*/}; [ "$pid" != 1 ] && [ "$pid" != "$self" ] || continue
    [ -r "$entry/stat" ] && [ -r "$entry/status" ] || continue
    IFS= read -r stat < "$entry/stat" || continue; rest=\${stat##*) }; set -- $rest
    state=$1; pgid=$3; started=\${20}; [ "$state" != Z ] && [ "$state" != X ] || continue
    if [ -n "$group" ]; then [ "$pgid" = "$group" ] || continue
    else
      owner=""; while read -r key value rest; do [ "$key" != Uid: ] || { owner=$value; break; }; done < "$entry/status"
      [ "$owner" = "$uid" ] || continue
    fi
    targets="$targets $pid:$started"
  done
}
attempt=0
scan
echo ARMED
IFS= read -r trigger || exit 0
[ "$trigger" = CRASH ] || exit 4
while [ "$attempt" -lt 50 ]; do
  if [ -z "$targets" ]; then
    [ "$killed" -gt 0 ] || { echo 'no live process was crashed' >&2; exit 4; }
    echo QUIET; exit 0
  fi
  for identity in $targets; do
    pid=\${identity%:*}; started=\${identity#*:}
    [ -r "/proc/$pid/stat" ] || continue
    IFS= read -r stat < "/proc/$pid/stat" || continue; rest=\${stat##*) }; set -- $rest
    [ "\${20}" = "$started" ] || { echo 'process identity changed before crash' >&2; exit 4; }
    at=$(date +%s%3N)
    if kill -KILL "$pid"; then echo "KILLED $pid $started $at"; killed=$((killed + 1)); fi
  done
  attempt=$((attempt + 1)); sleep 0.1; scan
done
echo 'writers remained after SIGKILL' >&2; exit 4`;
}

export async function prepareProcessCrash(lease: BackendLease, target: CrashTarget) {
  if (!['postgres', 'mongodb', 'spacetime', 'convex'].includes(lease.backend)
    || !['application', 'database'].includes(target)) throw new Error('unsupported process crash boundary');
  if (['spacetime', 'convex'].includes(lease.backend) && target === 'application') {
    throw new Error(`${lease.backend} application and database share one boundary; use database`);
  }
  requireAttemptNetwork(lease);
  const container = target === 'application' ? lease.resources.buildContainer : lease.resources.container;
  if (!container?.owned) throw new Error('process crash requires an owned container');
  const actual = JSON.parse(await docker(['inspect', '--format',
    '{"id":{{json .Id}},"pidMode":{{json .HostConfig.PidMode}},"state":{{json .State}}}', container.name]));
  if (actual.id !== container.id || !actual.state.Running
    || !['', 'private'].includes(actual.pidMode)) {
    throw new Error('process crash requires the live leased container with a private PID namespace');
  }
  const user = target === 'application' ? `${CODING_CONTAINER_AGENT.uid}:${CODING_CONTAINER_AGENT.gid}`
    : lease.backend === 'postgres' ? 'postgres' : lease.backend === 'mongodb' ? 'mongodb' : '0:0';
  // Start Docker exec before dispatch. Its shell waits for one stdin line, so
  // Docker startup latency is outside the request/fault window. EOF disarms it.
  let arm!: () => void, failArm!: (error: Error) => void;
  const armed = new Promise<void>((resolve, reject) => { arm = resolve; failArm = reject; });
  let child!: ReturnType<typeof execFile>;
  const completed = new Promise<string>((resolve, reject) => {
    child = execFile('docker', ['exec', '-i', '--user', user, container.id, 'sh', '-c',
      processCrashScript(lease.backend === 'spacetime' ? SPACETIME_PROCESS_RECORD
        : lease.backend === 'convex' ? CONVEX_PROCESS_RECORD : null)],
    { encoding: 'utf8', timeout: 60_000, maxBuffer: 1024 * 1024 }, (error, stdout) => {
      failArm(new Error('fault process exited before it was armed'));
      if (error) reject(Object.assign(error, { stdout })); else resolve(stdout);
    });
  });
  // Setup and early cancellation own this rejection until crash() consumes it.
  void completed.catch(() => {});
  child.stdin!.on('error', () => { /* completed records a closed fault process. */ });
  let output = '';
  child.stdout!.on('data', chunk => { output += String(chunk); if (/^ARMED$/m.test(output)) arm(); });
  const close = async () => { child.stdin?.end(); await completed.catch(() => {}); };
  try { await armed; } catch (error) { await close(); throw error; }
  let fired = false;
  return { close, async crash(): Promise<ProcessCrashReceipt> {
    if (fired) throw new Error('prepared crash can fire only once');
    fired = true;
    const receipt: ProcessCrashReceipt = { backend: lease.backend, target, containerId: container.id,
      requestedAtMs: evidenceNowMs(), completedAtMs: 0, signal: 'SIGKILL', processEvidence: '',
      clockOffsetBeforeMs: Date.now() - evidenceNowMs(), clockOffsetAfterMs: 0 };
    try {
      child.stdin!.end('CRASH\n');
      receipt.processEvidence = await completed;
      receipt.completedAtMs = evidenceNowMs();
      receipt.clockOffsetAfterMs = Date.now() - evidenceNowMs();
      if (!/^KILLED \d+ \d+ \d+$/m.test(receipt.processEvidence) || !/^QUIET$/m.test(receipt.processEvidence)) {
        throw new Error('crash command did not prove a killed process and stopped writers');
      }
      return receipt;
    } catch (cause) {
      receipt.completedAtMs = evidenceNowMs();
      receipt.clockOffsetAfterMs = Date.now() - evidenceNowMs();
      if (cause && typeof cause === 'object' && 'stdout' in cause) receipt.processEvidence = String(cause.stdout);
      throw Object.assign(new Error('process crash was not verified', { cause }), { receipt });
    }
  } };
}
