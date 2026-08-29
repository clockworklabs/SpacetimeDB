interface CleanupContext {
  readonly tracing: { stop(options?: unknown): Promise<void> };
  close(): Promise<void>;
}

interface CleanupVideo {
  delete(): Promise<void>;
  saveAs(path: string): Promise<void>;
}

interface CleanupEntry {
  readonly context: CleanupContext;
  readonly name: string;
  readonly page: { video(): CleanupVideo | null };
}

interface CleanupFailure {
  readonly actor: string;
  readonly message: string;
  readonly stage: string;
}

export function closeActorContexts(
  entries: readonly CleanupEntry[],
  options: {
    readonly media: string;
    readonly slug: string;
    readonly trace: boolean;
  },
): Promise<readonly CleanupFailure[]>;
