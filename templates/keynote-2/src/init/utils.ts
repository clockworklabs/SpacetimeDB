import { spawn } from 'node:child_process';
import pg from 'pg';
import { setTimeout as sleep } from 'timers/promises';

export const ACC = Number(process.env.SEED_ACCOUNTS ?? 100_000);
export const BAL = Number(process.env.SEED_INITIAL_BALANCE ?? 1_000_000);

export function has(v?: string) {
  return v && v.trim().length > 0;
}

export function sh(
  cmd: string,
  args: string[],
  opts: import('node:child_process').SpawnOptions = {},
): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, {
      stdio: 'inherit',
      ...opts,
    });

    child.on('error', reject);

    child.on('close', (code, signal) => {
      if (code === 0) return resolve();
      reject(
        new Error(
          `${cmd} ${args.join(' ')} failed` +
            (code !== null ? ` with exit code ${code}` : '') +
            (signal ? ` (signal ${signal})` : ''),
        ),
      );
    });
  });
}

export async function waitFor(host: string, tries = 30) {
  for (let i = 0; i < tries; i++) {
    try {
      const url = new URL(host);
      if (url.protocol.startsWith('postgres')) {
        const c = new pg.Client({ connectionString: host });
        await c.connect();
        await c.end();
        return;
      }
    } catch {}
    await sleep(1000);
  }
  throw new Error(`Timed out waiting for ${host}`);
}

export function spawnServer(command: string, name: string) {
  const child = spawn(command, {
    stdio: 'inherit',
    shell: true,
    env: process.env,
  });

  console.log(`[rpc] started ${name} (pid=${child.pid})`);
  child.on('error', (err) => {
    console.error(`[rpc] ${name} error:`, err);
  });

  // child.unref();
}
