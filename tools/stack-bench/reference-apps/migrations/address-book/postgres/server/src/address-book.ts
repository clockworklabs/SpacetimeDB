import type { Express, RequestHandler } from 'express';
import type { Pool, PoolClient } from 'pg';

export async function initializeAddressBook(pool: Pool) {
  const client = await pool.connect();
  try {
    await client.query('BEGIN');
    await client.query(`
      CREATE TABLE IF NOT EXISTS address_book_owner (
        account_id integer PRIMARY KEY REFERENCES account(id) ON DELETE CASCADE
      );
      CREATE TABLE IF NOT EXISTS address_entry (
        id serial PRIMARY KEY,
        account_id integer NOT NULL REFERENCES account(id) ON DELETE CASCADE,
        name text NOT NULL, address text NOT NULL,
        is_default boolean NOT NULL DEFAULT false
      );
      CREATE UNIQUE INDEX IF NOT EXISTS one_default_address ON address_entry(account_id) WHERE is_default;
      WITH imported AS (
        INSERT INTO address_book_owner(account_id) SELECT id FROM account
        ON CONFLICT DO NOTHING RETURNING account_id
      )
      INSERT INTO address_entry(account_id, name, address, is_default)
      SELECT a.id, a.profile_name, a.profile_address, true FROM account a
      JOIN imported m ON m.account_id = a.id
      WHERE a.profile_name <> '' OR a.profile_address <> '';
    `);
    await client.query('COMMIT');
  } catch (error) { await client.query('ROLLBACK'); throw error; }
  finally { client.release(); }
}

async function withBook<T>(pool: Pool, accountId: number, work: (client: PoolClient) => Promise<T>) {
  const client = await pool.connect();
  try {
    await client.query('BEGIN');
    const account = await client.query('SELECT * FROM account WHERE id = $1 FOR UPDATE', [accountId]);
    if (account.rows.length !== 1) throw new Error('Account missing');
    const imported = await client.query(
      'INSERT INTO address_book_owner(account_id) VALUES ($1) ON CONFLICT DO NOTHING RETURNING account_id', [accountId]);
    if (imported.rows.length && (account.rows[0].profile_name !== '' || account.rows[0].profile_address !== '')) {
      await client.query('INSERT INTO address_entry(account_id,name,address,is_default) VALUES ($1,$2,$3,true)',
        [accountId, account.rows[0].profile_name, account.rows[0].profile_address]);
    }
    const result = await work(client);
    await client.query('COMMIT');
    return result;
  } catch (error) { await client.query('ROLLBACK'); throw error; }
  finally { client.release(); }
}

function text(name: unknown, address: unknown) {
  if (typeof name !== 'string' || typeof address !== 'string') throw new Error('Name and address must be text');
  return [name, address];
}

async function syncProfile(client: PoolClient, accountId: number) {
  const result = await client.query('SELECT name,address FROM address_entry WHERE account_id=$1 AND is_default', [accountId]);
  await client.query('UPDATE account SET profile_name=$2, profile_address=$3 WHERE id=$1',
    [accountId, result.rows[0]?.name ?? '', result.rows[0]?.address ?? '']);
}

export async function saveDefaultAddress(pool: Pool, accountId: number, name: string, address: string) {
  text(name, address);
  await withBook(pool, accountId, async client => {
    const updated = await client.query('UPDATE address_entry SET name=$2,address=$3 WHERE account_id=$1 AND is_default RETURNING id',
      [accountId, name, address]);
    if (!updated.rows.length) await client.query('INSERT INTO address_entry(account_id,name,address,is_default) VALUES ($1,$2,$3,true)',
      [accountId, name, address]);
    await syncProfile(client, accountId);
  });
}

export function registerAddressBook(app: Express, pool: Pool, auth: RequestHandler, changed: () => Promise<void>) {
  const route = (method: 'get' | 'post' | 'put' | 'delete', path: string,
    work: (client: PoolClient, accountId: number, id: string, body: any) => Promise<unknown>) => {
    app[method](path, auth, async (req, res, next) => {
      try {
        const result = await withBook(pool, req.account!.id,
          client => work(client, req.account!.id, String(req.params.id ?? ''), req.body));
        if (method !== 'get') await changed();
        res.json(result);
      } catch (error) {
        if (error instanceof Error && ['Address missing', 'Choose another default first', 'Name and address must be text'].includes(error.message)) {
          res.status(error.message === 'Address missing' ? 404 : 400).json({ error: error.message });
        } else next(error);
      }
    });
  };
  const entries = async (client: PoolClient, accountId: number) => (await client.query(
    'SELECT id::text, name, address, is_default AS "isDefault" FROM address_entry WHERE account_id=$1 ORDER BY id', [accountId])).rows;
  route('get', '/api/addresses', async (client, accountId) => ({ entries: await entries(client, accountId) }));
  route('get', '/api/addresses/:id', async (client, accountId, id) => {
    const entry = (await entries(client, accountId)).find(row => row.id === id);
    if (!entry) throw new Error('Address missing');
    return entry;
  });
  route('post', '/api/addresses', async (client, accountId, _id, body) => {
    const [name, address] = text(body?.name, body?.address);
    const result = await client.query(`INSERT INTO address_entry(account_id,name,address,is_default)
      VALUES ($1,$2,$3,NOT EXISTS(SELECT 1 FROM address_entry WHERE account_id=$1)) RETURNING id::text`, [accountId, name, address]);
    await syncProfile(client, accountId);
    return result.rows[0];
  });
  for (const operation of ['edit', 'default', 'delete'] as const) {
    route(operation === 'delete' ? 'delete' : 'put', `/api/addresses/:id${operation === 'default' ? '/default' : ''}`,
      async (client, accountId, id, body) => {
        const found = (await entries(client, accountId)).find(row => row.id === id);
        if (!found) throw new Error('Address missing');
        if (operation === 'edit') {
          const [name, address] = text(body?.name, body?.address);
          await client.query('UPDATE address_entry SET name=$3,address=$4 WHERE account_id=$1 AND id::text=$2', [accountId, id, name, address]);
        } else if (operation === 'default') {
          await client.query('UPDATE address_entry SET is_default=false WHERE account_id=$1 AND is_default', [accountId]);
          await client.query('UPDATE address_entry SET is_default=true WHERE account_id=$1 AND id::text=$2', [accountId, id]);
        } else {
          if (found.isDefault && (await entries(client, accountId)).length > 1) throw new Error('Choose another default first');
          await client.query('DELETE FROM address_entry WHERE account_id=$1 AND id::text=$2', [accountId, id]);
        }
        await syncProfile(client, accountId);
        return { ok: true };
      });
  }
}
