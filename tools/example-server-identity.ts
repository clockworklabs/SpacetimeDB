import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs';

export type StoredServerToken = {
  token: string | undefined;
  source: 'environment' | 'file' | 'none';
};

export function loadServerToken(
  tokenPath: string,
  environmentToken: string | undefined
): StoredServerToken {
  const explicit = environmentToken?.trim();
  if (explicit) return { token: explicit, source: 'environment' };
  if (!existsSync(tokenPath)) return { token: undefined, source: 'none' };

  const stored = readFileSync(tokenPath, 'utf8').trim();
  return stored
    ? { token: stored, source: 'file' }
    : { token: undefined, source: 'none' };
}

export function saveServerToken(tokenPath: string, token: string): void {
  writeFileSync(tokenPath, `${token.trim()}\n`, {
    encoding: 'utf8',
    mode: 0o600,
  });
}

export function discardStoredServerToken(tokenPath: string): void {
  if (existsSync(tokenPath)) unlinkSync(tokenPath);
}

export function grantServerIdentity(options: {
  spacetimeBin?: string;
  server: string;
  database: string;
  procedure: string;
  identity: string;
}): void {
  const spacetimeBin = options.spacetimeBin ?? 'spacetime';
  const result = spawnSync(
    spacetimeBin,
    [
      'call',
      '--server',
      options.server,
      options.database,
      options.procedure,
      JSON.stringify(options.identity),
    ],
    { encoding: 'utf8', shell: false }
  );

  if (result.error) {
    throw new Error(`failed to run ${spacetimeBin}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const detail = result.stderr.trim() || result.stdout.trim();
    throw new Error(
      [
        `could not authorize the example server identity with ${options.procedure}`,
        detail || `${spacetimeBin} exited ${result.status}`,
        'Publish the database with the currently logged-in CLI identity, then restart the example.',
      ].join(': ')
    );
  }
}
