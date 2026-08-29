export function controlBackend(
  restartSpec: unknown,
  mode: 'restart' | 'start' | 'stop',
  options: { readonly signal: AbortSignal },
): Promise<unknown>;
