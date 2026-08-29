export interface CapturedProcessLog {
  path: string;
  sha256: string;
  bytes: number;
  retainedBytes: number;
  truncated: boolean;
}

export interface BoundedProcessResult {
  ok: boolean;
  code: number | null;
  signal: NodeJS.Signals | null;
  timedOut: boolean;
  cancelled: boolean;
  error: Error | null;
  logs: Record<string, CapturedProcessLog> | null;
  stdoutTail: string;
  stderrTail: string;
}

export interface RunBoundedOptions {
  cwd: string;
  env: NodeJS.ProcessEnv;
  stdio?: 'inherit';
  timeoutMs: number;
  logs?: { stdout: string; stderr: string; maxBytes?: number } | null;
  signal?: AbortSignal | null;
}

export function runBounded(command: string, argv: string[],
  options: RunBoundedOptions): Promise<BoundedProcessResult>;
export function rescueSupervisedLease(path: string, output: string): void;
