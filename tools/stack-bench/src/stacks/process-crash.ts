import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import type { BackendLease } from '../runtime/backend-lease.js';
import { CODING_CONTAINER_AGENT } from '../runtime/coding-container-policy.js';
import { evidenceNowMs } from '../evidence/evidence-timing.js';
import { requireAttemptNetwork } from '../runtime/docker-network.js';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.js';

const execute = promisify(execFile);
type Docker = (args: readonly string[]) => Promise<string>;
const docker: Docker = async args => (await execute('docker', [...args], {
  encoding: 'utf8', timeout: 30_000, maxBuffer: 1024 * 1024,
})).stdout;

export type CrashTarget = 'application' | 'database';

// How a stack's database process is crashed, restarted and observed. Each stack
// declares this in its own module; its adapter carries it as `runtime`.
export interface StackDatabaseRuntime {
  // The application runs inside the database process: one fault boundary.
  readonly combinedBoundary: boolean;
  // The container user that owns the database processes and may signal them.
  readonly databaseUser: string;
  // The recorded database process group, or null to crash that user's processes.
  readonly processRecord: string | null;
  // Starts the crashed database again on its existing data.
  recoverDatabase(input: { leasePath: string; lease: BackendLease; signal: AbortSignal }): void;
  databaseReady(lease: BackendLease): Promise<boolean>;
  // Counts database work a crashed application left behind, as a command run in
  // the leased database container. Absent when the stacks share one boundary.
  drainCommand?(lease: BackendLease, database: string): string[];
}

export function stackDatabaseRuntime(backend: string): StackDatabaseRuntime | null {
  if (!STACK_ADAPTER_REGISTRY.ids.includes(backend)) return null;
  return STACK_ADAPTER_REGISTRY.get(backend).runtime ?? null;
}

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
  applicationFreeze?: {
    startedAtMs: number;
    completedAtMs: number;
    processes: Array<{ pid: number; startTicks: number; threads: number[] }>;
  };
}

