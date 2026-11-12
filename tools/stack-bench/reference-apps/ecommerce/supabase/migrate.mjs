// Applies each migration in supabase/migrations once, in file order, and records it.
import { readdirSync, readFileSync } from 'node:fs';
import pg from 'pg';

const directory = new URL('./supabase/migrations/', import.meta.url);
const client = new pg.Client({ connectionString: process.env.SUPABASE_DB_URL });
await client.connect();
try {
  await client.query(`create schema if not exists supabase_migrations;
    create table if not exists supabase_migrations.schema_migrations (version text primary key, name text not null)`);
  const applied = new Set((await client.query('select version from supabase_migrations.schema_migrations')).rows
    .map(row => row.version));
  for (const file of readdirSync(directory).filter(name => name.endsWith('.sql')).sort()) {
    const [version, ...name] = file.replace(/\.sql$/, '').split('_');
    if (applied.has(version)) continue;
    await client.query('begin');
    try {
      await client.query(readFileSync(new URL(file, directory), 'utf8'));
      await client.query('insert into supabase_migrations.schema_migrations (version, name) values ($1, $2)',
        [version, name.join('_')]);
      await client.query('commit');
    } catch (error) {
      await client.query('rollback');
      throw new Error(`migration ${file} failed: ${error.message}`);
    }
  }
  // PostgREST serves new functions once it reloads its schema cache.
  await client.query("notify pgrst, 'reload schema'");
} finally {
  await client.end();
}
