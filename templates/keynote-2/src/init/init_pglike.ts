import { ACC, BAL, waitFor } from './utils.ts';
import pg from 'pg';

export async function initPgLike(url: string, label: string) {
  const isPlanetScale = url.includes('psdb.cloud');

  console.log(`\n[${label}] init @ ${url}`);
  await waitFor(url);

  const client = new pg.Client({
    connectionString: url,
    ssl: isPlanetScale ? { rejectUnauthorized: false } : undefined,
  });

  await client.connect();

  await client.query(`
    CREATE TABLE IF NOT EXISTS accounts (
                                          id INT PRIMARY KEY,
                                          balance BIGINT NOT NULL
    );
  `);

  // reset
  await client.query(`TRUNCATE TABLE accounts;`);

  // seed
  await client.query(
    `
      INSERT INTO accounts(id, balance)
      SELECT g, $1 FROM generate_series(0, $2) AS g;
    `,
    [BAL, ACC - 1],
  );

  await client.end();
  console.log(`[${label}] ready`);
}