// The container remains alive: it owns the attempt's network namespace. Kill
// only application-user processes or the recorded native backend process group.
// /proc avoids adding process tools to the pinned database images.
function processCrashScript(processRecord: string | null, applicationPort?: number): string {
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
    pgid=$3; started=\${20}
    if [ -n "$group" ]; then [ "$pgid" = "$group" ] || continue
    else
      owner=""; while read -r key value rest; do [ "$key" != Uid: ] || { owner=$value; break; }; done < "$entry/status"
      [ "$owner" = "$uid" ] || continue
    fi
    # A terminated leader can still own live threads. An unreadable task list
    # is not proof of quiescence; retain that group for bounded KILL cleanup.
    live=0
    for task in "$entry"/task/[0-9]*; do
      if IFS= read -r stat < "$task/stat"; then
        rest=\${stat##*) }; set -- $rest
        case "$1" in Z|X) ;; *) live=1; break;; esac
      else live=1; break; fi
    done
    [ "$live" = 1 ] || continue
    targets="$targets $pid:$started"
  done
}
${applicationPort ? `# Prove the entry belongs to this UID; the fault covers every writer, not socket roles.
entry_pids=$(lsof -nP -t -a -u "$uid" -iTCP:${applicationPort} -sTCP:LISTEN)
[ -n "$entry_pids" ] || { echo 'no owned application listener' >&2; exit 4; }
entry_identities=""
for pid in $entry_pids; do
  IFS= read -r stat < "/proc/$pid/stat"; rest=\${stat##*) }; set -- $rest
  entry_identities="$entry_identities $pid:\${20}"
done
invalid=0; frozen_targets=""; frozen_threads=""
stopped() {
  verify_pid=$1; verify_start=$2; threads=""
  IFS= read -r stat < "/proc/$verify_pid/stat" || return 1
  rest=\${stat##*) }; set -- $rest
  [ "\${20}" = "$verify_start" ] || return 1
  for task in /proc/$verify_pid/task/[0-9]*; do
    IFS= read -r stat < "$task/stat" || return 1
    rest=\${stat##*) }; set -- $rest
    case "$1" in Z|X) continue;; T|t) ;; *) return 1;; esac
    threads="$threads\${threads:+,}\${task##*/}"
  done
  [ -n "$threads" ]
}
freeze_writers() {
  scan
  for identity in $entry_identities; do
    case " $targets " in *" $identity "*) ;; *) return 1;; esac
  done
  freeze_attempt=0
  while [ "$freeze_attempt" -lt 50 ]; do
    sweep="$targets"
    for identity in $sweep; do
      pid=\${identity%:*}; started=\${identity#*:}
      IFS= read -r stat < "/proc/$pid/stat" || return 1
      rest=\${stat##*) }; set -- $rest
      [ "\${20}" = "$started" ] || return 1
      at=$(date +%s%3N) || return 1
      kill -STOP "$pid" || return 1
      echo "STOP $pid $started $at"
    done
    scan
    all_stopped=1; frozen_threads=""
    for identity in $targets; do
      if stopped "\${identity%:*}" "\${identity#*:}"; then
        frozen_threads="$frozen_threads $identity:$threads"
      else all_stopped=0; fi
    done
    verified="$targets"; scan
    for identity in $entry_identities; do
      case " $targets " in *" $identity "*) ;; *) return 1;; esac
    done
    if [ "$all_stopped" = 1 ] && [ "$sweep" = "$targets" ] && [ "$verified" = "$targets" ]; then
      frozen_targets="$targets"
      for row in $frozen_threads; do echo "FROZEN_PROCESS $row"; done
      echo "FROZEN $(date +%s%3N)"
      return 0
    fi
    freeze_attempt=$((freeze_attempt + 1)); sleep 0.1
  done
  return 1
}` : ''}
attempt=0
scan
echo ARMED
IFS= read -r trigger || exit 0
[ "$trigger" = CRASH ] || exit 4
${applicationPort ? `# Failure must still reach bounded KILL cleanup, including partially stopped writers.
if ! freeze_writers; then invalid=1; fi
scan` : ''}
while [ "$attempt" -lt 50 ]; do
  if [ -z "$targets" ]; then
    [ "$killed" -gt 0 ] || { echo 'no live process was crashed' >&2; exit 4; }
    echo QUIET
    ${applicationPort ? `[ "$invalid" = 0 ] || { echo 'application writer freeze was not verified' >&2; exit 4; }` : ''}
    exit 0
  fi
  for identity in $targets; do
    pid=\${identity%:*}; started=\${identity#*:}
    [ -r "/proc/$pid/stat" ] || continue
    IFS= read -r stat < "/proc/$pid/stat" || continue; rest=\${stat##*) }; set -- $rest
    [ "\${20}" = "$started" ] || { ${applicationPort ? 'invalid=1; continue' : "echo 'process identity changed before crash' >&2; exit 4"}; }
    ${applicationPort ? `case " $frozen_targets " in *" $identity "*)
      if stopped "$pid" "$started"; then
        case " $frozen_threads " in *" $identity:$threads "*) ;; *) invalid=1;; esac
      else invalid=1; fi;;
      *) invalid=1;;
    esac` : ''}
    at=$(date +%s%3N)
    if kill -KILL "$pid"; then echo "KILLED $pid $started $at"; killed=$((killed + 1)); fi
  done
  attempt=$((attempt + 1)); sleep 0.1; scan
done
echo 'writers remained after SIGKILL' >&2; exit 4`;
}

export async function prepareProcessCrash(lease: BackendLease, target: CrashTarget, applicationPort?: number) {
  const runtime = stackDatabaseRuntime(lease.backend);
  if (!runtime || !['application', 'database'].includes(target)) throw new Error('unsupported process crash boundary');
  if (runtime.combinedBoundary && target === 'application') {
    throw new Error(`${lease.backend} application and database share one boundary; use database`);
  }
  if (target === 'application' && (!Number.isInteger(applicationPort) || applicationPort! < 1 || applicationPort! > 65535)) {
    throw new Error('application crash requires its leased entry port');
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
    : runtime.databaseUser;
  // Start Docker exec before dispatch. Its shell waits for one stdin line, so
  // Docker startup latency is outside the request/fault window. EOF disarms it.
  let arm!: () => void, failArm!: (error: Error) => void;
  const armed = new Promise<void>((resolve, reject) => { arm = resolve; failArm = reject; });
  let child!: ReturnType<typeof execFile>;
  const completed = new Promise<string>((resolve, reject) => {
    child = execFile('docker', ['exec', '-i', '--user', user, container.id, 'sh', '-c',
      processCrashScript(target === 'database' ? runtime.processRecord : null, target === 'application' ? applicationPort : undefined)],
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
      if (target === 'application') {
        const startedAtMs = Number(receipt.processEvidence.match(/^STOP \d+ \d+ (\d+)$/m)?.[1]);
        const completedAtMs = Number(receipt.processEvidence.match(/^FROZEN (\d+)$/m)?.[1]);
        const processes = [...receipt.processEvidence.matchAll(/^FROZEN_PROCESS (\d+):(\d+):([\d,]+)$/gm)]
          .map(row => ({ pid: Number(row[1]), startTicks: Number(row[2]), threads: row[3]!.split(',').map(Number) }));
        const firstKill = Math.min(...[...receipt.processEvidence.matchAll(/^KILLED \d+ \d+ (\d+)$/gm)]
          .map(row => Number(row[1])));
        if (!(startedAtMs > 0 && completedAtMs >= startedAtMs && completedAtMs <= firstKill) || !processes.length
          || processes.some(row => !new RegExp(`^KILLED ${row.pid} ${row.startTicks} \\d+$`, 'm').test(receipt.processEvidence))) {
          throw new Error('application writer freeze receipt is incomplete');
        }
        receipt.applicationFreeze = { startedAtMs, completedAtMs, processes };
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
