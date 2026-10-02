// Seeds the catalog when the store is empty and creates the provided accounts through Supabase Auth.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import pg from 'pg';

const ROLES = { admin: ['admin'], staff: ['staff'], customer: [] };

// Same derivations as client/src/request.ts.
const accountEmail = username => `${Buffer.from(username, 'utf8').toString('hex')}@accounts.invalid`;
const accountPassword = password => createHash('sha256').update(password, 'utf8').digest('hex');

const client = new pg.Client({ connectionString: process.env.SUPABASE_DB_URL });
await client.connect();
try {
  await client.query(readFileSync(new URL('./supabase/seed.sql', import.meta.url), 'utf8'));
  const existing = new Set((await client.query('select username from public.order_account where username = any($1)',
    [Object.keys(ROLES)])).rows.map(row => row.username));
  for (const [username, roles] of Object.entries(ROLES)) {
    if (existing.has(username)) continue;
    const response = await fetch(`${process.env.SUPABASE_URL}/auth/v1/signup`, {
      method: 'POST',
      headers: { apikey: process.env.SUPABASE_ANON_KEY, 'content-type': 'application/json' },
      body: JSON.stringify({ email: accountEmail(username),
        password: accountPassword(`stackbench-${username}-2026`) }),
    });
    if (!response.ok) throw new Error(`Seed account ${username} failed: ${response.status} ${await response.text()}`);
    await client.query('update public.order_account set roles = $2 where username = $1', [username, roles]);
  }
} finally {
  await client.end();
}
