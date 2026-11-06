import { execFile } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { promisify } from 'node:util';

const RENEWAL_TIMEOUT_MS = 2 * 60_000;

// Google's CLI renews a sign-in before any command, but only once its token has expired.
// This runs it on a private copy of the sign-in marked expired, in its own container
// outside every agent's sandbox, and returns the file it leaves. The token it replaces
// stays valid until its own expiry. `directory` must be at the same path on the Docker host.
export function agyRenewal(image: string, directory: string): (text: string) => Promise<string> {
  return async text => {
    const home = mkdtempSync(join(directory, 'stack-bench-agy-renewal-'));
    try {
      const state = join(home, '.gemini', 'antigravity-cli');
      mkdirSync(state, { recursive: true, mode: 0o700 });
      const login = JSON.parse(text) as { token: { expiry: string } };
      login.token.expiry = '2000-01-01T00:00:00Z';
      writeFileSync(join(state, 'antigravity-oauth-token'), JSON.stringify(login), { mode: 0o600 });
      writeFileSync(join(state, 'settings.json'), '{"enableTelemetry":false}\n', { mode: 0o600 });
      // The model list names the account's plan; none of the output is kept.
      try {
        await promisify(execFile)('docker', ['run', '--rm', '--name', `sb-agy-renewal-${randomBytes(6).toString('hex')}`,
          '--user', `${process.getuid?.() ?? 0}:${process.getgid?.() ?? 0}`, '--cap-drop', 'ALL',
          '--security-opt', 'no-new-privileges:true', '--mount', `type=bind,src=${home},dst=/agy`, '-e', 'HOME=/agy',
          '-w', '/agy', image, 'agy', 'models'], { timeout: RENEWAL_TIMEOUT_MS, maxBuffer: 16 * 1024 * 1024 });
      } catch { throw new Error('agy could not renew the Antigravity sign-in; sign in with agy again'); }
      return readFileSync(join(state, 'antigravity-oauth-token'), 'utf8');
    } finally { rmSync(home, { recursive: true, force: true }); }
  };
}
